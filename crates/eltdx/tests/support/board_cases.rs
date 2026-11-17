//! Public HQ board contracts, authored for the final unified verification.
//! These sources have not been compiled or executed during implementation.
use super::{daily_helpers_cases as daily, hq_server};
use bytes::Bytes;
use eltdx::types::EnrichmentStatus;
use eltdx::{
    BoardCategory, BoardFile, BoardId, BoardQuery, CancellationToken, Clock, ErrorKind,
    OperationOptions, RawPolicy, TdxClient, TradingDate,
};
use eltdx_protocol::frame::RequestFrame;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct BoardClock(AtomicI64);
impl Clock for BoardClock {
    fn unix_seconds(&self) -> eltdx::Result<i64> {
        Ok(self.0.load(Ordering::SeqCst))
    }
}
fn client(server: &hq_server::Server) -> TdxClient {
    TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .clock(Arc::new(BoardClock(AtomicI64::new(0))))
        .build()
        .unwrap()
}
fn file(name: &str, data: &[u8]) -> BoardFile {
    BoardFile::new(name, Bytes::copy_from_slice(data)).unwrap()
}
fn memory_query(content: &[u8]) -> BoardQuery {
    BoardQuery::new()
        .with_file(file("infoharbor_block.dat", content))
        .unwrap()
        .with_file(file("tdxhy.cfg", b""))
        .unwrap()
}
fn session_directory(request: &RequestFrame, numbers: &[&str]) -> Option<Vec<u8>> {
    match request.msg_type {
        0x000d => Some(hq_server::handshake_dates(20261113, 20261116)),
        0x0004 => Some(vec![0; 10]),
        0x044d => Some(
            if hq_server::directory_offset(request) == 0 && request.data[0] == 1 {
                hq_server::directory(
                    &numbers
                        .iter()
                        .map(|number| (*number, 2))
                        .collect::<Vec<_>>(),
                )
            } else {
                hq_server::directory(&[])
            },
        ),
        _ => None,
    }
}
fn path(request: &RequestFrame) -> &str {
    let bytes = &request.data[8..];
    let end = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..end]).unwrap()
}

