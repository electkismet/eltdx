#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Authored public MAC-EX contracts. Do not execute before T01-T30 sources
//! are complete. Every login payload is synthetic, not production evidence.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"]
mod wire;
use bytes::Bytes;
use eltdx::ex::ExMarketId;
use eltdx::mac::ex_query::*;
use eltdx::mac::handshake::LoginReplyRules;
use eltdx::mac::{CatalogKind, FieldSelection, MacExClient, MacExClientBuilder, PageCompletion};
use eltdx::{CancellationToken, ErrorKind, OperationOptions, PageOptions, RawPolicy};
use std::io::{ErrorKind as IoKind, Write};
use std::net::{TcpListener, TcpStream};
use std::num::NonZeroUsize;
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;
use wire::{eof, frame, frame_bytes, join, peer, request, Case};

fn market(value: u8) -> ExMarketId {
    ExMarketId::from_wire(value)
}
fn rules() -> eltdx::Result<LoginReplyRules> {
    LoginReplyRules::new(
        vec![Bytes::from_static(b"catalog fixture accepted")],
        vec![Bytes::from_static(b"catalog fixture denied")],
        "synthetic catalog source only",
    )
}
fn builder(endpoint: eltdx::mac::Endpoint) -> eltdx::Result<MacExClientBuilder> {
    Ok(MacExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_login_reply_rules(rules()?))
}
fn login(stream: &mut TcpStream) -> Case {
    let body = request(stream, 1, 0x2454)?;
    assert_eq!(body.len(), 80);
    frame(stream, b"catalog fixture accepted")
}
fn count(stream: &mut TcpStream, total: u32) -> Case {
    assert!(request(stream, 1, 0x23f0)?.is_empty());
    let mut body = vec![0; 23];
    body[19..].copy_from_slice(&total.to_le_bytes());
    frame(stream, &body)
}
fn directory_body(offset: u32, rows: &[(u8, &str, &str)]) -> Case<Vec<u8>> {
    let mut body = offset.to_le_bytes().to_vec();
    body.extend_from_slice(&u16::try_from(rows.len())?.to_le_bytes());
    for &(market, code, name) in rows {
        assert!(code.len() <= 9 && name.len() <= 17);
        let mut row = [0; 64];
        row[0] = 5;
        row[1] = market;
        row[5..5 + code.len()].copy_from_slice(code.as_bytes());
        row[14..14 + name.len()].copy_from_slice(name.as_bytes());
        row[31..35].copy_from_slice(b"desc");
        row[63] = 0xa5;
        body.extend_from_slice(&row);
    }
    Ok(body)
}
fn directory_request(stream: &mut TcpStream, offset: u32, requested: u16) -> Case {
    let body = request(stream, 1, 0x23f5)?;
    assert_eq!(body.len(), 6);
    assert_eq!(u32::from_le_bytes(body[..4].try_into()?), offset);
    assert_eq!(u16::from_le_bytes(body[4..].try_into()?), requested);
    Ok(())
}
fn scan(stream: &mut TcpStream, rows: &[(u8, &str, &str)], page_size: usize) -> Case {
    count(stream, u32::try_from(rows.len())?)?;
    for (index, page) in rows.chunks(page_size).enumerate() {
        let offset = u32::try_from(index * page_size)?;
        directory_request(stream, offset, u16::try_from(page.len())?)?;
        frame(stream, &directory_body(offset, page)?)?;
    }
    count(stream, u32::try_from(rows.len())?)
}
fn goods_body(codes: &[&str]) -> Case<Vec<u8>> {
    let mut body = u16::try_from(codes.len())?.to_le_bytes().to_vec();
    for (index, code) in codes.iter().enumerate() {
        assert!(code.len() <= 23);
        let mut row = [0; 48];
        row[0..2].copy_from_slice(&5_u16.to_le_bytes());
        row[2..2 + code.len()].copy_from_slice(code.as_bytes());
        row[27..31].copy_from_slice(&u32::try_from(index)?.to_le_bytes());
        row[32..36].copy_from_slice(&(-0.0_f32).to_bits().to_le_bytes());
        body.extend_from_slice(&row);
    }
    Ok(body)
}
fn goods(stream: &mut TcpStream, market: u16, start: u32, requested: u32, codes: &[&str]) -> Case {
    let body = request(stream, 1, 0x2562)?;
    assert_eq!(body.len(), 10);
    assert_eq!(u16::from_le_bytes(body[..2].try_into()?), market);
    assert_eq!(u32::from_le_bytes(body[2..6].try_into()?), start);
    assert_eq!(u32::from_le_bytes(body[6..].try_into()?), requested);
    frame(stream, &goods_body(codes)?)
}
fn quote_request(stream: &mut TcpStream, expected: &[&str]) -> Case {
    let body = request(stream, 1, 0x122b)?;
    assert_eq!(body.len(), 22 + 24 * expected.len());
    assert_eq!(
        u16::from_le_bytes(body[20..22].try_into()?),
        u16::try_from(expected.len())?
    );
    for (row, code) in body[22..].chunks_exact(24).zip(expected) {
        assert_eq!(u16::from_le_bytes(row[..2].try_into()?), 38);
        assert_eq!(&row[2..2 + code.len()], code.as_bytes());
    }
    Ok(())
}
fn quote_body(codes: &[&str]) -> Case<Vec<u8>> {
    let mut body = vec![0; 26];
    body[0] = 1 << 4;
    body[20..24].copy_from_slice(&99_u32.to_le_bytes());
    body[24..26].copy_from_slice(&u16::try_from(codes.len())?.to_le_bytes());
    for code in codes {
        let mut row = vec![0; 72];
        row[..2].copy_from_slice(&38_u16.to_le_bytes());
        row[2..2 + code.len()].copy_from_slice(code.as_bytes());
        row[24..29].copy_from_slice(b"macro");
        row[68..].copy_from_slice(&10.5_f32.to_bits().to_le_bytes());
        body.extend_from_slice(&row);
    }
    Ok(body)
}
fn selectors(values: &[&str]) -> eltdx::Result<MacroQuotesQuery> {
    MacroQuotesQuery::new(
        values
            .iter()
            .map(|value| MacroCode::new(*value))
            .collect::<eltdx::Result<Vec<_>>>()?,
    )
}
fn reason(error: &eltdx::Error, expected: &str) -> bool {
    error
        .context()
        .iter()
        .any(|(key, value)| key == "reason" && value == expected)
}
fn diagnostic(meta: &eltdx::types::ResultMeta, stage: &str) -> bool {
    meta.diagnostics().iter().any(|item| item.stage() == stage)
}
fn no_connection(listener: &TcpListener) -> Case {
    listener.set_nonblocking(true)?;
    assert_eq!(
        listener.accept().expect_err("no connection").kind(),
        IoKind::WouldBlock
    );
    Ok(())
}

