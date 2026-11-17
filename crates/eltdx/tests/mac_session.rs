#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Public-client fixture sources. These synthetic replies are not production
//! authentication or response-association evidence. Execution is deferred.
#![cfg(feature = "mac")]
use bytes::Bytes;
use eltdx::mac::handshake::{
    LoginReplyRules, MacSetupPolicy, OptionalSetupBoundary, ResponseIdentity, SetupReply,
};
use eltdx::mac::*;
use eltdx::types::BarTime;
use eltdx::{CancellationToken, ErrorKind, OperationOptions, PageOptions, RawPolicy, TradingDate};
use std::error::Error as StdError;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread::{self, JoinHandle};
use std::time::Duration;
type Case<T = ()> = Result<T, Box<dyn StdError + Send + Sync>>;

fn peer(
    script: impl FnOnce(TcpStream) -> Case + Send + 'static,
) -> Case<(Endpoint, JoinHandle<Case>)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = listener.local_addr()?;
    let job = thread::spawn(move || {
        let (stream, _) = listener.accept()?;
        stream.set_read_timeout(Some(Duration::from_secs(3)))?;
        stream.set_write_timeout(Some(Duration::from_secs(3)))?;
        script(stream)
    });
    Ok((endpoint, job))
}
fn join(job: JoinHandle<Case>) -> Case {
    job.join()
        .map_err(|_| std::io::Error::other("MAC fixture panicked"))?
}
fn eof(stream: &mut TcpStream) -> Case {
    match stream.read(&mut [0]) {
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted
            ) => {}
        result => assert_eq!(result?, 0),
    }
    Ok(())
}
fn hex(text: &str) -> Case<Vec<u8>> {
    let mut bytes = Vec::new();
    for offset in (0..text.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&text[offset..offset + 2], 16)?);
    }
    Ok(bytes)
}
fn exact(stream: &mut TcpStream, expected: &[u8]) -> Case {
    let mut bytes = vec![0; expected.len()];
    stream.read_exact(&mut bytes)?;
    assert_eq!(bytes, expected);
    Ok(())
}
fn frame(stream: &mut TcpStream, method: u32, body: &[u8]) -> Case {
    let mut bytes = 7_654_321_u32.to_le_bytes().to_vec();
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(&method.to_le_bytes());
    let len = u16::try_from(body.len())?;
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(&len.to_le_bytes());
    bytes.extend_from_slice(body);
    for chunk in bytes.chunks(3) {
        stream.write_all(chunk)?;
    }
    Ok(())
}
fn required_setup(stream: &mut TcpStream) -> Case {
    for packet in [
        "0c0218930001030003000d0001",
        "0c0218940001030003000d0002",
        "0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002",
    ] {
        exact(stream, &hex(packet)?)?;
        frame(stream, 0, b"synthetic setup")?;
    }
    Ok(())
}
fn request(stream: &mut TcpStream, expected_head: u8, expected_command: u16) -> Case<Vec<u8>> {
    let mut header = [0; 12];
    stream.read_exact(&mut header)?;
    assert_eq!(header[0], expected_head);
    assert_eq!(&header[6..8], &header[8..10]);
    assert_eq!(
        u16::from_le_bytes([header[10], header[11]]),
        expected_command
    );
    let size = usize::from(u16::from_le_bytes([header[6], header[7]]));
    let mut body = vec![
        0;
        size.checked_sub(2)
            .ok_or_else(|| std::io::Error::other("bad source request"))?
    ];
    stream.read_exact(&mut body)?;
    Ok(body)
}
fn id() -> eltdx::Result<MacInstrumentId> {
    Ok(MacInstrumentId::new(
        MacMarketId::new(0),
        MacCode::new("600000")?,
    ))
}
fn quote_body() -> Vec<u8> {
    // Independent nonempty row: known float bit0 and unknown legal bit47.
    let mut body = vec![0; 26 + 68 + 8];
    body[0] = 1;
    body[5] = 0x80;
    body[20..24].copy_from_slice(&1_u32.to_le_bytes());
    body[24..26].copy_from_slice(&1_u16.to_le_bytes());
    body[28..34].copy_from_slice(b"600000");
    body[50..54].copy_from_slice(b"name");
    body[94..98].copy_from_slice(&0x4120_0000_u32.to_le_bytes());
    body[98..102].copy_from_slice(&0xdead_beef_u32.to_le_bytes());
    body
}
fn rules() -> eltdx::Result<LoginReplyRules> {
    LoginReplyRules::new(
        vec![Bytes::from_static(b"synthetic accepted")],
        vec![Bytes::from_static(b"synthetic denied")],
        "synthetic fixture only; not production ACK evidence",
    )
}

