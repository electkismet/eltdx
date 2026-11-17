#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Authored public board contracts. Execution is deferred until all T01-T30 sources exist.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"]
mod wire;
use eltdx::mac::models::{Evidence, NativeUnit};
use eltdx::mac::*;
use eltdx::{ErrorKind, OperationOptions, PageOptions, RawPolicy, TradingDate};
use std::io::Write;
use std::net::TcpStream;
use std::sync::mpsc;
use wire::{bars, bars_request, eof, frame, join, peer, request, setup_mac, Case};

fn directory(stream: &mut TcpStream, values: &[(&str, f32, f32)]) -> Case {
    for (offset, rows) in [(0, values), (u16::try_from(values.len())?, &[][..])] {
        let body = request(stream, 0x1c, 0x1231)?;
        assert_eq!(u16::from_le_bytes(body[6..8].try_into()?), offset);
        assert_eq!(u16::from_le_bytes(body[..2].try_into()?), 150);
        let mut response = (u16::try_from(rows.len())? * 2).to_le_bytes().to_vec();
        response.extend_from_slice(&999_u16.to_le_bytes());
        for &(code, price, pre) in rows {
            for (code, name, price, pre) in
                [(code, "board", price, pre), ("600000", "leader", 10.0, 9.0)]
            {
                let mut side = [0; 80];
                side[2..2 + code.len()].copy_from_slice(code.as_bytes());
                side[24..24 + name.len()].copy_from_slice(name.as_bytes());
                side[68..72].copy_from_slice(&price.to_bits().to_le_bytes());
                side[72..76].copy_from_slice(&(-0.0_f32).to_bits().to_le_bytes());
                side[76..80].copy_from_slice(&pre.to_bits().to_le_bytes());
                response.extend_from_slice(&side);
            }
        }
        frame(stream, &response)?;
    }
    Ok(())
}
fn members(
    stream: &mut TcpStream,
    board: u32,
    start: u32,
    count: u16,
    values: &[(f32, u32, f32, f32, f32, f32)],
) -> Case {
    let body = request(stream, 0x1c, 0x122c)?;
    assert_eq!(u32::from_le_bytes(body[..4].try_into()?), board);
    assert_eq!(u32::from_le_bytes(body[15..19].try_into()?), start);
    assert_eq!(u16::from_le_bytes(body[19..21].try_into()?), count);
    // The required source bits are independently fixed, not obtained from an SDK encoder.
    for bit in [0_usize, 1, 2, 3, 4, 5, 7, 56, 111, 112] {
        assert_ne!(body[23 + bit / 8] & (1 << (bit % 8)), 0);
    }
    let mut response = vec![0; 26];
    let bits = [0_usize, 4, 5, 7, 47, 56, 111, 112];
    for bit in bits {
        response[bit / 8] |= 1 << (bit % 8);
    }
    response[20..24].copy_from_slice(&u32::try_from(values.len())?.to_le_bytes());
    response[24..26].copy_from_slice(&u16::try_from(values.len())?.to_le_bytes());
    for &(close, volume, amount, net, net3, net5) in values {
        let mut row = [0; 68];
        row[2..8].copy_from_slice(b"600000");
        row[24..30].copy_from_slice(b"member");
        response.extend_from_slice(&row);
        for bits in [
            10.0_f32.to_bits(),
            close.to_bits(),
            volume,
            amount.to_bits(),
            0x1234_5678,
            net.to_bits(),
            net3.to_bits(),
            net5.to_bits(),
        ] {
            response.extend_from_slice(&bits.to_le_bytes());
        }
    }
    frame(stream, &response)
}
fn has(meta: &eltdx::types::ResultMeta, stage: &str) -> bool {
    meta.diagnostics()
        .iter()
        .any(|value| value.stage() == stage)
}
fn scalar(amount: f32) -> (f32, u32, f32, f32, f32, f32) {
    (11.0, 2, amount, amount, amount, amount)
}