#[test]
fn sorted_global_scan_keeps_market_relative_rows_true_bounds_and_full_provenance() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        scan(
            &mut stream,
            &[
                (3, "A", "a"),
                (38, "X_A", "one"),
                (38, "X_B", "two"),
                (38, "X_C", "three"),
                (44, "Z", "z"),
            ],
            2,
        )?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.with_raw(RawPolicy::RetainWire).build()?;
    let pages = PageOptions::new(2, 8, 20)?;
    let snapshot = client.instrument_catalog(market(38), &pages, &OperationOptions::default())?;
    assert_eq!(snapshot.global_count(), Some(5));
    assert_eq!(snapshot.sorted_global_bounds(), Some((1, 4)));
    assert_eq!(snapshot.kind(), CatalogKind::ExInstruments);
    assert_eq!(snapshot.endpoint(), endpoint);
    assert_eq!(
        snapshot
            .rows()
            .iter()
            .map(|row| row.code())
            .collect::<Vec<_>>(),
        ["X_A", "X_B", "X_C"]
    );
    assert_eq!(snapshot.rows()[0].record_raw()[63], 0xa5);
    let query = InstrumentsQuery::new(market(38))
        .with_start(1)?
        .with_count(1)?
        .with_pages(pages)?;
    let page = client.instruments(&query, &OperationOptions::default())?;
    assert_eq!(page.rows()[0].code(), "X_B");
    assert_eq!(page.next_offset(), Some(2));
    assert_eq!(page.completion(), PageCompletion::RequestedCount);
    assert_eq!(page.server_total(), Some(3));
    let raw = page.meta().raw().expect("full directory evidence");
    assert_eq!(raw.payloads().len(), 5);
    assert_eq!(raw.wire_frames().unwrap().len(), 5);
    assert_eq!(raw.row_ranges(), &[(2, 6, 64)]);
    for source in page.meta().sources() {
        assert_eq!(source.endpoint(), Some(endpoint));
    }
    client.close()?;
    assert_eq!(snapshot.rows().len(), 3);
    join(task)
}

