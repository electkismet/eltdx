use std::time::{Duration, Instant};

use eltdx_protocol::commands::security::SecurityListRequest;
use eltdx_protocol::context::PricePrecisionSource;
use eltdx_protocol::unit::{Market, NormalizedCode};
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::{Engine, PendingPoll, PendingRawExecution, SIGNAL_POLL_INTERVAL};
use eltdx_runtime::execution::{RawReply, SubmissionOptions};

use super::{FlightControl, MarketCandidate, MetadataRecord, PageEvidence, PrecisionMetadata};
use crate::error::{Error, ErrorKind, Result};
use crate::types::{HqMarket, RawPolicy, SecurityId, ValueProvenance};

struct NoPrecision;
impl PricePrecisionSource for NoPrecision {
    fn security_decimal(&self, _: &NormalizedCode) -> Option<u8> { None }
}

pub(super) fn load_market(engine: &Engine, market: HqMarket, request_timeout: Duration, control: &FlightControl) -> Result<MarketCandidate> {
    let candidate = MarketCandidate::with_limits(control.key().clone(), control.catalog_limits())?;
    scan_market(candidate, market, || control.check_active(), |request| {
        let options = match control.source_deadline() {
            Some(deadline) => SubmissionOptions::new(deadline),
            None => SubmissionOptions::legacy_after_startup(request_timeout),
        }.with_attempt_authorization(control.retry_policy(&request)?, control.attempt_authorization());
        let mut pending = engine.begin_execute_raw_with_options(request, options).map_err(Error::from_runtime)?;
        let raw = await_page(&mut pending, control)?;
        control.check_active()?;
        let decoded = crate::hq::decode::decode_reply(raw, RawPolicy::Omit, &NoPrecision)
            .map_err(|error| Error::new(ErrorKind::Protocol, "invalid security metadata page").with_source(error))?;
        Ok(decoded.response)
    })
}

/// One catalog pagination and validation algorithm for both shared flights and
/// an already-reserved connection. The latter must not wait on its own pool.
pub(crate) fn scan_market(
    mut candidate: MarketCandidate, market: HqMarket,
    mut boundary: impl FnMut() -> Result<()>,
    mut execute: impl FnMut(CommandRequest) -> Result<CommandResponse>,
) -> Result<MarketCandidate> {
    let mut page = SecurityListRequest::first_page(protocol_market(market));
    loop {
        boundary()?;
        let offset = page.start;
        let response = execute(CommandRequest::SecurityList(page))?;
        boundary()?;
        let CommandResponse::SecurityList(rows) = response else {
            return Err(Error::new(ErrorKind::Protocol, "metadata source returned a non-security page"));
        };
        if rows.len() > usize::from(page.limit) {
            return Err(Error::new(ErrorKind::Protocol, "security metadata page exceeds requested limit"));
        }
        let count = u32::try_from(rows.len()).map_err(|_| Error::new(ErrorKind::LimitExceeded, "security page count overflow"))?;
        let mut records = Vec::with_capacity(rows.len());
        for row in rows {
            boundary()?;
            if row.market != protocol_market(market) {
                return Err(Error::new(ErrorKind::Protocol, "security page has a mismatched market"));
            }
            let id = SecurityId::new(market, &row.code).map_err(|error| {
                Error::new(ErrorKind::Protocol, "security page has an invalid identity").with_source(error)
            })?;
            let precision = PrecisionMetadata::new(row.decimal, ValueProvenance::SecurityTable)?;
            // The catalog's multiple/category fields do not establish a
            // business quantity or currency unit, so those remain unknown.
            records.push(MetadataRecord::new(id, Some(precision), None, None));
        }
        candidate.push_page(offset, records, if count == 0 { PageEvidence::TerminalEmpty } else { PageEvidence::More })?;
        if count == 0 { return Ok(candidate); }
        page.start = offset.checked_add(count)
            .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "security page offset overflow"))?;
        // A short nonempty page remains nonterminal; only a real empty response
        // proves that the complete market candidate is safe to publish.
    }
}

