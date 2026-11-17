//! Physical attempts consume the domain-neutral authorization from T19. No HQ
//! command, renewed operation clock, or independent retry budget is created.
use super::commands::Request;
use super::config::ExConfig;
use super::control::{connection, Control};
use super::frame::{Association, Frame, FrameAccumulator, FrameLimits};
use super::query::BarsRangeQuery;
use super::session::{Connection, Session};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use bytes::Bytes;
use eltdx_runtime::execution::AttemptAuthorization;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

#[derive(Clone, Debug)]
pub(crate) enum Work {
    Connect,
    Request(Request),
    Range(BarsRangeQuery),
    /// Already checked profile command bytes, admitted only through the same
    /// owner/operation worker. This is not a public raw socket bypass.
    Encoded(Bytes),
    /// An existing exact connection is required. No reconnect or failover is
    /// permitted after a source-sensitive composition accepted its first reply.
    PinnedEncoded {
        wire: Bytes,
        source: (SocketAddr, u64),
    },
}
pub(crate) struct WireReply {
    pub frame: Option<Frame>,
    pub endpoint: SocketAddr,
    pub generation: u64,
}

pub(crate) async fn execute(
    session: &Arc<Session>,
    config: &ExConfig,
    work: &Work,
    maximum_attempts: u32,
    authorization: &Arc<dyn AttemptAuthorization>,
    control: &Control,
) -> Result<(WireReply, Connection)> {
    control.check(TimeoutPhase::Startup)?;
    let mut existing = session.take()?;
    let pinned = match work {
        Work::PinnedEncoded { source, .. } => Some(*source),
        _ => None,
    };
    if let Some(source) = pinned {
        if existing
            .as_ref()
            .is_none_or(|connection| (connection.endpoint, connection.generation) != source)
        {
            return Err(Error::new(
                ErrorKind::Protocol,
                "pinned extension source is no longer the established connection",
            )
            .with_context("reason", "SourceChanged"));
        }
    }
    // A failed pinned request retires this socket and aborts the composition.
    // Reconnecting the same host would create a different unverified snapshot.
    let maximum_attempts = if pinned.is_some() {
        1
    } else {
        maximum_attempts
    };
    if matches!(work, Work::Connect) {
        if let Some(ready) = existing.take() {
            return Ok((
                WireReply {
                    frame: None,
                    endpoint: ready.endpoint,
                    generation: ready.generation,
                },
                ready,
            ));
        }
    }
    let candidates = session.candidates()?;
    let mut endpoint = existing
        .as_ref()
        .map(|item| item.endpoint)
        .or_else(|| candidates.first().copied())
        .ok_or_else(|| super::invalid("EX has no candidate endpoint"))?;
    let mut tried = Vec::new();
    let mut previous: Option<Error> = None;
    for number in 1..=maximum_attempts {
        control.check(if number == 1 {
            TimeoutPhase::Connect
        } else {
            TimeoutPhase::Retry
        })?;
        let failure = previous.clone().map(crate::policy::retries::to_runtime);
        let reserved = authorization
            .reserve(number, failure.as_ref())
            .map_err(|error| {
                let error = Error::from_runtime(error);
                match &previous {
                    Some(root) => error.with_source(root.clone()),
                    None => error,
                }
            })?;
        reserved
            .start_before(control.deadline)
            .map_err(Error::from_runtime)?;
        let result = attempt(session, config, work, endpoint, existing.take(), control).await;
        match result {
            Ok(reply) => return Ok(reply),
            Err(error) => {
                // attempt owns its socket and has already dropped it here,
                // including on a partial write/read, before a new reservation.
                let error = error.with_context("endpoint", format!("{endpoint}"));
                if error.kind() != ErrorKind::Connection {
                    return Err(error);
                }
                session.failed(endpoint)?;
                tried.push(endpoint);
                if number == maximum_attempts {
                    return Err(error);
                }
                previous = Some(error);
                if config.auto_failover() {
                    endpoint = candidates
                        .iter()
                        .find(|candidate| !tried.contains(candidate))
                        .copied()
                        .unwrap_or(endpoint);
                }
            }
        }
    }
    Err(Error::new(
        ErrorKind::Internal,
        "EX attempt maximum must be positive",
    ))
}