#[test]
fn absent_sorted_market_reports_its_real_insertion_range() -> Case {
    for (requested, bounds) in [(1, (0, 0)), (38, (1, 1)), (99, (2, 2))] {
        let (endpoint, task) = peer(|mut stream| {
            login(&mut stream)?;
            scan(&mut stream, &[(3, "A", "a"), (44, "Z", "z")], 600)?;
            eof(&mut stream)
        })?;
        let client = builder(endpoint)?.build()?;
        let snapshot = client.instrument_catalog(
            market(requested),
            &PageOptions::new(600, 8, 100)?,
            &OperationOptions::default(),
        )?;
        assert!(snapshot.rows().is_empty());
        assert_eq!(snapshot.sorted_global_bounds(), Some(bounds));
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn unsorted_global_directory_scans_all_occurrences_without_claiming_a_range() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        scan(
            &mut stream,
            &[
                (38, "X_A", "a"),
                (44, "Z", "z"),
                (38, "X_B", "b"),
                (2, "A", "a"),
            ],
            2,
        )?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let snapshot = client.instrument_catalog(
        market(38),
        &PageOptions::new(2, 8, 20)?,
        &OperationOptions::default(),
    )?;
    assert_eq!(
        snapshot
            .rows()
            .iter()
            .map(|row| row.code())
            .collect::<Vec<_>>(),
        ["X_A", "X_B"]
    );
    assert_eq!(snapshot.sorted_global_bounds(), None);
    assert!(diagnostic(snapshot.meta(), "DirectoryUnsorted"));
    client.close()?;
    join(task)
}

#[test]
fn empty_native_goods_fallback_preserves_actual_code_name_description_and_raw() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &[])?;
        scan(
            &mut stream,
            &[(3, "A", "a"), (38, "X_A", "Actual Name")],
            600,
        )?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.with_raw(RawPolicy::RetainWire).build()?;
    let page = client.goods(&GoodsQuery::new(market(38)), &OperationOptions::default())?;
    let row = &page.rows()[0];
    assert!(row.is_derived());
    assert_eq!(row.actual_code(), "X_A");
    assert_eq!(row.display_name(), Some("Actual Name"));
    assert_eq!(row.description(), Some("desc"));
    assert_eq!(row.derived_numeric_code(), None);
    assert_eq!(row.source_instrument().unwrap().record_raw()[63], 0xa5);
    assert!(diagnostic(page.meta(), "EmptyGoodsExFallback"));
    let raw = page.meta().raw().unwrap();
    assert_eq!(raw.payloads().len(), 4);
    assert_eq!(raw.row_ranges(), &[(2, 70, 64)]);
    client.close()?;
    join(task)
}

#[test]
fn native_goods_bits_remain_native_and_market_count_reuses_one_snapshot() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &["X_A", "X_B"])?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let rows = client.macro_indicators(&OperationOptions::default())?;
    assert_eq!(rows.len(), 2);
    assert!(!rows[0].is_derived());
    assert_eq!(rows[0].code()[0].bits(), (-0.0_f32).to_bits());
    assert_eq!(rows[0].display_name(), None);
    assert_eq!(
        client.goods_count(
            &GoodsCountQuery::new().with_market(Some(market(38))),
            &OperationOptions::default()
        )?,
        2
    );
    client.close()?;
    join(task)
}

#[test]
fn global_goods_count_is_the_direct_ex_count_without_market_scan() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        count(&mut stream, 12345)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    assert_eq!(
        client.goods_count(&GoodsCountQuery::new(), &OperationOptions::default())?,
        12345
    );
    client.close()?;
    join(task)
}

#[test]
fn final_count_change_rejects_a_moving_directory_instead_of_caching_it() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        count(&mut stream, 1)?;
        directory_request(&mut stream, 0, 1)?;
        frame(&mut stream, &directory_body(0, &[(38, "X_A", "a")])?)?;
        count(&mut stream, 2)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let error = client
        .instrument_catalog(
            market(38),
            &PageOptions::new(600, 8, 100)?,
            &OperationOptions::default(),
        )
        .expect_err("changed directory");
    assert_eq!(error.kind(), ErrorKind::Protocol);
    assert!(reason(&error, "DirectoryChanged"));
    client.close()?;
    join(task)
}