#[test]
fn builders_are_lazy_clones_share_close_and_other_owners_remain_independent() -> Case {
    let mac = MacClient::builder().build()?;
    let other = MacClient::builder().build()?;
    let mx = MacExClient::builder().build()?;
    assert!(mac.host()?.is_none());
    assert!(mx.host()?.is_none());
    assert!(mac.healthy_hosts()?.is_empty());
    assert!(mx.healthy_hosts()?.is_empty());
    assert_eq!(mac.config().timeout(), Duration::from_secs(8));
    assert!(mx.config().login_reply_rules().is_none());
    mac.clone().close()?;
    assert_eq!(
        mac.connect(&OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    assert!(other.diagnostics()?.active_endpoint().is_none());
    other.close()?;
    mx.close()?;
    mac.close()?;
    assert!(MacClient::builder()
        .with_endpoints(Vec::new())
        .build()
        .is_err());
    assert!(MacExClient::builder()
        .with_timeout(Duration::ZERO)
        .build()
        .is_err());
    assert!(MacClient::builder()
        .with_cleanup_timeout(Duration::ZERO)
        .build()
        .is_err());
    Ok(())
}

#[test]
fn required_setup_nonempty_quotes_bits_meta_and_close_use_the_public_entry() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream, 0x1c, 0x122b)?;
        assert_eq!(body.len(), 46);
        assert_eq!(&body[20..22], &1_u16.to_le_bytes());
        frame(&mut stream, 0, &quote_body())?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainWire)
        .build()?;
    let fields = FieldSelection::from_ids([FieldId::new(0)?, FieldId::new(47)?]);
    let query = QuotesQuery::new(vec![id()?])?.with_fields(fields)?;
    let rows = client.quotes(&query, &OperationOptions::default())?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id(), &id()?);
    assert_eq!(rows[0].name(), "name");
    assert_eq!(
        rows[0].field(FieldId::new(47)?).unwrap().value,
        models::SlotValue::Unknown([0xef, 0xbe, 0xad, 0xde])
    );
    let raw = rows[0].meta().raw().unwrap();
    assert_eq!(raw.row_ranges(), &[(0, 26, 76)]);
    assert_eq!(raw.wire_frames().unwrap().len(), 1);
    assert_eq!(rows[0].meta().source().endpoint(), Some(endpoint));
    assert!(!rows[0].meta().diagnostics().is_empty());
    client.close()?;
    join(job)
}

#[test]
fn optional_setup_uses_exact_fence_then_keeps_business_response_aligned() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        exact(&mut stream, &hex("0c0218930001030003000d0001")?)?;
        frame(&mut stream, 1, b"required")?;
        exact(&mut stream, &hex("0c0218940001030003000d0002")?)?;
        exact(&mut stream,&hex("0c031899000120002000db0fd5d0c9ccd6a4a8af0000008fc22540130000d500c9ccbdf0d7ea00000002")?)?;
        request(&mut stream, 0x1c, 0x120f)?;
        // Optional step2 is absent. Step3 arrives before the mandatory fence.
        frame(&mut stream, 3, b"optional third")?;
        let mut server = vec![0; 71];
        server[22..26].copy_from_slice(&20_261_117_u32.to_le_bytes());
        server[63..67].copy_from_slice(&20_261_116_u32.to_le_bytes());
        frame(&mut stream, 4, &server)?;
        request(&mut stream, 0x1c, 0x124a)?;
        frame(&mut stream, 0, &[0, 0, 1, 0, 2, 0, 0, 0])?;
        eof(&mut stream)
    })?;
    let identity = |method| ResponseIdentity::new(7_654_321, 0, method);
    let boundary = OptionalSetupBoundary::new(
        [
            SetupReply::Required,
            SetupReply::Optional(identity(2)),
            SetupReply::Optional(identity(3)),
        ],
        identity(4),
        "synthetic ordered-fence fixture only",
    )?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_setup_policy(MacSetupPolicy::OptionalWithFence(boundary))
        .build()?;
    let offsets = client.kline_offsets(&KlineOffsetsQuery::new(), &OperationOptions::default())?;
    assert_eq!(offsets.total(), 256);
    assert_eq!(offsets.returned(), 2);
    client.close()?;
    join(job)
}