fn await_page(pending: &mut PendingRawExecution, control: &FlightControl) -> Result<RawReply> {
    loop {
        if let Err(root) = control.check_active() { return Err(retire_page(pending, control, root)); }
        match pending.wait_timeout(SIGNAL_POLL_INTERVAL) {
            Ok(PendingPoll::Pending) => {}
            Ok(PendingPoll::Ready(reply)) => {
                if let Err(root) = control.check_active() { return Err(retire_page(pending, control, root)); }
                return Ok(reply);
            }
            Err(error) => {
                let error = Error::from_runtime(error);
                if let Err(root) = control.check_active() {
                    return Err(retire_page(pending, control, root.with_source(error)));
                }
                return Err(error);
            }
        }
    }
}

fn retire_page(pending: &PendingRawExecution, control: &FlightControl, root: Error) -> Error {
    let retired = (|| {
        let local = Instant::now().checked_add(control.cleanup_timeout())
            .ok_or_else(|| Error::new(ErrorKind::Internal, "metadata cleanup deadline overflow"))?;
        let confirmation = pending.begin_cancel().map_err(Error::from_runtime)?;
        loop {
            match confirmation.wait_timeout(Duration::ZERO).map_err(Error::from_runtime)? {
                PendingPoll::Ready(()) => return Ok(()),
                PendingPoll::Pending => {}
            }
            let remaining = control.cleanup_deadline(local)?.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let _ = confirmation.quarantine();
                return Err(Error::new(ErrorKind::Timeout, "metadata retirement was not confirmed"));
            }
            match confirmation.wait_timeout(remaining.min(SIGNAL_POLL_INTERVAL)) {
                Ok(PendingPoll::Ready(())) => return Ok(()),
                Ok(PendingPoll::Pending) => {}
                Err(error) => { let _ = confirmation.quarantine(); return Err(Error::from_runtime(error)); }
            }
        }
    })();
    match retired {
        Ok(()) => root,
        Err(cleanup) => {
            let _ = pending.quarantine();
            Error::new(ErrorKind::CancelUnconfirmed, "metadata stopped but retirement was not confirmed")
                .with_context("stage", "metadata cleanup").with_context("root_kind", format!("{:?}", root.kind()))
                .with_source(root).with_cleanup_failure(cleanup)
        }
    }
}

pub(crate) const fn hq_market(market: Market) -> HqMarket {
    match market { Market::Shenzhen => HqMarket::Shenzhen, Market::Shanghai => HqMarket::Shanghai, Market::Beijing => HqMarket::Beijing }
}