#[test]
fn wrong_offset_oversized_count_and_premature_empty_are_protocol_errors() -> Case {
    for variant in 0..3 {
        let (endpoint, task) = peer(move |mut stream| {
            login(&mut stream)?;
            count(&mut stream, 1)?;
            directory_request(&mut stream, 0, 1)?;
            let body = match variant {
                0 => directory_body(1, &[(38, "X_A", "a")])?,
                1 => directory_body(0, &[(38, "X_A", "a"), (38, "X_B", "b")])?,
                _ => directory_body(0, &[])?,
            };
            frame(&mut stream, &body)?;
            eof(&mut stream)
        })?;
        let client = builder(endpoint)?.build()?;
        assert_eq!(
            client
                .instrument_catalog(
                    market(38),
                    &PageOptions::new(600, 8, 100)?,
                    &OperationOptions::default()
                )
                .expect_err("malformed directory")
                .kind(),
            ErrorKind::Protocol
        );
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn total_request_limit_includes_the_final_count_fence() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        count(&mut stream, 1)?;
        directory_request(&mut stream, 0, 1)?;
        frame(&mut stream, &directory_body(0, &[(38, "X_A", "a")])?)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    assert_eq!(
        client
            .instrument_catalog(
                market(38),
                &PageOptions::new(600, 2, 100)?,
                &OperationOptions::default()
            )
            .expect_err("no third request")
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(task)
}

#[test]
fn global_record_limit_rejects_the_count_before_requesting_any_directory_page() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        count(&mut stream, 101)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    assert_eq!(
        client
            .instrument_catalog(
                market(38),
                &PageOptions::new(600, 8, 100)?,
                &OperationOptions::default()
            )
            .expect_err("record limit")
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(task)
}

#[test]
fn fallback_extra_budget_is_reserved_before_sending_the_ex_count() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &[])?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.with_extra_attempt_budget(0).build()?;
    assert_eq!(
        client
            .goods(&GoodsQuery::new(market(38)), &OperationOptions::default())
            .expect_err("no fallback authorization")
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(task)
}

#[test]
fn refresh_publishes_a_new_version_and_old_handles_survive_clear_and_close() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 100, &["X_A"])?;
        goods(&mut stream, 38, 0, 100, &["X_B"])?;
        goods(&mut stream, 38, 0, 100, &["X_C"])?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let pages = PageOptions::new(600, 8, 100)?;
    let options = OperationOptions::default();
    let old = client.goods_catalog(market(38), &pages, &options)?;
    assert!(Arc::ptr_eq(
        &old,
        &client.goods_catalog(market(38), &pages, &options)?
    ));
    let fresh = client.refresh_goods_catalog(market(38), &pages, &options)?;
    assert!(fresh.version() > old.version());
    assert_eq!(old.rows()[0].actual_code(), "X_A");
    assert_eq!(fresh.rows()[0].actual_code(), "X_B");
    client.clear_catalog_cache(&options)?;
    let third = client.goods_catalog(market(38), &pages, &options)?;
    assert!(third.version() > fresh.version());
    assert_eq!(third.rows()[0].actual_code(), "X_C");
    client.close()?;
    assert_eq!(old.rows()[0].actual_code(), "X_A");
    join(task)
}

#[test]
fn clear_during_an_inflight_load_prevents_its_late_republication() -> Case {
    let (observed, entered) = mpsc::channel();
    let (resume, release) = mpsc::channel();
    let (endpoint, task) = peer(move |mut stream| {
        login(&mut stream)?;
        count(&mut stream, 1)?;
        directory_request(&mut stream, 0, 1)?;
        observed.send(())?;
        release.recv_timeout(Duration::from_secs(2))?;
        frame(&mut stream, &directory_body(0, &[(38, "X_A", "a")])?)?;
        count(&mut stream, 1)?;
        scan(&mut stream, &[(38, "X_B", "b")], 600)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let caller = client.clone();
    let pages = PageOptions::new(600, 8, 100)?;
    let call_pages = pages.clone();
    let call = thread::spawn(move || {
        caller.instrument_catalog(market(38), &call_pages, &OperationOptions::default())
    });
    entered.recv_timeout(Duration::from_secs(2))?;
    client.clear_catalog_cache(&OperationOptions::default())?;
    resume.send(())?;
    let old = call
        .join()
        .map_err(|_| std::io::Error::other("catalog caller panicked"))??;
    let next = client.instrument_catalog(market(38), &pages, &OperationOptions::default())?;
    assert_eq!(old.rows()[0].code(), "X_A");
    assert_eq!(next.rows()[0].code(), "X_B");
    assert!(next.version() > old.version());
    client.close()?;
    join(task)
}

#[test]
fn raw_policy_override_does_not_reuse_a_different_policy_snapshot() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 100, &["X_A"])?;
        goods(&mut stream, 38, 0, 100, &["X_B"])?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let pages = PageOptions::new(600, 8, 100)?;
    let plain = client.goods_catalog(market(38), &pages, &OperationOptions::default())?;
    let raw = client.goods_catalog(
        market(38),
        &pages,
        &OperationOptions::default().with_raw(RawPolicy::RetainPayload),
    )?;
    assert!(plain.meta().raw().is_none());
    assert!(raw.meta().raw().is_some());
    assert!(!Arc::ptr_eq(&plain, &raw));
    assert_eq!(plain.rows()[0].actual_code(), "X_A");
    assert_eq!(raw.rows()[0].actual_code(), "X_B");
    client.close()?;
    join(task)
}