#[test]
fn mac_ex_uses_login_then_head_rewrite_without_the_ex_setup_packet() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        let login = request(&mut stream, 1, 0x2454)?;
        assert_eq!(login.len(), 80);
        assert_eq!(
            &login[..8],
            &[0xe5, 0xbb, 0x1c, 0x2f, 0xaf, 0xe5, 0x25, 0x94]
        );
        frame(&mut stream, 0, b"synthetic accepted")?;
        let body = request(&mut stream, 1, 0x122b)?;
        assert_eq!(
            &body[..20],
            &[0xbf, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        );
        frame(&mut stream, 0, &quote_body())?;
        eof(&mut stream)
    })?;
    let client = MacExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_login_reply_rules(rules()?)
        .build()?;
    let id = eltdx::ex::ExInstrumentId::new(
        eltdx::ex::ExMarketId::from_wire(0),
        eltdx::ex::ExCode::new("600000")?,
    );
    let rows = client.quotes(
        &ex_query::QuotesQuery::new(vec![id])?,
        &OperationOptions::default(),
    )?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id().market().get(), 0);
    assert!(rows[0].meta().raw().is_none());
    client.close()?;
    join(job)
}

#[test]
fn negative_login_is_protocol_and_does_not_send_a_business_request() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        request(&mut stream, 1, 0x2454)?;
        frame(&mut stream, 0, b"synthetic denied")?;
        eof(&mut stream)
    })?;
    let client = MacExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_login_reply_rules(rules()?)
        .build()?;
    assert_eq!(
        client
            .connect(&OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    assert!(client.host()?.is_none());
    client.close()?;
    join(job)
}

fn word(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}
fn scalar(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn list_body(count: u16, total: u32, first: u32, bit: u8) -> Vec<u8> {
    let mut body = vec![0; 26];
    body[usize::from(bit / 8)] = 1 << (bit % 8);
    body[20..24].copy_from_slice(&total.to_le_bytes());
    body[24..26].copy_from_slice(&count.to_le_bytes());
    for index in 0..count {
        let mut row = [0; 72];
        row[2..8].copy_from_slice(b"600000");
        row[24..28].copy_from_slice(b"name");
        row[68..72].copy_from_slice(&(first + u32::from(index)).to_le_bytes());
        body.extend_from_slice(&row);
    }
    body
}
fn bar_body(records: &[(u32, u32, u32)]) -> Vec<u8> {
    let mut body = vec![0; 33];
    body[27..29].copy_from_slice(&(records.len() as u16).to_le_bytes());
    for &(date, seconds, first) in records {
        body.extend_from_slice(&date.to_le_bytes());
        body.extend_from_slice(&seconds.to_le_bytes());
        // Distinct bits make swapping amount/volume/float-shares observable.
        for delta in 0..7 {
            body.extend_from_slice(&(first + delta).to_le_bytes());
        }
    }
    body
}
fn mac_trade_body(seconds: &[u32]) -> Vec<u8> {
    let mut body = vec![0; 39];
    body[29..31].copy_from_slice(&(seconds.len() as u16).to_le_bytes());
    for &value in seconds {
        body.extend_from_slice(&value.to_le_bytes());
        body.extend_from_slice(&0x4148_0000_u32.to_le_bytes());
        body.extend_from_slice(&123_u32.to_le_bytes());
        body.extend_from_slice(&7_u32.to_le_bytes());
        body.extend_from_slice(&5_u16.to_le_bytes());
    }
    body
}
fn hk_trade_body(count: u16) -> Vec<u8> {
    let mut body = vec![0; 16];
    body[14..16].copy_from_slice(&count.to_le_bytes());
    for _ in 0..count {
        body.extend_from_slice(&570_u16.to_le_bytes());
        body.extend_from_slice(&12_345_u32.to_le_bytes());
        body.extend_from_slice(&456_u32.to_le_bytes());
        body.extend_from_slice(&(-7_i32).to_le_bytes());
        body.extend_from_slice(&10_005_u16.to_le_bytes());
    }
    body
}
fn ex_id(market: u8) -> eltdx::Result<eltdx::ex::ExInstrumentId> {
    Ok(eltdx::ex::ExInstrumentId::new(
        eltdx::ex::ExMarketId::from_wire(market),
        eltdx::ex::ExCode::new("00700")?,
    ))
}

#[test]
fn quote_list_requests_eighty_then_twenty_and_keeps_duplicate_occurrences() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        for (start, count) in [(0_u32, 80_u16), (80, 20)] {
            let body = request(&mut stream, 0x1c, 0x122c)?;
            assert_eq!(scalar(&body, 0), 6);
            assert_eq!(scalar(&body, 15), start);
            assert_eq!(word(&body, 19), count);
            frame(&mut stream, 0, &list_body(count, 500, start, 93))?;
        }
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainWire)
        .build()?;
    let query = QuoteListQuery::new(Category::A)
        .with_count(100)?
        .with_fields(FieldSelection::from_ids([FieldId::new(93)?]))?;
    let result = client.quote_list(&query, &OperationOptions::default())?;
    assert_eq!(result.rows().len(), 100);
    assert_eq!(result.server_total(), Some(500));
    assert_eq!(result.next_offset(), Some(100));
    assert_eq!(result.completion(), PageCompletion::RequestedCount);
    assert_eq!(result.meta().sources().len(), 2);
    let bit = FieldId::new(93)?;
    assert_eq!(
        result.rows()[0].field(bit).unwrap().name,
        Some("BID2_VOLUME")
    );
    assert_eq!(
        result.rows()[99].field(bit).unwrap().value,
        models::SlotValue::U32(99)
    );
    assert_eq!(result.rows()[0].id(), result.rows()[99].id());
    let raw = result.meta().raw().unwrap();
    assert_eq!(raw.payloads().len(), 2);
    assert_eq!(raw.row_ranges()[79], (0, 26 + 79 * 72, 72));
    assert_eq!(raw.row_ranges()[80], (1, 26, 72));
    assert!(std::ptr::eq(
        result.rows()[0].meta(),
        result.rows()[79].meta()
    ));
    assert!(!std::ptr::eq(
        result.rows()[79].meta(),
        result.rows()[80].meta()
    ));
    client.close()?;
    join(job)
}