async fn attempt(
    session: &Session,
    config: &ExConfig,
    work: &Work,
    endpoint: SocketAddr,
    existing: Option<Connection>,
    control: &Control,
) -> Result<(WireReply, Connection)> {
    // Declare the sequence before the socket. On every early return the socket
    // is destroyed before this lease is released; on success its full response
    // has been consumed. Each owner has a separate 1..255 sequence allocator.
    let sequence = if matches!(work, Work::Range(_)) {
        Some(session.sequences.acquire()?)
    } else {
        None
    };
    let was_ready = existing.is_some();
    let (mut stream, generation) = if let Some(existing) = existing {
        // Observe already pending unsolicited bytes before writing another
        // command. A previous failed exchange is never reused at all.
        let mut pending = [0_u8; 1];
        match existing.stream.peek(&mut pending) {
            Ok(0) => return Err(Error::new(ErrorKind::Connection, "EX idle peer has closed")),
            Ok(_) => {
                return Err(super::protocol(
                    "unsolicited EX bytes precede the next request",
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(connection(error)),
        }
        let stream = TcpStream::from_std(existing.stream).map_err(connection)?;
        (stream, existing.generation)
    } else {
        let stream = control
            .connect(async { TcpStream::connect(endpoint).await.map_err(connection) })
            .await?;
        stream.set_nodelay(true).map_err(connection)?;
        (stream, session.next_generation()?)
    };
    if !was_ready {
        // A connection becomes Ready only after the entire setup response,
        // including decompression, validates. Never leave its body unread.
        session
            .profile
            .negotiate(&mut stream, config.frame_limits(), control)
            .await
            .map_err(|error| error.with_context("profile", session.profile.name()))?;
    }
    let request = match work {
        Work::Connect => None,
        Work::Request(request) => Some(request.wire().clone()),
        Work::Range(query) => Some(
            super::commands::bars_range_request(
                query,
                sequence.as_ref().ok_or_else(|| {
                    Error::new(ErrorKind::Internal, "range request lacks sequence lease")
                })?,
            )
            .wire()
            .clone(),
        ),
        Work::Encoded(wire) | Work::PinnedEncoded { wire, .. } => Some(wire.clone()),
    };
    let frame = if let Some(request) = request {
        write(&mut stream, &request, TimeoutPhase::Send, control).await?;
        let frame = read_frame(
            &mut stream,
            config.frame_limits(),
            TimeoutPhase::Response,
            control,
        )
        .await?;
        control.check(TimeoutPhase::Decode)?;
        Some(frame)
    } else {
        None
    };
    control.check(TimeoutPhase::Response)?;
    let stream = stream.into_std().map_err(connection)?;
    let connection = Connection {
        stream,
        endpoint,
        generation,
    };
    Ok((
        WireReply {
            frame,
            endpoint,
            generation,
        },
        connection,
    ))
}

pub(crate) async fn write(
    stream: &mut TcpStream,
    wire: &[u8],
    phase: TimeoutPhase,
    control: &Control,
) -> Result<()> {
    control
        .wait(phase, async {
            stream.write_all(wire).await.map_err(connection)
        })
        .await
}
pub(crate) async fn read_frame(
    stream: &mut TcpStream,
    limits: FrameLimits,
    phase: TimeoutPhase,
    control: &Control,
) -> Result<Frame> {
    read_frame_inner(stream, limits, phase, control)
        .await
        .map_err(|error| super::control::phase_error(error, phase))
}
async fn read_frame_inner(
    stream: &mut TcpStream,
    limits: FrameLimits,
    phase: TimeoutPhase,
    control: &Control,
) -> Result<Frame> {
    let mut accumulator = FrameAccumulator::new(limits, Association::default());
    let mut received = 0_usize;
    let mut decode_charged = false;
    while accumulator.remaining() != 0 {
        // Exact remaining-byte reads never consume the beginning of another
        // frame, even when the peer coalesces its writes into one TCP segment.
        let mut buffer = [0_u8; 4096];
        let allowance = control.sponsor.remaining_bytes()?;
        if allowance == 0 {
            // This deliberately fails the existing limit gate before another
            // byte is read; it never increments a separate local budget.
            control.sponsor.record_bytes(1)?;
        }
        let maximum = buffer.len().min(accumulator.remaining()).min(allowance);
        let length = control
            .wait(phase, async {
                stream
                    .read(&mut buffer[..maximum])
                    .await
                    .map_err(connection)
            })
            .await?;
        if length == 0 {
            return Err(if received == 0 {
                Error::new(ErrorKind::Connection, "EX peer closed before a response")
            } else {
                Error::new(
                    ErrorKind::Connection,
                    "extension peer closed inside a declared frame",
                )
                .with_context("received_bytes", format!("{received}"))
                .with_source(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection ended before the declared frame was received",
                ))
            });
        }
        control.sponsor.record_bytes(length)?;
        let consumed = accumulator.push(&buffer[..length])?;
        if consumed != length {
            return Err(super::protocol("EX exact receive crossed a frame boundary"));
        }
        received += length;
        if !decode_charged {
            if let Some(header) = accumulator.header() {
                // Charge the declared decoded bytes before body acquisition
                // and bounded decompression allocate their output buffer.
                control
                    .sponsor
                    .record_bytes(usize::from(header.decoded_len()))?;
                decode_charged = true;
            }
        }
    }
    let decode_phase = if phase == TimeoutPhase::Handshake {
        phase
    } else {
        TimeoutPhase::Decode
    };
    control.check(decode_phase)?;
    let frame = accumulator
        .finish()
        .map_err(|error| super::control::phase_error(error, decode_phase))?;
    Ok(frame)
}