#[test]
fn macro_quote_resolution_is_exact_case_sensitive_unique_and_preserves_duplicates() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &["X_A", "X_A", "X_B"])?;
        quote_request(&mut stream, &["X_A", "X_B", "X_A"])?;
        frame(&mut stream, &quote_body(&["X_B", "X_A"])?)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let rows = client.macro_quotes(
        &selectors(&["A", "X_B", "A"])?,
        &OperationOptions::default(),
    )?;
    assert_eq!(
        rows.iter()
            .map(|row| row.id().code().as_str())
            .collect::<Vec<_>>(),
        ["X_A", "X_B", "X_A"]
    );
    let raw = rows[0].meta().raw().unwrap();
    assert_eq!(raw.row_ranges(), &[(1, 98, 72), (1, 26, 72), (1, 98, 72)]);
    assert!(std::ptr::eq(rows[0].meta(), rows[2].meta()));
    client.close()?;
    join(task)
}

#[test]
fn unknown_full_code_and_ambiguous_suffix_fail_before_quote_requests() -> Case {
    for (selector, kind) in [
        ("x_a", ErrorKind::MissingData),
        ("A", ErrorKind::InvalidArgument),
    ] {
        let (endpoint, task) = peer(|mut stream| {
            login(&mut stream)?;
            goods(&mut stream, 38, 0, 600, &["X_A", "Y_A"])?;
            eof(&mut stream)
        })?;
        let client = builder(endpoint)?.build()?;
        let error = client
            .macro_quotes(&selectors(&[selector])?, &OperationOptions::default())
            .expect_err("exact unique match required");
        assert_eq!(error.kind(), kind);
        if kind == ErrorKind::InvalidArgument {
            assert!(error
                .context()
                .iter()
                .any(|(key, value)| key == "candidates"
                    && value.contains("X_A")
                    && value.contains("Y_A")));
        }
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn native_code_only_goods_can_resolve_an_exact_name_via_same_source_directory() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &["X_A"])?;
        scan(&mut stream, &[(38, "X_A", "Actual Name")], 600)?;
        quote_request(&mut stream, &["X_A"])?;
        frame(&mut stream, &quote_body(&["X_A"])?)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let rows = client.macro_quotes(&selectors(&["Actual Name"])?, &OperationOptions::default())?;
    assert_eq!(rows[0].id().code().as_str(), "X_A");
    assert_eq!(rows[0].meta().sources().len(), 5);
    assert_eq!(rows[0].meta().raw().unwrap().row_ranges(), &[(4, 26, 72)]);
    client.close()?;
    join(task)
}

#[test]
fn resolved_but_missing_quote_is_missing_data_without_substituting_another_row() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &["X_A", "X_B"])?;
        quote_request(&mut stream, &["X_A"])?;
        frame(&mut stream, &quote_body(&["X_B"])?)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    assert_eq!(
        client
            .macro_quotes(&selectors(&["A"])?, &OperationOptions::default())
            .expect_err("requested identity missing")
            .kind(),
        ErrorKind::MissingData
    );
    client.close()?;
    join(task)
}