#[test]
fn summary_collects_all_members_preserves_units_unknown_slots_and_exact_u64_volume() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        members(
            &mut stream,
            20001,
            0,
            80,
            &vec![(11.0, u32::MAX, 2.0, 3.0, 4.0, 5.0); 80],
        )?;
        members(&mut stream, 20001, 80, 80, &[(9.0, 7, 6.0, 7.0, 8.0, 9.0)])?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainWire)
        .build()?;
    let result = client.board_summary(
        &BoardSummaryQuery::new(BoardSymbol::new("880001")?),
        &OperationOptions::default(),
    )?;
    assert_eq!(result.member_count(), 81);
    assert_eq!(result.up_count(), 80);
    assert_eq!(result.down_count(), 1);
    assert_eq!(*result.vol().value(), u64::from(u32::MAX) * 80 + 7);
    assert_eq!(*result.amount().value(), 166.0);
    assert_eq!(*result.main_net_amount().value(), 247.0);
    assert_eq!(*result.main_net_3d().value(), 328.0);
    assert_eq!(*result.main_net_5d().value(), 409.0);
    assert_eq!(
        result.amount().meaning().unit(),
        &NativeUnit::Currency("元".into())
    );
    assert_eq!(result.vol().meaning().evidence(), Evidence::Unknown);
    assert_eq!(result.extra().len(), 81);
    assert_eq!(result.extra()[0].name(), "members[0].bit_47");
    assert_eq!(
        result.extra()[0].value(),
        &NativeValue::Bytes(vec![0x78, 0x56, 0x34, 0x12])
    );
    assert_eq!(result.members()[80].fields()[4].bit.get(), 47);
    assert_eq!(
        result.meta().raw().expect("source rows").row_ranges().len(),
        81
    );
    assert!(!has(result.meta(), "MissingAggregateField"));
    client.close()?;
    join(task)
}

#[test]
fn summary_rejects_a_full_member_cap_without_eof_evidence() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        members(&mut stream, 20001, 0, 80, &vec![scalar(1.0); 80])?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let query = BoardSummaryQuery::new(BoardSymbol::new("880001")?)
        .with_pages(PageOptions::new(80, 1, 80)?)?;
    assert_eq!(
        client
            .board_summary(&query, &OperationOptions::default())
            .expect_err("no terminal evidence")
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(task)
}

#[test]
fn money_ranking_aggregates_the_third_candidate_before_top_two_and_keeps_stable_ties() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(
            &mut stream,
            &[
                ("880001", 12.0, 10.0),
                ("880002", 11.0, 10.0),
                ("880003", 10.0, 10.0),
            ],
        )?;
        for (board, amount) in [(20001, 10.0), (20002, 10.0), (20003, 100.0)] {
            members(&mut stream, board, 0, 80, &[scalar(amount)])?;
        }
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainWire)
        .build()?;
    let query = BoardRankingQuery::new()
        .with_sort(BoardRankKey::Amount)
        .with_top_n(Some(2))?;
    let rows = client.board_ranking(&query, &OperationOptions::default())?;
    assert_eq!(
        rows.iter().map(|row| row.code()).collect::<Vec<_>>(),
        ["880003", "880001"]
    );
    assert_eq!(*rows[0].summary().amount().value(), 100.0);
    let raw = rows[0].meta().raw().expect("all source observations");
    assert_eq!(raw.payloads().len(), 5);
    assert_eq!(raw.row_ranges(), &[(0, 324, 160), (0, 4, 160)]);
    assert!(std::ptr::eq(rows[0].meta(), rows[1].meta()));
    assert_eq!(rows[0].source().leader().id().code().as_str(), "600000");
    client.close()?;
    join(task)
}

