#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Public-client synthetic integration source. No fixture is a server capture.
#[path = "support/hq_server.rs"]
mod hq_server;
use eltdx::{CodeQuery, ErrorKind, HqMarket, OperationOptions, RawPolicy, TdxClient};
use std::sync::{Arc, Mutex};

#[test]
fn public_nonempty_directory_paging_then_fault_can_close_cleanly() {
    let offsets = Arc::new(Mutex::new(Vec::new()));
    let seen = offsets.clone();
    let server = hq_server::Server::start(move |request| match request.msg_type {
        0x044d => {
            let offset = hq_server::directory_offset(request);
            seen.lock().unwrap().push(offset);
            if offset == 0 {
                hq_server::directory(&[("600000", 2)])
            } else {
                hq_server::directory(&[])
            }
        }
        0x06b9 => vec![4, 0, 0, 0, b'x'],
        _ => panic!("unexpected source request"),
    })
    .unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let options = OperationOptions::default();
    let page = client
        .codes()
        .list(HqMarket::Shanghai, &CodeQuery::new(), &options)
        .unwrap();
    assert_eq!(page.items().len(), 1);
    assert_eq!(page.items()[0].id().number(), "600000");
    let continuation = page.next_cursor().expect("directory continuation").clone();
    let next = client
        .codes()
        .list(
            HqMarket::Shanghai,
            &CodeQuery::new().with_cursor(&continuation).unwrap(),
            &options,
        )
        .unwrap();
    assert!(next.items().is_empty());
    assert_eq!(*offsets.lock().unwrap(), vec![0, 1]);
    assert_eq!(
        client
            .resources()
            .download_file("file", 8, &options)
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn getters_and_local_ping_are_lazy_and_local_diagnostics_survive_close() {
    let server =
        hq_server::Server::start(|_| panic!("local ping is not a network request")).unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let session = client.session();
    let health = client.health();
    let _resources = client.resources();
    let _company = client.company_info();
    for ping in [session.ping().unwrap(), health.ping().unwrap()] {
        assert_eq!(ping.message(), "pong");
        assert!(!ping.remote_verified());
    }
    assert_eq!(server.connection_count(), 0);
    client.close().unwrap();
    assert_eq!(session.ping().unwrap().message(), "pong");
    assert!(!health.ping().unwrap().remote_verified());
    assert_eq!(
        health
            .probe(&OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    assert_eq!(server.connection_count(), 0);
    server.finish().unwrap();
}

#[test]
fn session_and_explicit_probe_preserve_control_semantics_under_omit() {
    let server = hq_server::Server::start(|_| panic!("only control requests expected")).unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .pool_size(1)
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let options = OperationOptions::default().with_raw(RawPolicy::Omit);
    let handshake = client.session().handshake(&options).unwrap();
    assert_eq!(handshake.flags().len(), 10);
    assert_eq!(handshake.control_tail().len(), 8);
    assert!(handshake.meta().raw().is_none());
    assert_eq!(handshake.unknown_time1(), Some(0));
    assert_eq!(handshake.unknown_time2(), Some(0));
    assert_eq!(handshake.meta().source().command(), Some(0x000d));
    let heartbeat = client.session().heartbeat(&options).unwrap();
    assert_eq!(heartbeat.reserved().len(), 6);
    assert_eq!(heartbeat.server_date_raw(), 0);
    assert!(heartbeat.server_date().is_none());
    assert!(heartbeat.meta().raw().is_none());
    let report = client.health().probe(&options).unwrap();
    assert_eq!(report.endpoint(), server.address);
    assert_eq!(report.meta().source().command(), Some(0x0004));
    assert!(report.meta().raw().is_none());
    assert_eq!(server.connection_count(), 1);
    client.close().unwrap();
    server.finish().unwrap();
}

#[test]
fn nonzero_controls_and_source_dates_survive_the_public_session_projection() {
    use eltdx::{types::EventTimeSource, TradingDate};
    let server = hq_server::Server::start_all(|request| match request.msg_type {
        0x000d => {
            let mut bytes = vec![0; 189];
            bytes[1..3].copy_from_slice(&2026_u16.to_le_bytes());
            bytes[3..9].copy_from_slice(&[16, 11, 30, 10, 0, 5]);
            bytes[42..46].copy_from_slice(&20261116_u32.to_le_bytes());
            bytes[50..54].copy_from_slice(&20261113_u32.to_le_bytes());
            bytes[46..50].copy_from_slice(&123_u32.to_le_bytes());
            bytes[54..58].copy_from_slice(&456_u32.to_le_bytes());
            bytes[58..68].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
            bytes[152..160].copy_from_slice(&[11, 12, 13, 14, 15, 16, 17, 18]);
            bytes
        }
        0x0004 => {
            let mut bytes = vec![1, 2, 3, 4, 5, 6];
            bytes.extend_from_slice(&20261116_u32.to_le_bytes());
            bytes
        }
        _ => panic!("unexpected session command"),
    })
    .unwrap();
    let client = TdxClient::builder()
        .endpoints(vec![server.address])
        .heartbeat_interval(None)
        .build()
        .unwrap();
    let options = OperationOptions::default().with_raw(RawPolicy::Omit);
    let handshake = client.session().handshake(&options).unwrap();
    assert_eq!(
        handshake.server_datetime().unwrap().source_kind(),
        EventTimeSource::Local
    );
    assert_eq!(
        handshake.server_datetime().unwrap().clock(),
        Some((10, 30, 5))
    );
    assert_eq!(
        handshake.server_date1(),
        Some(TradingDate::new(2026, 11, 16).unwrap())
    );
    assert_eq!(
        handshake.server_date2(),
        Some(TradingDate::new(2026, 11, 13).unwrap())
    );
    assert_eq!(handshake.unknown_time1(), Some(123));
    assert_eq!(handshake.unknown_time2(), Some(456));
    assert_eq!(&handshake.flags()[..], &[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
    assert_eq!(
        &handshake.control_tail()[..],
        &[11, 12, 13, 14, 15, 16, 17, 18]
    );
    let heartbeat = client.session().heartbeat(&options).unwrap();
    assert_eq!(&heartbeat.reserved()[..], &[1, 2, 3, 4, 5, 6]);
    assert_eq!(heartbeat.server_date_raw(), 20261116);
    assert_eq!(
        heartbeat.meta().source().trading_date(),
        Some(TradingDate::new(2026, 11, 16).unwrap())
    );
    client.close().unwrap();
    server.finish().unwrap();
}