#[test]
fn source_order_quote_list_exposes_80_truncation_and_directory_continuation() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        let names: Vec<_> = (0..81).map(|index| format!("X_{index:03}")).collect();
        let codes: Vec<_> = names.iter().map(String::as_str).collect();
        goods(&mut stream, 38, 0, 600, &codes)?;
        quote_request(&mut stream, &codes[..80])?;
        let reversed: Vec<_> = codes[..80].iter().rev().copied().collect();
        frame(&mut stream, &quote_body(&reversed)?)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let result = client.quote_list(
        &QuoteListQuery::new(market(38)),
        &OperationOptions::default(),
    )?;
    assert_eq!(result.rows().len(), 80);
    assert_eq!(result.rows()[0].id().code().as_str(), "X_000");
    assert_eq!(result.rows()[79].id().code().as_str(), "X_079");
    assert_eq!(result.server_total(), Some(81));
    assert_eq!(result.next_offset(), Some(80));
    assert_eq!(result.completion(), PageCompletion::MoreAvailable);
    assert!(diagnostic(result.meta(), "SourceOrder80"));
    client.close()?;
    join(task)
}

#[test]
fn macro_bars_resolves_before_sending_checked_identity_and_retains_both_sources() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &["X_A"])?;
        let body = request(&mut stream, 1, 0x122e)?;
        assert_eq!(&body[2..5], b"X_A");
        assert_eq!(u16::from_le_bytes(body[24..26].try_into()?), 6);
        frame(&mut stream, &wire::bars(&[(20261117, 10.0)])?)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let query = MacroBarsQuery::new(MacroCode::new("A")?)?
        .with_count(1)?
        .with_adjust(eltdx::mac::MacAdjust::None);
    let result = client.macro_bars(&query, &OperationOptions::default())?;
    assert_eq!(result[0].id().code().as_str(), "X_A");
    assert_eq!(result[0].meta().sources().len(), 2);
    assert_eq!(result[0].meta().raw().unwrap().row_ranges(), &[(1, 33, 36)]);
    client.close()?;
    join(task)
}

#[test]
fn accepted_count_prevents_failover_for_the_directory_page() -> Case {
    let unused = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let other = unused.local_addr()?;
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        count(&mut stream, 1)?;
        directory_request(&mut stream, 0, 1)?;
        Ok(())
    })?;
    let client = builder(endpoint)?
        .with_endpoints(vec![endpoint, other])
        .build()?;
    assert_eq!(
        client
            .instrument_catalog(
                market(38),
                &PageOptions::new(600, 8, 100)?,
                &OperationOptions::default()
            )
            .expect_err("pinned page loss")
            .kind(),
        ErrorKind::Connection
    );
    no_connection(&unused)?;
    client.close()?;
    join(task)
}

#[test]
fn preaccept_count_failure_can_failover_and_the_new_catalog_uses_only_the_accepted_source() -> Case
{
    let (first, first_task) = peer(|mut stream| {
        login(&mut stream)?;
        request(&mut stream, 1, 0x23f0)?;
        Ok(())
    })?;
    let (second, second_task) = peer(|mut stream| {
        login(&mut stream)?;
        scan(&mut stream, &[(38, "X_A", "a")], 600)?;
        eof(&mut stream)
    })?;
    let client = builder(first)?
        .with_endpoints(vec![first, second])
        .build()?;
    let result = client.instrument_catalog(
        market(38),
        &PageOptions::new(600, 8, 100)?,
        &OperationOptions::default(),
    )?;
    assert_eq!(result.endpoint(), second);
    for source in result.meta().sources() {
        assert_eq!(source.endpoint(), Some(second));
    }
    client.close()?;
    join(first_task)?;
    join(second_task)
}