#[test]
fn volume_and_multiday_money_metrics_use_all_candidates_and_native_values() -> Case {
    for key in [
        BoardRankKey::Volume,
        BoardRankKey::MainNetAmount,
        BoardRankKey::MainNet3d,
        BoardRankKey::MainNet5d,
    ] {
        let (endpoint, task) = peer(|mut stream| {
            setup_mac(&mut stream)?;
            directory(
                &mut stream,
                &[("880001", 12.0, 10.0), ("880002", 11.0, 10.0)],
            )?;
            members(
                &mut stream,
                20001,
                0,
                80,
                &[(11.0, 1, 100.0, 1.0, 1.0, 1.0)],
            )?;
            members(&mut stream, 20002, 0, 80, &[(11.0, 2, 1.0, 2.0, 2.0, 2.0)])?;
            eof(&mut stream)
        })?;
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let rows = client.board_ranking(
            &BoardRankingQuery::new()
                .with_sort(key)
                .with_top_n(Some(1))?,
            &OperationOptions::default(),
        )?;
        assert_eq!(rows[0].code(), "880002");
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn zero_reference_is_absent_and_sorts_after_known_changes_in_either_direction() -> Case {
    for ascending in [false, true] {
        let (endpoint, task) = peer(move |mut stream| {
            setup_mac(&mut stream)?;
            directory(
                &mut stream,
                &[
                    ("880001", 12.0, 10.0),
                    ("880002", 11.0, 10.0),
                    ("880003", 100.0, 0.0),
                ],
            )?;
            for code in if ascending {
                [20002, 20001, 20003]
            } else {
                [20001, 20002, 20003]
            } {
                members(&mut stream, code, 0, 80, &[scalar(1.0)])?;
            }
            eof(&mut stream)
        })?;
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let rows = client.board_ranking(
            &BoardRankingQuery::new()
                .with_top_n(None)?
                .with_ascending(ascending),
            &OperationOptions::default(),
        )?;
        assert_eq!(rows.last().expect("all boards").change_pct(), None);
        assert!(has(rows[0].meta(), "InvalidBoardReference"));
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn change_ranking_truncates_only_after_valid_sort_and_fetches_selected_summaries() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(
            &mut stream,
            &[
                ("880001", 11.0, 10.0),
                ("880002", 12.0, 10.0),
                ("880003", 100.0, 0.0),
            ],
        )?;
        members(&mut stream, 20002, 0, 80, &[scalar(1.0)])?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let rows = client.board_ranking(
        &BoardRankingQuery::new().with_top_n(Some(1))?,
        &OperationOptions::default(),
    )?;
    assert_eq!(rows[0].code(), "880002");
    assert_eq!(rows[0].change_pct(), Some(20.0));
    assert!(has(rows[0].meta(), "InvalidBoardReference"));
    client.close()?;
    join(task)
}

#[test]
fn composite_page_budget_is_shared_across_directory_and_member_collections() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(
            &mut stream,
            &[("880001", 11.0, 10.0), ("880002", 12.0, 10.0)],
        )?;
        members(&mut stream, 20001, 0, 80, &[scalar(1.0)])?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let query = BoardRankingQuery::new()
        .with_sort(BoardRankKey::Amount)
        .with_pages(PageOptions::new(150, 3, 1000)?)?;
    assert_eq!(
        client
            .board_ranking(&query, &OperationOptions::default())
            .expect_err("fourth page is refused before send")
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(task)
}

#[test]
fn required_member_protocol_failure_is_not_downgraded_to_an_empty_summary() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(&mut stream, &[("880001", 11.0, 10.0)])?;
        request(&mut stream, 0x1c, 0x122c)?;
        frame(&mut stream, b"malformed required member page")?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    assert_eq!(
        client
            .board_ranking(&BoardRankingQuery::new(), &OperationOptions::default())
            .expect_err("required decode failure")
            .kind(),
        ErrorKind::Protocol
    );
    client.close()?;
    join(task)
}

#[test]
fn historical_target_reads_older_pages_requires_full_intervals_and_keeps_missing_evidence() -> Case
{
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(
            &mut stream,
            &[
                ("880001", 11.0, 10.0),
                ("880002", 12.0, 10.0),
                ("880003", 13.0, 10.0),
            ],
        )?;
        for board in ["880001", "880002", "880003"] {
            let request_body = request(&mut stream, 0x1c, 0x122e)?;
            assert_eq!(&request_body[2..8], board.as_bytes());
            assert_eq!(&request_body[28..32], &0_u32.to_le_bytes());
            assert_eq!(&request_body[32..34], &3_u16.to_le_bytes());
            if board == "880002" {
                frame(&mut stream, &bars(&[(20261116, 5.0)])?)?;
                continue;
            }
            frame(
                &mut stream,
                &bars(&[(20261117, 13.0), (20261118, 14.0), (20261119, 15.0)])?,
            )?;
            bars_request(&mut stream, 0x1c, 0, 3, 3)?;
            frame(
                &mut stream,
                &bars(&[(20261114, 10.0), (20261115, 11.0), (20261116, 12.0)])?,
            )?;
        }
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let query = BoardChangeRankingQuery::new()
        .with_days(2)?
        .with_target_date(Some(TradingDate::new(2026, 11, 16)?))?
        .with_history_pages(PageOptions::new(3, 5, 15)?)?;
    let rows = client.board_change_ranking(&query, &OperationOptions::default())?;
    assert_eq!(
        rows.iter().map(|row| row.code()).collect::<Vec<_>>(),
        ["880001", "880003"]
    );
    assert_eq!(rows[0].change_pct(), 20.0);
    assert_eq!(rows[0].close_start().bits(), 10.0_f32.to_bits());
    assert_eq!(rows[0].start_date(), TradingDate::new(2026, 11, 14)?);
    assert_eq!(rows[0].end_date(), TradingDate::new(2026, 11, 16)?);
    assert!(has(rows[0].meta(), "InsufficientBoardHistory"));
    assert_eq!(
        rows[0]
            .meta()
            .raw()
            .expect("selected board rows")
            .row_ranges(),
        &[(0, 4, 160), (0, 324, 160)]
    );
    client.close()?;
    join(task)
}

#[test]
fn full_history_cap_is_an_error_instead_of_an_invented_partial_return() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(&mut stream, &[("880001", 11.0, 10.0)])?;
        bars_request(&mut stream, 0x1c, 0, 0, 2)?;
        frame(&mut stream, &bars(&[(20261117, 10.0), (20261118, 11.0)])?)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let query = BoardChangeRankingQuery::new()
        .with_days(2)?
        .with_history_pages(PageOptions::new(2, 1, 2)?)?;
    assert_eq!(
        client
            .board_change_ranking(&query, &OperationOptions::default())
            .expect_err("full cap cannot prove missing history")
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(task)
}

#[test]
fn invalid_history_prices_are_diagnosed_even_when_no_rank_can_be_returned() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(&mut stream, &[("880001", 11.0, 10.0)])?;
        bars_request(&mut stream, 0x1c, 0, 0, 12)?;
        frame(
            &mut stream,
            &bars(&[(20261116, 0.0), (20261117, 1.0), (20261118, 2.0)])?,
        )?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let rows = client.board_change_ranking(
        &BoardChangeRankingQuery::new().with_days(2)?,
        &OperationOptions::default(),
    )?;
    assert!(rows.is_empty());
    assert!(has(
        client
            .diagnostics()?
            .latest_meta()
            .expect("empty result still has evidence"),
        "InvalidBoardHistoryPrice"
    ));
    client.close()?;
    join(task)
}

#[test]
fn board_composite_queries_validate_optional_top_and_independent_child_policies() -> Case {
    assert_eq!(
        BoardRankingQuery::new().top_n().expect("default50").get(),
        50
    );
    assert!(BoardRankingQuery::new().with_top_n(None)?.top_n().is_none());
    assert_eq!(
        BoardRankingQuery::new()
            .with_top_n(Some(0))
            .expect_err("positive when present")
            .kind(),
        ErrorKind::InvalidArgument
    );
    assert!(BoardRankingQuery::new()
        .with_member_pages(PageOptions::new(81, 2, 100)?)
        .is_err());
    assert!(BoardChangeRankingQuery::new()
        .with_history_pages(PageOptions::new(701, 2, 1000)?)
        .is_err());
    assert!(BoardChangeRankingQuery::new().with_days(u32::MAX).is_err());
    Ok(())
}

#[test]
fn absent_member_fields_expose_coverage_and_diagnostics_without_inventing_wire_zero() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        request(&mut stream, 0x1c, 0x122c)?;
        let mut response = vec![0; 26];
        response[0] = 0x31; // only pre-close, close, volume
        response[20..24].copy_from_slice(&1_u32.to_le_bytes());
        response[24..26].copy_from_slice(&1_u16.to_le_bytes());
        let mut row = [0; 68];
        row[2..8].copy_from_slice(b"600000");
        response.extend_from_slice(&row);
        for value in [10.0_f32.to_bits(), 11.0_f32.to_bits(), 2] {
            response.extend_from_slice(&value.to_le_bytes());
        }
        frame(&mut stream, &response)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let result = client.board_summary(
        &BoardSummaryQuery::new(BoardSymbol::new("880001")?),
        &OperationOptions::default(),
    )?;
    assert_eq!(*result.amount().value(), 0.0);
    assert_eq!(result.missing_member_count(FieldId::new(7)?), Some(1));
    assert_eq!(result.missing_member_count(FieldId::new(5)?), Some(0));
    assert!(result.members()[0].field(FieldId::new(7)?).is_none());
    assert!(has(result.meta(), "MissingAggregateField"));
    client.close()?;
    join(task)
}