fn protocol_market(market: HqMarket) -> Market {
    match market { HqMarket::Shenzhen => Market::Shenzhen, HqMarket::Shanghai => Market::Shanghai, HqMarket::Beijing => Market::Beijing }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::Arc;
    use std::thread;

    use eltdx_protocol::frame::{RequestFrame, RESPONSE_PREFIX};
    use eltdx_runtime::engine::{EngineConfig, EngineConfigOptions};
    use crate::config::ConfigDraft;
    use crate::metadata::{FlightPolicy, SnapshotKey};
    use crate::operation::{OperationContext, OperationOptions};
    use crate::session::SessionOwner;

    fn read_request(stream: &mut TcpStream) -> RequestFrame {
        let mut bytes = vec![0; 12];
        stream.read_exact(&mut bytes).unwrap();
        let body = usize::from(u16::from_le_bytes([bytes[6], bytes[7]])) - 2;
        bytes.resize(12 + body, 0);
        stream.read_exact(&mut bytes[12..]).unwrap();
        RequestFrame::decode(&bytes).unwrap()
    }

    fn respond(stream: &mut TcpStream, request: &RequestFrame, payload: &[u8]) {
        let mut bytes = Vec::from(RESPONSE_PREFIX);
        bytes.push(1);
        bytes.extend_from_slice(&request.msg_id.to_le_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&request.msg_type.to_le_bytes());
        let size = u16::try_from(payload.len()).unwrap();
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(payload);
        stream.write_all(&bytes).unwrap();
    }

    fn record_page(code: &[u8; 6], decimal: u8) -> Vec<u8> {
        let mut payload = vec![1, 0];
        let mut record = [0_u8; 37];
        record[..6].copy_from_slice(code);
        record[28] = decimal;
        payload.extend_from_slice(&record);
        payload
    }

    fn fixture() -> (Engine, Arc<SessionOwner>, OperationContext, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            let handshake = read_request(&mut stream);
            respond(&mut stream, &handshake, &[0; 189]);
            // Two nonempty short pages must both be read before the empty page.
            // The next request then starts a refresh, whose second page fails.
            for (offset, payload) in [
                (0_u32, record_page(b"600000", 2)),
                (1, record_page(b"600001", 4)),
                (2, vec![0, 0]),
                (0, record_page(b"600000", 5)),
                (1, vec![1, 0]), // declared record is truncated
            ] {
                let request = read_request(&mut stream);
                assert_eq!(request.msg_type, eltdx_protocol::commands::security::TYPE_SECURITY_LIST);
                assert_eq!(u32::from_le_bytes(request.data[2..6].try_into().unwrap()), offset);
                respond(&mut stream, &request, &payload);
            }
            let mut byte = [0];
            assert_eq!(stream.read(&mut byte).unwrap(), 0);
        });
        let engine = Engine::new(EngineConfig::new(vec![format!("{address}")], 2.0, EngineConfigOptions {
            pool_size: 1, runtime_workers: Some(1), server_count: 1,
            max_connections_per_host: None, connect_concurrency: None, connect_concurrency_per_host: None,
            global_raw_bytes: None, global_decoded_bytes: None, heartbeat_interval: None,
            max_pending_requests: 4, push_queue_size: 1, push_queue_bytes: 1024,
        }).unwrap()).unwrap();
        let config = ConfigDraft::default().validate().unwrap();
        let owner = Arc::new(SessionOwner::new(Duration::from_secs(1), Duration::from_secs(1)));
        owner.register_engine(engine.clone()).unwrap();
        let context = OperationContext::rust_public(Instant::now(), "metadata-loader", Arc::clone(&owner),
            &config, &OperationOptions::default()).unwrap();
        (engine, owner, context, server)
    }

    #[test]
    fn actual_hq_loader_continues_short_pages_and_preserves_good_version_on_bad_refresh() {
        let (engine, owner, context, server) = fixture();
        let policy = FlightPolicy::rust_source_cap(Duration::from_secs(8), Duration::from_secs(1));
        let store = owner.metadata();
        let snapshot = store.load_hq_market(engine.clone(), "loopback-catalog", HqMarket::Shanghai,
            context.sponsor(), policy, Duration::from_secs(2), Duration::from_secs(60), false).unwrap();
        assert_eq!(snapshot.require_precision(&SecurityId::new(HqMarket::Shanghai, "600000").unwrap()).unwrap().decimals(), 2);
        assert_eq!(snapshot.require_precision(&SecurityId::new(HqMarket::Shanghai, "600001").unwrap()).unwrap().decimals(), 4);
        let failure = store.load_hq_market(engine, "loopback-catalog", HqMarket::Shanghai,
            context.sponsor(), policy, Duration::from_secs(2), Duration::from_secs(60), true).unwrap_err();
        assert_eq!(failure.kind(), ErrorKind::Protocol);
        assert_eq!(store.last_known_good(&SnapshotKey::hq("loopback-catalog", HqMarket::Shanghai)).unwrap().unwrap().version(), snapshot.version());
        drop(context);
        owner.close().unwrap();
        server.join().unwrap();
    }
}