#[test]
fn board_category_alias_differs_from_constituent_alias_and_changed_total_is_unknown() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream, 0x1c, 0x122c)?;
        assert_eq!(scalar(&body, 0), 10001);
        frame(&mut stream, 0, &list_body(1, 10, 3, 93))?;
        for (offset, total) in [(0, 20), (1, 21)] {
            let body = request(&mut stream, 0x1c, 0x122c)?;
            assert_eq!(scalar(&body, 0), 20686);
            assert_eq!(scalar(&body, 15), offset);
            frame(&mut stream, 0, &list_body(1, total, 4, 93))?;
        }
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let fields = FieldSelection::from_ids([FieldId::new(93)?]);
    let board = client.quote_list(
        &QuoteListQuery::new(Category::BoardHy)
            .with_count(1)?
            .with_fields(fields.clone())?,
        &OperationOptions::default(),
    )?;
    assert_eq!(
        board.rows()[0].field(FieldId::new(93)?).unwrap().name,
        Some("LIMIT_UP_COUNT")
    );
    let query = BoardMembersQuery::new(BoardSymbol::new("880686")?)
        .with_count(2)?
        .with_fields(fields)?
        .with_pages(PageOptions::new(1, 3, 3)?)?;
    let members = client.board_members(&query, &OperationOptions::default())?;
    assert_eq!(
        members.rows()[0].field(FieldId::new(93)?).unwrap().name,
        Some("BID2_VOLUME")
    );
    assert_eq!(members.server_total(), None);
    assert!(members.meta().diagnostics().iter().any(|item| item
        .message()
        .is_some_and(|text| text.contains("total changed"))));
    client.close()?;
    join(job)
}