#[test]
fn nonfinite_money_is_retained_and_sorts_after_known_values_in_either_direction() -> Case {
    for ascending in [false, true] {
        let (endpoint, task) = peer(|mut stream| {
            setup_mac(&mut stream)?;
            directory(
                &mut stream,
                &[("880001", 11.0, 10.0), ("880002", 12.0, 10.0)],
            )?;
            members(
                &mut stream,
                20001,
                0,
                80,
                &[scalar(f32::from_bits(0x7fc0_1234))],
            )?;
            members(&mut stream, 20002, 0, 80, &[scalar(-5.0)])?;
            eof(&mut stream)
        })?;
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let rows = client.board_ranking(
            &BoardRankingQuery::new()
                .with_sort(BoardRankKey::Amount)
                .with_ascending(ascending),
            &OperationOptions::default(),
        )?;
        assert_eq!(rows[0].code(), "880002");
        assert!(rows[1].summary().amount().value().is_nan());
        assert!(has(rows[1].meta(), "NonfiniteAggregateField"));
        let field = rows[1].summary().members()[0]
            .field(FieldId::new(7)?)
            .expect("actual NaN slot");
        assert_eq!(
            field.value,
            eltdx::mac::models::SlotValue::F32(WireFloat::from_bits(0x7fc0_1234))
        );
        client.close()?;
        join(task)?;
    }
    Ok(())
}

