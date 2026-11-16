use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use eltdx_protocol::commands::security::SecurityCountRequest;
use eltdx_protocol::frame::{RequestFrame, RESPONSE_PREFIX};
use eltdx_protocol::unit::Market;
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::endpoint::Endpoint;
use eltdx_runtime::engine::{Engine, EngineConfig, EngineConfigOptions, PendingPoll};
use eltdx_runtime::execution::SubmissionOptions;
use eltdx_runtime::RuntimeError;

fn runtime_io_error(error: io::Error) -> RuntimeError {
    RuntimeError::internal(format!("{error}"))
}

fn engine_config(address: String, max_pending_requests: usize) -> Result<EngineConfig, RuntimeError> {
    EngineConfig::from_endpoints(
        vec![Endpoint::numeric(&address)?],
        Duration::from_secs(10),
        EngineConfigOptions {
            pool_size: 1,
            runtime_workers: Some(1),
            server_count: 1,
            max_connections_per_host: None,
            connect_concurrency: None,
            connect_concurrency_per_host: None,
            global_raw_bytes: None,
            global_decoded_bytes: None,
            heartbeat_interval: None,
            max_pending_requests,
            push_queue_size: 8,
            push_queue_bytes: 1024,
        },
    )
}

fn request() -> CommandRequest {
    CommandRequest::SecurityCount(SecurityCountRequest {
        market: Market::Shanghai,
        client_date: 0,
    })
}

fn read_request(stream: &mut TcpStream) -> io::Result<RequestFrame> {
    let mut raw = vec![0_u8; 12];
    stream.read_exact(&mut raw)?;
    let encoded_length = usize::from(u16::from_le_bytes([raw[6], raw[7]]));
    let body_length = encoded_length.checked_sub(2).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "request body length is invalid")
    })?;
    raw.resize(12 + body_length, 0);
    stream.read_exact(&mut raw[12..])?;
    RequestFrame::decode(&raw).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn response(msg_id: u32, msg_type: u16, payload: &[u8]) -> Vec<u8> {
    debug_assert!(payload.len() <= usize::from(u16::MAX));
    let length = payload.len() as u16;
    let mut raw = Vec::with_capacity(16 + payload.len());
    raw.extend_from_slice(&RESPONSE_PREFIX);
    raw.push(1);
    raw.extend_from_slice(&msg_id.to_le_bytes());
    raw.push(0);
    raw.extend_from_slice(&msg_type.to_le_bytes());
    raw.extend_from_slice(&length.to_le_bytes());
    raw.extend_from_slice(&length.to_le_bytes());
    raw.extend_from_slice(payload);
    raw
}

fn serve_handshake(stream: &mut TcpStream) -> io::Result<()> {
    let handshake = read_request(stream)?;
    stream.write_all(&response(
        handshake.msg_id,
        handshake.msg_type,
        &[0; 189],
    ))
}

fn join_server(server: thread::JoinHandle<io::Result<()>>) -> Result<(), RuntimeError> {
    server
        .join()
        .map_err(|_| RuntimeError::internal("loopback server thread panicked"))?
        .map_err(runtime_io_error)
}

#[test]
fn elapsed_deadline_does_not_start_or_connect_the_engine() -> Result<(), RuntimeError> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(runtime_io_error)?;
    listener.set_nonblocking(true).map_err(runtime_io_error)?;
    let address = listener.local_addr().map_err(runtime_io_error)?.to_string();
    let engine = Engine::new(engine_config(address, 2)?)?;
    let options = SubmissionOptions::new(Instant::now());

    let error = engine
        .begin_execute_raw_with_options(request(), options)
        .err()
        .ok_or_else(|| RuntimeError::internal("an elapsed deadline was accepted"))?;

    assert!(matches!(error, RuntimeError::Timeout { .. }));
    assert!(engine.begin_pin_with_options(options).is_err());
    assert!(engine.begin_connect_with_options(options).is_err());
    assert!(matches!(listener.accept(), Err(error) if error.kind() == io::ErrorKind::WouldBlock));
    assert_eq!(engine.pool_diagnostics()?.state.as_str(), "STOPPED");
    engine.close()?;
    Ok(())
}

#[test]
fn raw_execution_returns_frame_and_wire_context_without_typed_parsing(
) -> Result<(), RuntimeError> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(runtime_io_error)?;
    let address = listener.local_addr().map_err(runtime_io_error)?.to_string();
    let (request_tx, request_rx) = mpsc::sync_channel(1);
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        serve_handshake(&mut stream)?;
        let received = read_request(&mut stream)?;
        request_tx.send(received.clone()).map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "test stopped observing request")
        })?;
        stream.write_all(&response(received.msg_id, received.msg_type, &[]))?;
        let typed = read_request(&mut stream)?;
        stream.write_all(&response(typed.msg_id, typed.msg_type, &[7, 0]))?;
        let malformed = read_request(&mut stream)?;
        stream.write_all(&response(malformed.msg_id, malformed.msg_type, &[]))?;
        Ok(())
    });

    let engine = Engine::new(engine_config(address.clone(), 2)?)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let reply = engine.execute_raw_with_options(request(), SubmissionOptions::new(deadline))?;
    let sent = request_rx
        .recv()
        .map_err(|_| RuntimeError::internal("loopback server did not publish the request"))?;

    assert_eq!(reply.request.command_code(), sent.msg_type);
    assert_eq!(reply.response.msg_id, sent.msg_id);
    assert!(reply.response.data.is_empty());
    assert!(!reply.response.raw.is_empty());
    assert_eq!(reply.host, address);
    assert_eq!(reply.slot_id.get(), 0);
    assert_ne!(reply.request_id.get(), 0);
    assert_ne!(reply.engine_epoch.get(), 0);
    assert_ne!(reply.generation.get(), 0);
    drop(reply);
    assert_eq!(engine.execute(request())?, CommandResponse::SecurityCount(7));
    assert!(matches!(
        engine.execute(request()),
        Err(RuntimeError::Protocol { .. })
    ));
    let broker = engine
        .pool_diagnostics()?
        .broker
        .ok_or_else(|| RuntimeError::internal("runtime did not publish its broker snapshot"))?;
    assert_eq!(broker.active_leases, 0);

    engine.close()?;
    join_server(server)?;
    Ok(())
}