#[test]
fn bars_prepend_whole_pages_keep_seconds_and_reorder_only_raw_row_references() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream, 0x1c, 0x122e)?;
        assert_eq!(word(&body, 24), 13);
        assert_eq!(word(&body, 26), 10);
        assert_eq!(scalar(&body, 28), 0);
        assert_eq!(word(&body, 32), 2);
        assert_eq!(&body[36..40], &[1, 1, 0, 1]);
        frame(
            &mut stream,
            0,
            &bar_body(&[(20261117, 86395, 100), (20261117, 86395, 200)]),
        )?;
        let body = request(&mut stream, 0x1c, 0x122e)?;
        assert_eq!(scalar(&body, 28), 2);
        assert_eq!(word(&body, 32), 1);
        frame(&mut stream, 0, &bar_body(&[(20261117, 34205, 300)]))?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainWire)
        .build()?;
    let query = BarsQuery::new(id()?)
        .with_period(MacPeriod::Seconds)?
        .with_times(10)?
        .with_count(3)?
        .with_bar_time(BarTime::End)?
        .with_pages(PageOptions::new(2, 3, 10)?)?;
    let rows = client.bars(&query, &OperationOptions::default())?;
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].source_time().clock(), Some((9, 30, 5)));
    assert_eq!(rows[0].time().clock(), Some((9, 30, 15)));
    assert_eq!(rows[1].time().date(), Some(TradingDate::new(2026, 11, 18)?));
    assert_eq!(rows[1].time().clock(), Some((0, 0, 5)));
    assert_eq!(rows[1].time_raw(), 86395);
    assert_eq!(
        rows.iter()
            .map(|row| row.open().value().bits())
            .collect::<Vec<_>>(),
        [300, 100, 200]
    );
    assert_eq!(rows[0].amount().value().bits(), 304);
    assert_eq!(rows[0].volume().value().bits(), 305);
    assert_eq!(rows[0].float_shares().unwrap().value().bits(), 306);
    assert_eq!(rows[0].record_raw().unwrap().len(), 36);
    let latest = client.diagnostics()?.latest_meta().unwrap().clone();
    let raw = latest.raw().unwrap();
    assert_eq!(raw.row_ranges(), &[(1, 33, 36), (0, 33, 36), (0, 69, 36)]);
    assert_eq!(raw.payloads().len(), 2);
    assert_eq!(raw.wire_frames().unwrap().len(), 2);
    assert_eq!(scalar(&raw.payloads()[0], 33 + 8), 100);
    assert_eq!(scalar(&raw.payloads()[1], 33 + 8), 300);
    client.close()?;
    join(job)
}

#[test]
fn calendar_and_explicit_unknown_periods_do_not_invent_intraday_clocks() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        request(&mut stream, 0x1c, 0x122e)?;
        frame(&mut stream, 0, &bar_body(&[(20261117, 999999, 0)]))?;
        request(&mut stream, 0x1c, 0x122e)?;
        frame(&mut stream, 0, &bar_body(&[(20260230, 999999, 0)]))?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let query = BarsQuery::new(id()?)
        .with_count(1)?
        .with_bar_time(BarTime::End)?;
    let rows = client.bars(&query, &OperationOptions::default())?;
    assert_eq!(rows[0].time().clock(), Some((0, 0, 0)));
    assert_eq!(rows[0].time_raw(), 999999);
    assert_eq!(rows[0].time_source(), BarTimeSource::CalendarMidnight);
    assert!(rows[0].float_shares().is_some());
    assert_eq!(rows[0].float_shares().unwrap().value().bits(), 6);
    let unknown = BarsQuery::new(id()?)
        .with_count(1)?
        .with_period(MacPeriod::Wire(12))?;
    let rows = client.bars(&unknown, &OperationOptions::default())?;
    assert_eq!(rows[0].time().unknown_raw_value(), Some(20260230));
    assert_eq!(rows[0].time_raw(), 999999);
    assert_eq!(rows[0].time_source(), BarTimeSource::UnknownPeriod);
    assert!(rows[0].record_raw().is_none());
    assert!(!rows[0].meta().diagnostics().is_empty());
    client.close()?;
    join(job)
}