#[test]
fn cancel_or_close_during_directory_half_frame_retires_the_owner_worker() -> Case {
    for close in [false, true] {
        let (observed, entered) = mpsc::channel();
        let (endpoint, task) = peer(move |mut stream| {
            login(&mut stream)?;
            count(&mut stream, 1)?;
            directory_request(&mut stream, 0, 1)?;
            let bytes = frame_bytes(&directory_body(0, &[(38, "X_A", "a")])?)?;
            stream.write_all(&bytes[..20])?;
            observed.send(())?;
            eof(&mut stream)
        })?;
        let client = builder(endpoint)?.build()?;
        let caller = client.clone();
        let token = CancellationToken::default();
        let options = OperationOptions::default().with_cancellation(token.clone());
        let call = thread::spawn(move || {
            caller.instrument_catalog(market(38), &PageOptions::new(600, 8, 100)?, &options)
        });
        entered.recv_timeout(Duration::from_secs(2))?;
        if close {
            client.close()?;
        } else {
            token.cancel();
        }
        let error = call
            .join()
            .map_err(|_| std::io::Error::other("directory caller panicked"))?
            .expect_err("interrupted directory");
        assert_eq!(
            error.kind(),
            if close {
                ErrorKind::Closed
            } else {
                ErrorKind::Cancelled
            }
        );
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn cancelled_or_expired_entry_starts_no_connection() -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    for variant in 0..2 {
        let client = builder(listener.local_addr()?)?.build()?;
        let token = CancellationToken::default();
        token.cancel();
        let options = if variant == 0 {
            OperationOptions::default().with_cancellation(token)
        } else {
            OperationOptions::default().with_timeout(Duration::ZERO)
        };
        let error = client
            .goods(&GoodsQuery::new(market(38)), &options)
            .expect_err("pre-I/O gate");
        assert_eq!(
            error.kind(),
            if variant == 0 {
                ErrorKind::Cancelled
            } else {
                ErrorKind::Timeout
            }
        );
        no_connection(&listener)?;
        client.close()?;
    }
    Ok(())
}

#[test]
fn tiny_byte_budget_stops_during_the_login_frame_before_any_business_request() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        request(&mut stream, 1, 0x2454)?;
        // The peer may observe retirement while its one synthetic write is in
        // flight. No EOF timing assumption is needed for this byte gate.
        let _ = stream.write_all(&frame_bytes(b"catalog fixture accepted")?);
        Ok(())
    })?;
    let client = builder(endpoint)?
        .with_max_operation_bytes(NonZeroUsize::new(1).unwrap())
        .build()?;
    assert_eq!(
        client
            .goods(&GoodsQuery::new(market(38)), &OperationOptions::default())
            .expect_err("streamed byte gate")
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(task)
}

#[test]
fn unsupported_sort_and_invalid_macro_selectors_are_local_errors() -> Case {
    assert!(MacroCode::new("").is_err());
    assert!(MacroCode::new("A\0B").is_err());
    assert!(MacroCode::new("a".repeat(513)).is_err());
    assert!(MacroCode::new("Long Exact Display Name").is_ok());
    assert!(QuoteListQuery::new(market(38))
        .with_sort(eltdx::mac::SortType::ChangePct, eltdx::mac::SortOrder::Desc)
        .is_err());
    assert!(MacroQuotesQuery::new(vec![]).is_err());
    let _ = FieldSelection::empty();
    Ok(())
}

#[test]
fn a_source_replacement_invalidates_cached_goods_but_keeps_the_old_arc_readable() -> Case {
    let (first, first_task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 100, &["X_A"])?;
        request(&mut stream, 1, 0x23f0)?;
        Ok(())
    })?;
    let (second, second_task) = peer(|mut stream| {
        login(&mut stream)?;
        count(&mut stream, 999)?;
        goods(&mut stream, 38, 0, 100, &["X_B"])?;
        eof(&mut stream)
    })?;
    let client = builder(first)?
        .with_endpoints(vec![first, second])
        .build()?;
    let pages = PageOptions::new(600, 8, 100)?;
    let options = OperationOptions::default();
    let old = client.goods_catalog(market(38), &pages, &options)?;
    assert_eq!(client.goods_count(&GoodsCountQuery::new(), &options)?, 999);
    let next = client.goods_catalog(market(38), &pages, &options)?;
    assert_eq!(old.endpoint(), first);
    assert_eq!(next.endpoint(), second);
    assert_ne!(old.generation(), next.generation());
    assert_eq!(old.rows()[0].actual_code(), "X_A");
    assert_eq!(next.rows()[0].actual_code(), "X_B");
    assert!(next.version() > old.version());
    client.close()?;
    join(first_task)?;
    join(second_task)
}

#[test]
fn independent_owners_at_one_endpoint_never_share_catalogs_or_close_state() -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = listener.local_addr()?;
    let task = thread::spawn(move || -> Case {
        let (mut first, _) = listener.accept()?;
        first.set_read_timeout(Some(Duration::from_secs(3)))?;
        login(&mut first)?;
        goods(&mut first, 38, 0, 100, &["X_A"])?;
        let (mut second, _) = listener.accept()?;
        second.set_read_timeout(Some(Duration::from_secs(3)))?;
        login(&mut second)?;
        goods(&mut second, 38, 0, 100, &["X_B"])?;
        eof(&mut first)?;
        eof(&mut second)
    });
    let first = builder(endpoint)?.build()?;
    let second = builder(endpoint)?.build()?;
    let pages = PageOptions::new(600, 8, 100)?;
    let options = OperationOptions::default();
    let a = first.goods_catalog(market(38), &pages, &options)?;
    let b = second.goods_catalog(market(38), &pages, &options)?;
    assert_ne!(a.owner_id(), b.owner_id());
    assert_eq!(a.rows()[0].actual_code(), "X_A");
    assert_eq!(b.rows()[0].actual_code(), "X_B");
    first.close()?;
    assert!(Arc::ptr_eq(
        &b,
        &second.goods_catalog(market(38), &pages, &options)?
    ));
    second.close()?;
    join(task)
}

