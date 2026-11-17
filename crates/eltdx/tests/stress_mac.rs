//! Synthetic MAC-EX cache stress. No production login claim is made.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"] mod wire;
#[path = "support/stress_log.rs"] mod stress_log;
use std::sync::Arc;
use std::time::Instant;
use bytes::Bytes;
use eltdx::{ErrorKind, OperationOptions, PageOptions, RawPolicy};
use eltdx::ex::ExMarketId;
use eltdx::mac::{MacExClient, handshake::LoginReplyRules};
use wire::{Case, eof, frame, join, peer, request};

fn count(stream: &mut std::net::TcpStream) -> Case {
    assert!(request(stream, 1, 0x23f0)?.is_empty());
    let mut reply = vec![0; 23]; reply[19..].copy_from_slice(&1_u32.to_le_bytes()); frame(stream, &reply)
}
fn instruments(stream: &mut std::net::TcpStream) -> Case {
    count(stream)?;
    let command = request(stream, 1, 0x23f5)?;
    assert_eq!(command, [0, 0, 0, 0, 1, 0]);
    let mut body = vec![0, 0, 0, 0, 1, 0];
    let mut row = [0; 64]; row[0] = 5; row[1] = 38;
    row[5..8].copy_from_slice(b"X_A"); row[14..17].copy_from_slice(b"one");
    body.extend_from_slice(&row); frame(stream, &body)?; count(stream)
}
fn goods(stream: &mut std::net::TcpStream, code: &[u8]) -> Case {
    let command = request(stream, 1, 0x2562)?;
    assert_eq!(command.len(), 10); assert_eq!(command[..2], [38, 0]);
    assert_eq!(u32::from_le_bytes(command[2..6].try_into()?), 0);
    let mut body = vec![1, 0]; let mut row = [0; 48];
    row[..2].copy_from_slice(&5_u16.to_le_bytes()); row[2..2 + code.len()].copy_from_slice(code);
    row[32..36].copy_from_slice(&1_f32.to_bits().to_le_bytes()); body.extend_from_slice(&row); frame(stream, &body)
}

#[test]
#[ignore = "candidate-only: real MAC-EX directory/macro caches, immutable handles and close cycles"]
fn stress_mac_ex_catalog_macro_cache_lifecycle() -> Case {
    let cycles = stress_log::count("ELTDX_RUST_STRESS_MAC_CYCLES", 500, 100, 5_000)?;
    let log = stress_log::Log::start("mac_cache", cycles, 1)?;
    let market = ExMarketId::from_wire(38); let pages = PageOptions::new(600, 8, 100)?;
    for iteration in 0..=cycles {
        let id = iteration.saturating_sub(1);
        let started = Instant::now();
        let (endpoint, task) = peer(|mut stream| {
            assert_eq!(request(&mut stream, 1, 0x2454)?.len(), 80);
            frame(&mut stream, b"stress synthetic accepted")?;
            instruments(&mut stream)?; goods(&mut stream, b"X_A")?;
            // Reads through live snapshots and macro_indicators must be local;
            // only the explicit clear permits this second goods request.
            goods(&mut stream, b"X_B")?; eof(&mut stream)
        })?;
        let rules = LoginReplyRules::new(vec![Bytes::from_static(b"stress synthetic accepted")],
            vec![Bytes::from_static(b"stress synthetic rejected")], "synthetic stress only")?;
        let client = MacExClient::builder().with_endpoints(vec![endpoint]).with_login_reply_rules(rules)
            .with_raw(if id % 2 == 0 { RawPolicy::Omit } else { RawPolicy::RetainWire }).build()?;
        let options = OperationOptions::default();
        let old_directory = client.instrument_catalog(market, &pages, &options)?;
        let cached_directory = client.instrument_catalog(market, &pages, &options)?;
        assert!(Arc::ptr_eq(&old_directory, &cached_directory));
        assert_eq!(old_directory.rows()[0].code(), "X_A");
        let old_goods = client.goods_catalog(market, &pages, &options)?;
        for _ in 0..8 {
            assert_eq!(client.macro_indicators(&options)?[0].actual_code(), "X_A");
        }
        client.clear_catalog_cache(&options)?;
        let new_goods = client.goods_catalog(market, &pages, &options)?;
        assert_eq!(new_goods.rows()[0].actual_code(), "X_B");
        assert!(!Arc::ptr_eq(&old_goods, &new_goods));
        assert!(new_goods.version() > old_goods.version());
        client.close()?; client.close()?;
        assert_eq!(client.clear_catalog_cache(&options).expect_err("closed cache owner reopened").kind(), ErrorKind::Closed);
        assert_eq!(old_directory.rows()[0].code(), "X_A");
        assert_eq!(old_goods.rows()[0].actual_code(), "X_A");
        assert_eq!(new_goods.rows()[0].actual_code(), "X_B");
        join(task)?;
        drop((old_directory, cached_directory, old_goods, new_goods, client));
        if iteration == 0 {
            log.write("\"kind\":\"configuration\",\"catalog_page_size\":600,\"max_pages\":8,\"max_records\":100,\"macro_cache_reads\":8,\"warmup_owners\":1".into())?;
            log.warmed()?;
        } else {
            log.terminal(id, "completed", started.elapsed())?;
        }
    }
    log.finish(cycles)?; Ok(())
}