#[test]
fn mac_transactions_advance_by_wire_count_and_keep_provided_count_and_nature() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream, 0x1c, 0x122f)?;
        assert_eq!(scalar(&body, 24), 20261116);
        assert_eq!(scalar(&body, 28), 7);
        assert_eq!(word(&body, 32), 2);
        frame(&mut stream, 0, &mac_trade_body(&[34205, 34204]))?;
        let body = request(&mut stream, 0x1c, 0x122f)?;
        assert_eq!(scalar(&body, 28), 9);
        assert_eq!(word(&body, 32), 1);
        frame(&mut stream, 0, &mac_trade_body(&[34203]))?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let query = TransactionsQuery::new(id()?)
        .with_count(3)?
        .with_start(7)?
        .with_date(Some(TradingDate::new(2026, 11, 16)?))?
        .with_pages(PageOptions::new(2, 3, 8)?)?;
    let result = client.transactions(&query, &OperationOptions::default())?;
    assert_eq!(result.next_offset(), Some(10));
    assert_eq!(result.completion(), PageCompletion::RequestedCount);
    assert_eq!(
        result
            .rows()
            .iter()
            .map(|row| row.seconds_raw())
            .collect::<Vec<_>>(),
        [34205, 34204, 34203]
    );
    assert_eq!(result.rows()[0].time().clock(), Some((9, 30, 5)));
    assert_eq!(result.rows()[0].trade_count(), 7);
    assert_eq!(result.rows()[0].count_presence(), CountPresence::Provided);
    assert_eq!(result.rows()[0].side(), TransactionSide::AfterHours);
    assert_eq!(result.rows()[0].source(), TransactionSource::Mac);
    client.close()?;
    join(job)
}

#[test]
fn hk_today_and_history_use_ex_commands_with_native_price_and_absent_trade_count() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        request(&mut stream, 1, 0x2454)?;
        frame(&mut stream, 0, b"synthetic accepted")?;
        let body = request(&mut stream, 1, 0x23fc)?;
        assert_eq!(body.len(), 16);
        assert_eq!(body[0], 27);
        assert_eq!(&body[1..6], b"00700");
        assert_eq!(word(&body, 14), 1);
        frame(&mut stream, 0, &hk_trade_body(1))?;
        let body = request(&mut stream, 1, 0x2406)?;
        assert_eq!(scalar(&body, 0), 20261116);
        assert_eq!(body[4], 27);
        assert_eq!(word(&body, 18), 1);
        frame(&mut stream, 0, &hk_trade_body(1))?;
        eof(&mut stream)
    })?;
    let client = MacExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_login_reply_rules(rules()?)
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let query = ex_query::TransactionsQuery::new(ex_id(27)?)?.with_count(1)?;
    let today = client.transactions(&query, &OperationOptions::default())?;
    let row = &today.rows()[0];
    assert_eq!(row.price().as_f64(), 12.345);
    assert_eq!(
        row.price(),
        models::TransactionPrice::HongKongScaledU32 {
            raw: 12345,
            divisor: 1000
        }
    );
    assert_eq!(row.time().clock(), Some((9, 30, 5)));
    assert_eq!(*row.volume().value(), 456);
    assert_eq!(row.trade_count(), 0);
    assert_eq!(row.count_presence(), CountPresence::NotProvided);
    assert_eq!(row.side(), TransactionSide::Sell);
    assert_eq!(row.direction_raw(), Some(10005));
    assert_eq!(row.open_interest_change(), Some(-7));
    assert_eq!(row.source(), TransactionSource::ExHongKong);
    assert_eq!(row.record_raw().unwrap().len(), 16);
    assert_eq!(row.meta().source().command(), Some(0x23fc));
    let history = client.transactions(
        &query.with_date(Some(TradingDate::new(2026, 11, 16)?))?,
        &OperationOptions::default(),
    )?;
    assert_eq!(history.rows()[0].meta().source().command(), Some(0x2406));
    client.close()?;
    join(job)
}

#[test]
fn hk_futures_stay_on_mac_command_and_full_all_cap_cannot_succeed() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        request(&mut stream, 1, 0x2454)?;
        frame(&mut stream, 0, b"synthetic accepted")?;
        let body = request(&mut stream, 1, 0x122f)?;
        assert_eq!(word(&body, 0), 23);
        frame(&mut stream, 0, &mac_trade_body(&[34205]))?;
        let body = request(&mut stream, 1, 0x23fc)?;
        assert_eq!(body[0], 27);
        assert_eq!(word(&body, 14), 2);
        frame(&mut stream, 0, &hk_trade_body(2))?;
        eof(&mut stream)
    })?;
    let client = MacExClient::builder()
        .with_endpoints(vec![endpoint])
        .with_login_reply_rules(rules()?)
        .build()?;
    let query = ex_query::TransactionsQuery::new(ex_id(23)?)?.with_count(1)?;
    let result = client.transactions(&query, &OperationOptions::default())?;
    assert_eq!(result.rows()[0].source(), TransactionSource::Mac);
    assert_eq!(result.rows()[0].count_presence(), CountPresence::Provided);
    let all =
        ex_query::AllTransactionsQuery::new(ex_id(27)?)?.with_pages(PageOptions::new(2, 1, 2)?)?;
    assert_eq!(
        client
            .all_transactions(&all, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::LimitExceeded
    );
    client.close()?;
    join(job)
}