#[test]
fn duplicate_daily_observations_are_retained_but_do_not_supply_missing_trading_intervals() -> Case {
    let (endpoint, task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        directory(&mut stream, &[("880001", 11.0, 10.0)])?;
        bars_request(&mut stream, 0x1c, 0, 0, 3)?;
        frame(
            &mut stream,
            &bars(&[(20261117, 11.0), (20261118, 99.0), (20261118, 12.0)])?,
        )?;
        bars_request(&mut stream, 0x1c, 0, 3, 3)?;
        frame(&mut stream, &bars(&[(20261116, 10.0)])?)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let query = BoardChangeRankingQuery::new()
        .with_days(2)?
        .with_history_pages(PageOptions::new(3, 5, 15)?)?;
    let rows = client.board_change_ranking(&query, &OperationOptions::default())?;
    assert_eq!(rows[0].change_pct(), 20.0);
    assert_eq!(rows[0].close_end().bits(), 12.0_f32.to_bits());
    assert!(has(rows[0].meta(), "DuplicateBoardHistoryDate"));
    assert_eq!(
        rows[0]
            .meta()
            .raw()
            .expect("directory plus both bar pages")
            .payloads()
            .len(),
        4
    );
    client.close()?;
    join(task)
}

#[test]
fn cancellation_and_owner_close_during_required_member_half_frame_abort_the_composite() -> Case {
    for close_owner in [false, true] {
        let (ready_tx, ready_rx) = mpsc::sync_channel(0);
        let (endpoint, task) = peer(move |mut stream| {
            setup_mac(&mut stream)?;
            directory(&mut stream, &[("880001", 11.0, 10.0)])?;
            request(&mut stream, 0x1c, 0x122c)?;
            stream.write_all(&wire::frame_bytes(&[0; 200])?[..13])?;
            ready_tx.send(())?;
            eof(&mut stream)
        })?;
        let token = eltdx::CancellationToken::default();
        let options = OperationOptions::default().with_cancellation(token.clone());
        let client = MacClient::builder()
            .with_endpoints(vec![endpoint])
            .build()?;
        let caller = client.clone();
        let call =
            std::thread::spawn(move || caller.board_ranking(&BoardRankingQuery::new(), &options));
        ready_rx.recv()?;
        if close_owner {
            client.close()?;
        } else {
            token.cancel();
        }
        let result = call
            .join()
            .map_err(|_| std::io::Error::other("board caller panicked"))?;
        assert_eq!(
            result
                .expect_err("terminal operation is never an empty rank")
                .kind(),
            if close_owner {
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