#[test]
fn queued_request_expires_against_its_submission_deadline() -> Result<(), RuntimeError> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(runtime_io_error)?;
    let address = listener.local_addr().map_err(runtime_io_error)?.to_string();
    let (first_request_tx, first_request_rx) = mpsc::sync_channel(1);
    let (release_first_tx, release_first_rx) = mpsc::sync_channel(1);
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut stream, _) = listener.accept()?;
        serve_handshake(&mut stream)?;
        let first = read_request(&mut stream)?;
        first_request_tx.send(()).map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "test stopped observing request")
        })?;
        release_first_rx.recv().map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "test did not release first response")
        })?;
        stream.write_all(&response(first.msg_id, first.msg_type, &[1, 0]))?;
        Ok(())
    });

    let engine = Engine::new(engine_config(address, 2)?)?;
    let first = engine.begin_execute_raw_with_options(
        request(),
        SubmissionOptions::new(Instant::now() + Duration::from_secs(5)),
    )?;
    first_request_rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| RuntimeError::internal("first request did not reach the server"))?;

    let second_deadline = Instant::now() + Duration::from_millis(250);
    let mut second = engine.begin_execute_raw_with_options(
        request(),
        SubmissionOptions::new(second_deadline),
    )?;
    let second_error = match second.wait_timeout(Duration::from_secs(2)) {
        Err(error @ RuntimeError::Timeout { .. }) => error,
        Err(error) => return Err(error),
        Ok(PendingPoll::Ready(_)) => {
            return Err(RuntimeError::internal(
                "queued request completed before its only Slot was released",
            ));
        }
        Ok(PendingPoll::Pending) => {
            return Err(RuntimeError::internal(
                "queued request outlived its absolute submission deadline",
            ));
        }
    };
    assert!(matches!(second_error, RuntimeError::Timeout { .. }));

    release_first_tx
        .send(())
        .map_err(|_| RuntimeError::internal("loopback server stopped before first response"))?;
    assert_eq!(first.wait()?.response.data.as_ref(), [1, 0]);
    engine.close()?;
    join_server(server)?;
    Ok(())
}

#[test]
fn response_wait_and_retry_share_the_absolute_deadline() -> Result<(), RuntimeError> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(runtime_io_error)?;
    let address = listener.local_addr().map_err(runtime_io_error)?.to_string();
    let deadline = Instant::now() + Duration::from_millis(500);
    let (request_tx, request_rx) = mpsc::sync_channel(2);
    let server = thread::spawn(move || -> io::Result<()> {
        let (mut first, _) = listener.accept()?;
        serve_handshake(&mut first)?;
        let _ = read_request(&mut first)?;
        request_tx.send(()).map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "test stopped observing request")
        })?;
        drop(first);

        let (mut second, _) = listener.accept()?;
        serve_handshake(&mut second)?;
        let _ = read_request(&mut second)?;
        request_tx.send(()).map_err(|_| {
            io::Error::new(io::ErrorKind::BrokenPipe, "test stopped observing retry")
        })?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining + Duration::from_millis(50));
        Ok(())
    });

    let engine = Engine::new(engine_config(address, 2)?)?;
    let mut pending = engine.begin_execute_raw_with_options(
        request(),
        SubmissionOptions::new(deadline),
    )?;
    request_rx
        .recv_timeout(Duration::from_secs(3))
        .map_err(|_| RuntimeError::internal("business request did not reach the server"))?;
    request_rx
        .recv_timeout(Duration::from_secs(3))
        .map_err(|_| RuntimeError::internal("retry request did not reach the server"))?;

    let timeout = match pending.wait_timeout(Duration::from_secs(2)) {
        Err(error @ RuntimeError::Timeout { .. }) => error,
        Err(error) => return Err(error),
        Ok(PendingPoll::Ready(_)) => {
            return Err(RuntimeError::internal(
                "server returned a reply despite holding it through the deadline",
            ));
        }
        Ok(PendingPoll::Pending) => {
            return Err(RuntimeError::internal(
                "response wait or retry extended the submission deadline",
            ));
        }
    };
    assert!(matches!(timeout, RuntimeError::Timeout { .. }));

    engine.close()?;
    join_server(server)?;
    Ok(())
}