#[test]
fn unusual_short_reply_remains_unknown_and_does_not_issue_an_implicit_next_page() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream, 0x1c, 0x1237)?;
        assert_eq!(word(&body, 6), 600);
        let mut reply = vec![0; 34];
        reply[..2].copy_from_slice(&1_u16.to_le_bytes());
        reply[4..10].copy_from_slice(b"600000");
        reply[11] = 0xfe;
        reply[31] = 9;
        reply[32..34].copy_from_slice(&3005_u16.to_le_bytes());
        reply.extend_from_slice(b",name,");
        frame(&mut stream, 0, &reply)?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .with_raw(RawPolicy::RetainPayload)
        .build()?;
    let result = client.unusual(
        &UnusualQuery::new(MacMarketId::new(0)),
        &OperationOptions::default(),
    )?;
    assert_eq!(result.rows().len(), 1);
    assert_eq!(result.next_offset(), Some(1));
    assert_eq!(result.server_total(), None);
    assert_eq!(result.completion(), PageCompletion::Unknown);
    assert_eq!(result.rows()[0].name(), "name");
    assert_eq!(result.rows()[0].time().clock(), Some((9, 30, 5)));
    assert_eq!(
        result.rows()[0].meta().raw().unwrap().row_ranges(),
        &[(0, 2, 32)]
    );
    client.close()?;
    join(job)
}

#[test]
fn malformed_page_retires_socket_and_pre_cancelled_calls_stay_lazy() -> Case {
    let token = CancellationToken::default();
    token.cancel();
    let idle = MacClient::builder().build()?;
    assert_eq!(
        idle.transactions(
            &TransactionsQuery::new(id()?),
            &OperationOptions::default().with_cancellation(token)
        )
        .unwrap_err()
        .kind(),
        ErrorKind::Cancelled
    );
    assert!(idle.host()?.is_none());
    idle.close()?;
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        request(&mut stream, 0x1c, 0x122f)?;
        // Complete binary layout with an impossible clock must fail decoding
        // before publication and must not be retried on this or another peer.
        frame(&mut stream, 0, &mac_trade_body(&[86400]))?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    assert_eq!(
        client
            .transactions(
                &TransactionsQuery::new(id()?).with_count(1)?,
                &OperationOptions::default()
            )
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    assert!(client.host()?.is_none());
    client.close()?;
    join(job)
}

#[test]
fn boards_require_empty_followup_after_a_short_page_and_keep_leader_identity() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream, 0x1c, 0x1231)?;
        assert_eq!(word(&body, 0), 2);
        assert_eq!(word(&body, 2), 255);
        assert_eq!(word(&body, 6), 0);
        let mut reply = vec![0; 164];
        reply[..2].copy_from_slice(&2_u16.to_le_bytes());
        reply[2..4].copy_from_slice(&999_u16.to_le_bytes());
        reply[6..12].copy_from_slice(b"880686");
        reply[86..92].copy_from_slice(b"600000");
        frame(&mut stream, 0, &reply)?;
        let body = request(&mut stream, 0x1c, 0x1231)?;
        assert_eq!(word(&body, 0), 1);
        assert_eq!(word(&body, 6), 1);
        frame(&mut stream, 0, &[0, 0, 0xe7, 3])?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let query = BoardsQuery::new()
        .with_count(2)?
        .with_pages(PageOptions::new(2, 4, 8)?)?;
    let rows = client.boards(&query, &OperationOptions::default())?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].board().id().code().as_str(), "880686");
    assert_eq!(rows[0].leader().id().code().as_str(), "600000");
    assert_eq!(rows[0].meta().source().endpoint(), Some(endpoint));
    assert_eq!(
        client.diagnostics()?.latest_meta().unwrap().sources().len(),
        2
    );
    client.close()?;
    join(job)
}