#[test]
fn board_types_are_explicit_and_entry_rejection_precedes_session_or_files() {
    let query = memory_query(b"#GN_Concept,0,880001\n");
    assert_eq!(query.category(), BoardCategory::Concept);
    assert!(!query.refresh());
    assert!(query.data_dir().is_none());
    assert!(query.definitions_dir().is_none());
    assert_eq!(query.files().len(), 2);
    assert_eq!(
        BoardId::new("12345", None).unwrap_err().kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        BoardFile::new("../infoharbor_block.dat", Bytes::new())
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!(
        BoardQuery::new()
            .with_data_dir(Some(std::path::PathBuf::new()))
            .unwrap_err()
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert_eq!("880001".parse::<BoardId>().unwrap().market(), None);
    let server = hq_server::Server::start(|_| panic!("rejected operation must not send")).unwrap();
    let client = client(&server);
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        client
            .helpers()
            .board_quotes(
                &query,
                &OperationOptions::default().with_cancellation(token)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert_eq!(
        client
            .helpers()
            .board_member_quotes(
                &"880001".parse().unwrap(),
                &query,
                &OperationOptions::default().with_timeout(Duration::ZERO)
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Timeout
    );
    assert_eq!(server.connection_count(), 0);
    client.close().unwrap();
    assert_eq!(
        client
            .helpers()
            .board_quotes(&query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    server.finish().unwrap();
}

#[test]
fn explicit_memory_board_quotes_keep_definition_order_and_join_reversed_snapshots() {
    let server = hq_server::Server::start_all(|request| {
        if let Some(reply) = session_directory(request, &["880001", "880002"]) {
            return reply;
        }
        assert_eq!(
            request.msg_type, 0x054c,
            "explicit sources must not start HQ file downloads"
        );
        hq_server::snapshots(&[("880001", 200_000), ("880002", 100_000)])
    })
    .unwrap();
    let client = client(&server);
    let query = memory_query(b"#GN_Second,0,880002\n#GN_First,0,880001\n");
    let table = client
        .helpers()
        .board_quotes(
            &query,
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
        )
        .unwrap();
    assert_eq!(
        table
            .rows()
            .iter()
            .map(|row| row.board().id().number())
            .collect::<Vec<_>>(),
        vec!["880002", "880001"]
    );
    for row in table.rows() {
        assert_eq!(row.security_id(), row.quote().unwrap().id());
    }
    assert_eq!(
        table.rows()[1].last().unwrap().as_f64(),
        table.rows()[0].last().unwrap().as_f64() * 2.0
    );
    assert_eq!(table.rows()[0].board().name(), "Second");
    assert!(table
        .meta()
        .raw()
        .unwrap()
        .payloads()
        .iter()
        .any(|bytes| bytes.as_ref()
            == query
                .files()
                .iter()
                .find(|file| file.name() == "infoharbor_block.dat")
                .unwrap()
                .data()
                .as_ref()));
    assert!(table
        .meta()
        .diagnostics()
        .iter()
        .any(|item| item.stage() == "boards.source_date"
            && item.status() == EnrichmentStatus::Unavailable
            && item.message().unwrap_or("").contains("freshness")));
    assert!(table
        .meta()
        .sources()
        .iter()
        .any(|source| source.command() == Some(0x054c)));
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn board_members_retain_raw_excluded_facts_and_quote_only_the_current_ordered_subset() {
    let server = hq_server::Server::start_all(|request| {
        if let Some(reply) = session_directory(request, &["600000", "600001"]) {
            return reply;
        }
        assert_eq!(request.msg_type, 0x054c);
        assert_eq!(hq_server::snapshot_codes(request), vec!["600001", "600000"]);
        hq_server::snapshots(&[("600000", 100_000), ("600001", 200_000)])
    })
    .unwrap();
    let client = client(&server);
    let query = memory_query(b"#GN_Members,3,880001\n1#600001,1#600099,1#600000,1#600001\n");
    let result = client
        .helpers()
        .board_member_quotes(
            &"880001".parse().unwrap(),
            &query,
            &OperationOptions::default().with_raw(RawPolicy::RetainWire),
        )
        .unwrap();
    assert_eq!(
        result.prepared_date(),
        TradingDate::new(2026, 11, 16).unwrap()
    );
    assert_eq!(
        result
            .raw_members()
            .iter()
            .map(|member| member.id().number())
            .collect::<Vec<_>>(),
        vec!["600001", "600099", "600000"]
    );
    assert_eq!(
        result
            .display_members()
            .iter()
            .map(|member| member.id().number())
            .collect::<Vec<_>>(),
        vec!["600001", "600000"]
    );
    assert_eq!(result.excluded_members().len(), 1);
    assert_eq!(
        result.raw_members()[1].excluded_reason(),
        Some("absent_from_current_security_list")
    );
    assert_eq!(
        result.excluded_members()[0].excluded_reason(),
        result.raw_members()[1].excluded_reason()
    );
    for (row, member) in result.rows().iter().zip(result.display_members()) {
        assert_eq!(row.security_id(), member.id());
        assert_eq!(row.quote().unwrap().id(), member.id());
    }
    assert_eq!(
        result.rows()[0].last().unwrap().as_f64(),
        result.rows()[1].last().unwrap().as_f64() * 2.0
    );
    assert!(result.meta().raw().unwrap().wire_frames().unwrap().len() >= 4);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn optional_board_quote_corruption_is_failed_and_confirmed_missing_is_unavailable_but_cancel_escapes(
) {
    for case in 0..3 {
        let token = CancellationToken::default();
        let cancel = token.clone();
        let server = hq_server::Server::start_all(move |request| {
            if let Some(reply) = session_directory(request, &["880001"]) {
                return reply;
            }
            assert_eq!(request.msg_type, 0x054c);
            match case {
                0 => vec![1],
                1 => hq_server::snapshots(&[]),
                _ => {
                    cancel.cancel();
                    hq_server::snapshots(&[])
                }
            }
        })
        .unwrap();
        let client = client(&server);
        let result = client.helpers().board_quotes(
            &memory_query(b"#GN_Concept,0,880001\n"),
            &OperationOptions::default().with_cancellation(token),
        );
        if case == 2 {
            assert!(matches!(
                result.unwrap_err().kind(),
                ErrorKind::Cancelled | ErrorKind::CancelUnconfirmed
            ));
        } else {
            let table = result.unwrap();
            assert_eq!(table.rows().len(), 1);
            assert!(table.rows()[0].quote().is_none());
            let expected = if case == 0 {
                EnrichmentStatus::Failed
            } else {
                EnrichmentStatus::Unavailable
            };
            assert!(table.rows()[0]
                .meta()
                .diagnostics()
                .iter()
                .any(|item| item.stage() == "boards.quotes" && item.status() == expected));
            assert!(table
                .meta()
                .diagnostics()
                .iter()
                .any(|item| item.stage() == "boards.quotes" && item.status() == expected));
        }
        client.close().unwrap();
        server.finish().unwrap();
    }
}

#[test]
fn directory_or_membership_corruption_is_required_and_region_needs_a_real_dbf_source() {
    for case in 0..3 {
        let server = hq_server::Server::start_all(move |request| {
            if request.msg_type == 0x044d {
                assert_eq!(case, 0);
                return vec![1];
            }
            session_directory(request, &[])
                .expect("required source failure must precede quote work")
        })
        .unwrap();
        let client = client(&server);
        let query = match case {
            0 => memory_query(b"#GN_Concept,1,880001\n1#600000\n"),
            1 => memory_query(b"#GN_Bad,2,880001\n1#600000,1#600000\n"),
            _ => memory_query(b"#GN_Other,0,880002\n")
                .with_file(file("tdxzs.cfg", b"Region|880001|3|1|0|13\n"))
                .unwrap(),
        };
        let error = client
            .helpers()
            .board_member_quotes(
                &"880001".parse().unwrap(),
                &query,
                &OperationOptions::default(),
            )
            .unwrap_err();
        assert_eq!(
            error.kind(),
            if case == 0 {
                ErrorKind::Protocol
            } else {
                ErrorKind::ResourceFormat
            }
        );
        client.close().unwrap();
        server.finish().unwrap();
    }
}

#[test]
fn downloaded_board_pair_is_memory_cached_refreshable_and_cleared_without_relabeling_freshness() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let observed = seen.clone();
    let server = hq_server::Server::start_all(move |request| {
        if let Some(reply) = session_directory(request, &["880001"]) {
            return reply;
        }
        match request.msg_type {
            0x06b9 => {
                let name = path(request);
                observed.lock().unwrap().push(name.to_owned());
                daily::file_reply(
                    request,
                    match name {
                        "infoharbor_block.dat" => b"#GN_Concept,0,880001\n".as_slice(),
                        "tdxhy.cfg" => b"".as_slice(),
                        _ => panic!("only the fixed HQ board pair may be downloaded"),
                    },
                )
            }
            0x054c => hq_server::snapshots(&[("880001", 100_000)]),
            _ => panic!("unexpected board source"),
        }
    })
    .unwrap();
    let client = client(&server);
    let options = OperationOptions::default();
    let query = BoardQuery::new();
    for _ in 0..2 {
        let table = client.helpers().board_quotes(&query, &options).unwrap();
        assert!(table
            .meta()
            .diagnostics()
            .iter()
            .any(|item| item.stage() == "boards.source_date"
                && item.status() == EnrichmentStatus::Unavailable));
    }
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        ["infoharbor_block.dat", "tdxhy.cfg"]
    );
    client
        .helpers()
        .board_quotes(&query.clone().with_refresh(true).unwrap(), &options)
        .unwrap();
    assert_eq!(seen.lock().unwrap().len(), 4);
    client.helpers().clear_cache();
    client.helpers().board_quotes(&query, &options).unwrap();
    assert_eq!(seen.lock().unwrap().len(), 6);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn board_cache_preserves_failed_counterpart_diagnostic_until_short_ttl_then_retries() {
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let clock = Arc::new(BoardClock(AtomicI64::new(0)));
    let server = hq_server::Server::start_all(move |request| {
        if let Some(reply) = session_directory(request, &["880001"]) {
            return reply;
        }
        match request.msg_type {
            0x06b9 => {
                observed.fetch_add(1, Ordering::SeqCst);
                if path(request) == "tdxhy.cfg" {
                    vec![1]
                } else {
                    daily::file_reply(request, b"#GN_Concept,0,880001\n")
                }
            }
            0x054c => hq_server::snapshots(&[("880001", 100_000)]),
            _ => panic!("unexpected board source"),
        }
    })
    .unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .clock(clock.clone())
        .build()
        .unwrap();
    for _ in 0..2 {
        let result = client
            .helpers()
            .board_quotes(&BoardQuery::new(), &OperationOptions::default())
            .unwrap();
        assert!(result
            .meta()
            .diagnostics()
            .iter()
            .any(|item| item.stage() == "boards.tdxhy.cfg"
                && item.status() == EnrichmentStatus::Failed));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    clock.0.store(30, Ordering::SeqCst);
    client
        .helpers()
        .board_quotes(&BoardQuery::new(), &OperationOptions::default())
        .unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn category_switch_uses_all_prepared_sources_and_member_quotes_keep_eighty_item_wire_batches() {
    let batches = Arc::new(Mutex::new(Vec::new()));
    let observed = batches.clone();
    let numbers = (0..81)
        .map(|index| format!("{:06}", 600000 + index))
        .collect::<Vec<_>>();
    let catalog = numbers.clone();
    let mut content = b"#GN_Members,81,880001\n".to_vec();
    for number in &numbers {
        content.extend_from_slice(format!("1#{number},").as_bytes());
    }
    content.extend_from_slice(b"\n#FG_Style,0,880002\n");
    let server = hq_server::Server::start_all(move |request| {
        let catalog = catalog
            .iter()
            .map(String::as_str)
            .chain(["880001", "880002"])
            .collect::<Vec<_>>();
        if let Some(reply) = session_directory(request, &catalog) {
            return reply;
        }
        assert_eq!(request.msg_type, 0x054c);
        let mut codes = hq_server::snapshot_codes(request);
        observed.lock().unwrap().push(codes.len());
        codes.reverse();
        hq_server::snapshots(
            &codes
                .iter()
                .map(|number| (number.as_str(), 100_000))
                .collect::<Vec<_>>(),
        )
    })
    .unwrap();
    let client = client(&server);
    let query = memory_query(&content);
    client
        .helpers()
        .board_quotes(&query, &OperationOptions::default())
        .unwrap();
    let styles = client
        .helpers()
        .board_quotes(
            &query.clone().with_category(BoardCategory::Style).unwrap(),
            &OperationOptions::default(),
        )
        .unwrap();
    assert_eq!(styles.rows()[0].board().id().number(), "880002");
    assert!(styles
        .meta()
        .diagnostics()
        .iter()
        .any(|item| item.stage() == "boards.cache"
            && item.message().unwrap_or("").contains("cache_hit=true")));
    batches.lock().unwrap().clear();
    let members = client
        .helpers()
        .board_member_quotes(
            &"880001".parse().unwrap(),
            &query,
            &OperationOptions::default(),
        )
        .unwrap();
    assert_eq!(members.rows().len(), 81);
    assert_eq!(*batches.lock().unwrap(), vec![80, 1]);
    assert_eq!(
        members
            .rows()
            .iter()
            .map(|row| row.security_id().number())
            .collect::<Vec<_>>(),
        numbers.iter().map(String::as_str).collect::<Vec<_>>()
    );
    client.close().unwrap();
    server.finish().unwrap();
}