#[test]
fn macro_quotes_eighty_plus_one_batches_keep_requested_occurrences_and_raw_mappings() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &["X_A", "X_B"])?;
        let expected: Vec<_> = (0..80)
            .map(|index| if index % 2 == 0 { "X_A" } else { "X_B" })
            .collect();
        quote_request(&mut stream, &expected)?;
        frame(&mut stream, &quote_body(&["X_B", "X_A"])?)?;
        quote_request(&mut stream, &["X_A"])?;
        frame(&mut stream, &quote_body(&["X_A"])?)?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let codes: Vec<_> = (0..81)
        .map(|index| MacroCode::new(if index % 2 == 0 { "A" } else { "B" }))
        .collect::<eltdx::Result<_>>()?;
    let result =
        client.macro_quotes(&MacroQuotesQuery::new(codes)?, &OperationOptions::default())?;
    assert_eq!(result.len(), 81);
    assert_eq!(result[80].id().code().as_str(), "X_A");
    let meta = result[80].meta();
    assert_eq!(meta.sources().len(), 3);
    let ranges = meta.raw().unwrap().row_ranges();
    assert_eq!(ranges.len(), 81);
    assert_eq!(ranges[0], (1, 98, 72));
    assert_eq!(ranges[1], (1, 26, 72));
    assert_eq!(ranges[80], (2, 26, 72));
    client.close()?;
    join(task)
}

#[test]
fn exact_display_name_collision_in_fallback_directory_is_ambiguous() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &[])?;
        scan(
            &mut stream,
            &[(38, "X_A", "Same Name"), (38, "X_B", "Same Name")],
            600,
        )?;
        eof(&mut stream)
    })?;
    let client = builder(endpoint)?.build()?;
    let error = client
        .macro_quotes(&selectors(&["Same Name"])?, &OperationOptions::default())
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::InvalidArgument);
    client.close()?;
    join(task)
}

#[test]
fn empty_global_fallback_and_out_of_range_market_page_have_observed_end_without_quotes() -> Case {
    for empty in [false, true] {
        let (endpoint, task) = peer(move |mut stream| {
            login(&mut stream)?;
            if empty {
                goods(&mut stream, 38, 0, 600, &[])?;
                scan(&mut stream, &[], 600)?;
            } else {
                goods(&mut stream, 38, 0, 600, &["X_A", "X_B"])?;
            }
            eof(&mut stream)
        })?;
        let client = builder(endpoint)?
            .with_raw(RawPolicy::RetainPayload)
            .build()?;
        let result = client.quote_list(
            &QuoteListQuery::new(market(38)).with_start(30)?,
            &OperationOptions::default(),
        )?;
        assert!(result.rows().is_empty());
        assert_eq!(result.next_offset(), Some(30));
        assert_eq!(result.server_total(), Some(if empty { 0 } else { 2 }));
        assert_eq!(result.completion(), PageCompletion::EndOfData);
        assert!(result.meta().raw().unwrap().row_ranges().is_empty());
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn source_bound_macro_bars_never_fails_over_after_its_directory_is_accepted() -> Case {
    let unused = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let (endpoint, task) = peer(|mut stream| {
        login(&mut stream)?;
        goods(&mut stream, 38, 0, 600, &["X_A"])?;
        request(&mut stream, 1, 0x122e)?;
        Ok(())
    })?;
    let client = builder(endpoint)?
        .with_endpoints(vec![endpoint, unused.local_addr()?])
        .build()?;
    let query = MacroBarsQuery::new(MacroCode::new("A")?)?
        .with_count(1)?
        .with_adjust(eltdx::mac::MacAdjust::None);
    assert_eq!(
        client
            .macro_bars(&query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Connection
    );
    no_connection(&unused)?;
    client.close()?;
    join(task)
}