fn partial_quote_frame(stream: &mut TcpStream) -> Case {
    let body = quote_body();
    let mut prefix = 7_654_321_u32.to_le_bytes().to_vec();
    prefix.extend_from_slice(&[0; 8]);
    prefix.extend_from_slice(&(body.len() as u16).to_le_bytes());
    prefix.extend_from_slice(&(body.len() as u16).to_le_bytes());
    prefix.extend_from_slice(&body[..2]);
    stream.write_all(&prefix)?;
    Ok(())
}

#[test]
fn cancelling_a_partial_frame_retires_it_before_a_fresh_setup_and_reply() -> Case {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let endpoint = listener.local_addr()?;
    let (sent, ready) = std::sync::mpsc::channel();
    let job = thread::spawn(move || -> Case {
        for first in [true, false] {
            let (mut stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(3)))?;
            required_setup(&mut stream)?;
            request(&mut stream, 0x1c, 0x122b)?;
            if first {
                partial_quote_frame(&mut stream)?;
                sent.send(())?;
            } else {
                frame(&mut stream, 0, &quote_body())?;
            }
            eof(&mut stream)?;
        }
        Ok(())
    });
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let caller = client.clone();
    let token = CancellationToken::default();
    let call_token = token.clone();
    let query = QuotesQuery::new(vec![id()?])?;
    let call_query = query.clone();
    let first = thread::spawn(move || {
        caller.quotes(
            &call_query,
            &OperationOptions::default().with_cancellation(call_token),
        )
    });
    ready.recv_timeout(Duration::from_secs(2))?;
    token.cancel();
    assert_eq!(
        first
            .join()
            .map_err(|_| std::io::Error::other("MAC caller panicked"))?
            .unwrap_err()
            .kind(),
        ErrorKind::Cancelled
    );
    assert!(client.host()?.is_none());
    assert_eq!(
        client.quotes(&query, &OperationOptions::default())?.len(),
        1
    );
    client.close()?;
    join(job)
}

#[test]
fn close_during_a_partial_frame_is_shared_and_irreversible() -> Case {
    let (sent, ready) = std::sync::mpsc::channel();
    let (endpoint, job) = peer(move |mut stream| {
        required_setup(&mut stream)?;
        request(&mut stream, 0x1c, 0x122b)?;
        partial_quote_frame(&mut stream)?;
        sent.send(())?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let caller = client.clone();
    let query = QuotesQuery::new(vec![id()?])?;
    let call = thread::spawn(move || caller.quotes(&query, &OperationOptions::default()));
    ready.recv_timeout(Duration::from_secs(2))?;
    client.close()?;
    assert_eq!(
        call.join()
            .map_err(|_| std::io::Error::other("MAC caller panicked"))?
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    assert_eq!(
        client
            .connect(&OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Closed
    );
    join(job)
}

#[test]
fn partial_transport_failure_repeats_setup_on_the_next_endpoint_without_row_leakage() -> Case {
    let (broken, broken_job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        request(&mut stream, 0x1c, 0x122b)?;
        partial_quote_frame(&mut stream)
    })?;
    let (healthy, healthy_job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        request(&mut stream, 0x1c, 0x122b)?;
        frame(&mut stream, 0, &quote_body())?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![broken, healthy])
        .with_auto_failover(true)
        .with_wire_attempt_limit(std::num::NonZeroU32::new(2).unwrap())
        .build()?;
    let rows = client.quotes(
        &QuotesQuery::new(vec![id()?])?,
        &OperationOptions::default(),
    )?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].meta().source().endpoint(), Some(healthy));
    assert_eq!(client.host()?, Some(healthy));
    client.close()?;
    join(broken_job)?;
    join(healthy_job)
}

#[test]
fn complete_over_response_is_protocol_and_cannot_publish_a_reusable_socket() -> Case {
    let (endpoint, job) = peer(|mut stream| {
        required_setup(&mut stream)?;
        let body = request(&mut stream, 0x1c, 0x122c)?;
        assert_eq!(word(&body, 19), 1);
        frame(&mut stream, 0, &list_body(2, 2, 0, 93))?;
        eof(&mut stream)
    })?;
    let client = MacClient::builder()
        .with_endpoints(vec![endpoint])
        .build()?;
    let query = QuoteListQuery::new(Category::A).with_count(1)?;
    assert_eq!(
        client
            .quote_list(&query, &OperationOptions::default())
            .unwrap_err()
            .kind(),
        ErrorKind::Protocol
    );
    assert!(client.host()?.is_none());
    client.close()?;
    join(job)
}
