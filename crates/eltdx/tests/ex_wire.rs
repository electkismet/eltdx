//! Authored T24 Stage A cases. Deliberately not executed or collected until the
//! controller permits validation after T01–T30 implementation is complete.
#![cfg(feature = "ex")]
use bytes::Bytes;
use eltdx::{ErrorKind, RawPolicy, TradingDate};
use eltdx::ex::{commands::*, config::*, frame::*, hosts::*, *};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn shared_quote_schemas_keep_source_offsets_types_and_the_legacy_futures_mismatch() {
    use eltdx::ex::layout::{QUOTE_FIELDS,FUTURES_FIELDS,HONG_KONG_FIELDS};
    assert_eq!(QUOTE_FIELDS.iter().find(|&&(name,_,_)| name == "chicang"),Some(&("chicang",13,false)));
    assert_eq!(QUOTE_FIELDS.iter().find(|&&(name,_,_)| name == "bid5"),Some(&("bid5",18,true)));
    assert_eq!(QUOTE_FIELDS.iter().find(|&&(name,_,_)| name == "ask_vol5"),Some(&("ask_vol5",33,false)));
    assert_eq!(FUTURES_FIELDS.last(),Some(&("MaiChuLiang",30,false)));
    assert_eq!(HONG_KONG_FIELDS.last(),Some(&("MaiChuLiang5",34,false)));
    assert_eq!(QUOTE_FIELDS.len(),31); assert_eq!(FUTURES_FIELDS.len(),17); assert_eq!(HONG_KONG_FIELDS.len(),32);
    for schema in [QUOTE_FIELDS,FUTURES_FIELDS,HONG_KONG_FIELDS] {
        for (index,&(name,slot,_)) in schema.iter().enumerate() {
            assert!(slot<35);
            assert!(!schema[..index].iter().any(|&(old_name,old_slot,_)| old_name == name || old_slot == slot));
        }
    }
}

fn id() -> Result<ExInstrumentId, eltdx::Error> { Ok(ExInstrumentId::new(ExMarketId::new(31)?, ExCode::new("00700")?)) }
fn date() -> Result<TradingDate, eltdx::Error> { TradingDate::new(2024,1,2) }
fn hex(text: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    text.split_whitespace().map(|byte| u8::from_str_radix(byte,16).map_err(Into::into)).collect()
}
fn response(header: usize, count_offset: usize, width: usize, count: u16) -> Vec<u8> {
    let mut bytes = vec![0; header + width * usize::from(count)];
    bytes[count_offset..count_offset+2].copy_from_slice(&count.to_le_bytes()); bytes
}
fn put_u32(bytes: &mut [u8], offset: usize, value: u32) { bytes[offset..offset+4].copy_from_slice(&value.to_le_bytes()); }
fn plain_frame(body: &[u8], sequence: u32, method: u32) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let len = u16::try_from(body.len())?;
    let mut wire = FrameHeader::new(ENCODED_MAGIC,sequence,method,len,len).encode().to_vec();
    wire.extend_from_slice(body); Ok(wire)
}

#[test]
fn eleven_request_goldens_and_native_defaults() -> TestResult {
    assert_eq!(markets_request().wire().as_ref(), hex("01 02 48 69 00 01 02 00 02 00 f4 23")?);
    assert_eq!(instrument_count_request().wire().as_ref(), hex("01 03 48 66 00 01 02 00 02 00 f0 23")?);
    assert_eq!(instruments_request(&InstrumentsQuery::new(0x12345678)).wire().as_ref(), hex("01 04 48 67 00 01 08 00 08 00 f5 23 78 56 34 12 64 00")?);
    assert_eq!(quote_request(&QuoteQuery::new(id()?)).wire().as_ref(), hex("01 01 08 02 02 01 0c 00 0c 00 fa 23 1f 30 30 37 30 30 00 00 00 00")?);
    let ql = QuoteListQuery::new(ExMarketId::new(31)?,QuoteCategory::HongKong);
    assert_eq!(quote_list_request(&ql).wire().as_ref(),hex("01 c1 06 0b 00 02 0b 00 0b 00 00 24 1f 00 00 00 00 50 00 01 00")?);
    assert_eq!(quote_list_request(&ql).wire(),quote_list_request(&ql.clone().with_category(QuoteCategory::Futures)).wire());
    assert_eq!(bars_request(&BarsQuery::new(id()?,ExPeriod::Minute5)).wire().as_ref(),hex("01 01 08 6a 01 01 16 00 16 00 ff 23 1f 30 30 37 30 30 00 00 00 00 00 00 01 00 00 00 00 00 bc 02")?);
    let sequence = RangeSequence::default(); let lease = sequence.acquire()?;
    assert_eq!(bars_range_request(&BarsRangeQuery::new(id()?,date()?,date()?)?,&lease).wire().as_ref(),hex("01 01 38 92 00 01 16 00 16 00 0d 24 1f 30 30 37 30 30 00 00 00 00 07 00 e6 d6 34 01 e6 d6 34 01")?);
    assert_eq!(minutes_request(&MinutesQuery::new(id()?)).wire().as_ref(),hex("01 07 08 00 01 01 0c 00 0c 00 0b 24 1f 30 30 37 30 30 00 00 00 00")?);
    assert_eq!(history_minutes_request(&HistoryMinutesQuery::new(id()?,date()?)?).wire().as_ref(),hex("01 01 30 00 01 01 10 00 10 00 0c 24 e6 d6 34 01 1f 30 30 37 30 30 00 00 00 00")?);
    assert_eq!(transactions_request(&TransactionsQuery::new(id()?)).wire().as_ref(),hex("01 01 08 00 03 01 12 00 12 00 fc 23 1f 30 30 37 30 30 00 00 00 00 00 00 00 00 08 07")?);
    assert_eq!(history_transactions_request(&HistoryTransactionsQuery::new(id()?,date()?)?).wire().as_ref(),hex("01 01 30 00 02 01 16 00 16 00 06 24 e6 d6 34 01 1f 30 30 37 30 30 00 00 00 00 00 00 00 00 08 07")?);
    Ok(())
}
#[test]
fn request_boundaries_are_checked_without_truncation() -> TestResult {
    assert!(ExCode::new("").is_err()); assert!(ExCode::new("1234567890").is_err()); assert!(ExCode::new("a\0b").is_err());
    assert_eq!(ExCode::new("中国中")?.wire_bytes(),[0xe4,0xb8,0xad,0xe5,0x9b,0xbd,0xe4,0xb8,0xad]);
    assert!(ExCode::new("中国中国").is_err()); assert!(ExMarketId::new(256).is_err());
    assert_eq!(ExMarketId::new(255)?.get(),255); assert!(QuoteCategory::new(1).is_err());
    assert!(InstrumentsQuery::new(0).with_count(0).is_err()); assert!(InstrumentsQuery::new(0).with_count(65536).is_err());
    assert_eq!(InstrumentsQuery::new(u32::MAX).with_count(65535)?.count().get(),65535);
    assert!(QuoteListQuery::new(ExMarketId::new(255)?,QuoteCategory::Futures).with_start(65536).is_err());
    assert!(BarsQuery::new(id()?,ExPeriod::Minute5).with_count(0).is_err());
    assert!(TransactionsQuery::new(id()?).with_count(65536).is_err());
    assert!(TransactionsQuery::new(id()?).with_start(0x80000000).is_err());
    assert_eq!(&transactions_request(&TransactionsQuery::new(id()?).with_start(i32::MAX as u32)?).wire()[22..26],&[255,255,255,127]);
    assert!(HistoryTransactionsQuery::new(id()?,date()?)?.with_start(0x80000000).is_err());
    assert!(TradingDate::new(2023,2,29).is_err());
    assert!(BarsRangeQuery::new(id()?,TradingDate::new(2024,1,3)?,date()?).is_err());
    assert!(BarsRangeQuery::new(id()?,date()?,date()?)?.with_bar_time(BarTime::End).is_err());
    assert!(ExPeriod::known(12).is_err()); assert_eq!(ExPeriod::Wire(u16::MAX).wire(),u16::MAX);
    assert!(BarsQuery::new(id()?,ExPeriod::Wire(12)).with_bar_time(BarTime::End).is_err());
    Ok(())
}
#[test]
fn setup_and_fragmented_frames_never_consume_the_next_frame() -> TestResult {
    let expected = hex(concat!("01 01 48 65 00 01 52 00 52 00 54 24 ",
        "1f 32 c6 e5 d5 3d fb 41 1f 32 c6 e5 d5 3d fb 41 1f 32 c6 e5 d5 3d fb 41 1f 32 c6 e5 d5 3d fb 41 ",
        "1f 32 c6 e5 d5 3d fb 41 1f 32 c6 e5 d5 3d fb 41 1f 32 c6 e5 d5 3d fb 41 1f 32 c6 e5 d5 3d fb 41 ",
        "cc e1 6d ff d5 ba 3f b8 cb c5 7a 05 4f 77 48 ea"))?;
    assert_eq!(SETUP_REQUEST.as_slice(),expected);
    let first = plain_frame(&[1,2,3],7,0x2454)?; let second = plain_frame(&[4,5],8,0x23f4)?;
    for split in 0..first.len() {
        let mut accumulator = FrameAccumulator::new(FrameLimits::default(),Association::new(Some(ENCODED_MAGIC),Some(7),Some(0x2454)));
        assert_eq!(accumulator.push(&first[..split])?,split);
        let mut joined = first[split..].to_vec(); joined.extend_from_slice(&second);
        assert_eq!(accumulator.push(&joined)?,first.len()-split);
        assert_eq!(accumulator.remaining(),0);
        let frame = accumulator.finish()?; assert_eq!(frame.payload().as_ref(),&[1,2,3]); assert!(frame.association_verified());
    }
    let mut truncated = FrameAccumulator::new(FrameLimits::default(),Association::default());
    truncated.push(&first[..first.len()-1])?; assert_eq!(truncated.finish().err().map(|e| e.kind()),Some(ErrorKind::Protocol));
    for association in [Association::new(Some(0),None,None),Association::new(None,Some(8),None),Association::new(None,None,Some(0))] {
        assert_eq!(Frame::decode(Bytes::from(first.clone()),FrameLimits::default(),association).err().map(|e| e.kind()),Some(ErrorKind::Protocol));
    }
    assert!(!Frame::decode(Bytes::from(first),FrameLimits::default(),Association::default())?.association_verified());
    Ok(())
}
#[test]
fn compressed_frames_require_exact_end_and_bounded_output() -> TestResult {
    // Independently specified zlib stream for ASCII "hello" (Adler32 062c0215).
    let compressed = hex("78 9c cb 48 cd c9 c9 07 00 06 2c 02 15")?;
    let header = FrameHeader::new(ENCODED_MAGIC,1,0x23f4,13,5);
    assert_eq!(decode_body(header,Bytes::from(compressed.clone()),FrameLimits::default())?.as_ref(),b"hello");
    for decoded in [0,4,6,65535] {
        assert!(decode_body(FrameHeader::new(ENCODED_MAGIC,1,0x23f4,13,decoded),Bytes::from(compressed.clone()),FrameLimits::default()).is_err());
    }
    let mut trailing = compressed.clone(); trailing.push(0);
    assert!(decode_body(FrameHeader::new(0,0,0,14,5),Bytes::from(trailing),FrameLimits::default()).is_err());
    assert!(decode_body(header,Bytes::from(compressed[..12].to_vec()),FrameLimits::default()).is_err());
    assert!(decode_body(header,Bytes::from(vec![0;13]),FrameLimits::default()).is_err());
    assert_eq!(decode_body(header,Bytes::from(compressed),FrameLimits::new(12,5)?).err().map(|e|e.kind()),Some(ErrorKind::LimitExceeded));
    assert!(FrameLimits::new(65536,1).is_err()); assert!(FrameLimits::new(1,0).is_err());
    let mut accumulator = FrameAccumulator::new(FrameLimits::new(1,1)?,Association::default());
    assert!(accumulator.push(&header.encode()).is_err()); assert!(accumulator.push(&[]).is_err());
    Ok(())
}
#[test]
fn sequence_wrap_and_independent_session_leases() -> TestResult {
    let first = RangeSequence::default(); let second = RangeSequence::default();
    for expected in 1..=255 {
        let lease = first.acquire()?; assert_eq!(lease.sequence(),expected);
        assert!(first.acquire().is_err());
        if expected == 1 { assert_eq!(second.acquire()?.sequence(),1); }
    }
    assert_eq!(first.acquire()?.sequence(),1); assert_eq!(second.acquire()?.sequence(),2);
    Ok(())
}
#[test]
fn markets_and_instruments_preserve_unknown_values_gbk_and_all_bytes() -> TestResult {
    let mut markets = response(2,0,64,2);
    markets[2]=9; markets[35]=255; markets[3..5].copy_from_slice(&[0xd6,0xd0]); markets[38..66].fill(0xaa);
    let decoded = decode_markets(markets.into())?;
    assert_eq!(decoded.value().len(),1); assert_eq!(decoded.value()[0].market().get(),255);
    assert_eq!(decoded.value()[0].name(),"中"); assert_eq!(decoded.value()[0].record_raw()[63],0xaa);
    assert_eq!(decoded.row_ranges(),&[(2,66)]);
    let mut instruments = response(6,4,64,1);
    instruments[6]=252; instruments[7]=255; instruments[11]=b'A';
    instruments[20..22].copy_from_slice(&[0xd6,0xd0]); instruments[46..70].fill(0xbb);
    let decoded = decode_instruments(instruments.into())?;
    assert_eq!(decoded.value()[0].id().code(),"A"); assert_eq!(decoded.value()[0].name(),"中");
    assert_eq!(decoded.value()[0].category(),252); assert_eq!(decoded.value()[0].record_raw()[63],0xbb);
    let mut bad = response(6,4,64,1); bad[20]=0xff;
    assert!(!decode_instruments(bad.into())?.diagnostics().is_empty());
    let mut count = vec![0;23]; put_u32(&mut count,19,u32::MAX);
    assert_eq!(*decode_instrument_count(count.into())?.value(),u32::MAX);
    Ok(())
}
#[test]
fn quote_keeps_native_bits_five_levels_and_unknown_slots_under_omit() -> TestResult {
    let mut raw = vec![0;150]; raw[0]=31; raw[1..6].copy_from_slice(b"00700");
    raw[10..14].fill(0xaa);
    for slot in 0..34 { put_u32(&mut raw,14+slot*4,slot as u32); }
    put_u32(&mut raw,14,0x80000000); put_u32(&mut raw,18,0x7fc01234);
    let decoded = decode_quote(raw.clone().into(),RawPolicy::Omit)?;
    let quote = decoded.value().as_ref().ok_or("missing quote")?;
    assert_eq!(quote.pre_close().value().bits(),0x80000000); assert_eq!(quote.open().value().bits(),0x7fc01234);
    assert_eq!(*quote.kaicang().value(),5); assert_eq!(*quote.chicang().value(),13);
    assert_eq!(quote.bid()[4].price().value().bits(),18); assert_eq!(*quote.ask()[4].volume().value(),33);
    assert_eq!(quote.opaque()[0].as_ref(),&[0xaa;4]); assert!(quote.record_raw().is_none());
    assert_eq!(quote.pre_close().meaning().unit(),&NativeUnit::Unknown);
    let retained = decode_quote(raw.clone().into(),RawPolicy::RetainPayload)?;
    assert_eq!(retained.value().as_ref().ok_or("missing quote")?.record_raw().ok_or("missing raw")?.as_ref(),raw);
    assert!(decode_quote(Bytes::new(),RawPolicy::Omit).is_err());
    Ok(())
}
#[test]
fn quote_list_slot_layout_categories_and_150_byte_tail() -> TestResult {
    let mut raw = response(2,0,300,1); raw[2]=255; raw[3]=b'X';
    for slot in 0..35 { put_u32(&mut raw,12+slot*4,slot as u32); }
    raw[152..302].fill(0xaa);
    let future = decode_quote_list(raw.clone().into(),QuoteCategory::Futures)?;
    let row = &future.value()[0]; assert_eq!(row.slot_raw().len(),300); assert_eq!(row.unused_tail(),&[0xaa;150]);
    assert_eq!(row.field("ChiCangLiang").ok_or("field")?.value(),NativeValue::Unsigned(14));
    assert_eq!(row.field("ZongJinE").ok_or("field")?.value(),NativeValue::Float(WireFloat::from_bits(10)));
    assert!(row.futures().is_some()); assert!(row.hong_kong().is_none());
    let hk = decode_quote_list(raw.into(),QuoteCategory::HongKong)?;
    assert_eq!(hk.value()[0].fields().len(),32);
    assert_eq!(hk.value()[0].field("MaiRuJia").ok_or("field")?.slot(),7);
    assert_eq!(hk.value()[0].field("MaiChuLiang5").ok_or("field")?.value(),NativeValue::Unsigned(34));
    Ok(())
}
fn bar_payload(header: usize,count_offset: usize) -> Vec<u8> {
    let mut bytes = response(header,count_offset,32,1);
    bytes[header..header+4].copy_from_slice(&[102,0,58,2]); // 2004-01-02 09:30
    put_u32(&mut bytes,header+4,0x80000000); put_u32(&mut bytes,header+8,0x7fc01234);
    put_u32(&mut bytes,header+20,0x4996b43c); put_u32(&mut bytes,header+24,u32::MAX);
    put_u32(&mut bytes,header+28,0x3fc00000); bytes
}
#[test]
fn bars_preserve_dual_slot_range_settlement_and_time_sources() -> TestResult {
    let query = BarsQuery::new(id()?,ExPeriod::Minute5).with_bar_time(BarTime::End)?;
    let decoded = decode_bars(bar_payload(20,18).into(),&query,RawPolicy::Omit)?;
    let row = &decoded.value()[0]; assert_eq!(row.source_time().clock(),Some((9,30,0))); assert_eq!(row.time().clock(),Some((9,35,0)));
    assert_eq!(row.position_bits(),0x4996b43c); assert_eq!(row.amount_view().ok_or("amount view")?.value().bits(),row.position_bits());
    assert_eq!(row.open().value().bits(),0x80000000); assert_eq!(row.high().value().bits(),0x7fc01234);
    assert!(row.settlement().is_none()); assert_eq!(row.last_slot().bits(),0x3fc00000);
    let range = decode_bars_range(bar_payload(14,12).into(),RawPolicy::RetainPayload)?;
    assert_eq!(range.value()[0].settlement().ok_or("settlement")?.value().bits(),0x3fc00000);
    assert_eq!(range.value()[0].record_raw().ok_or("raw")?.len(),32);
    assert!(range.value()[0].amount_view().is_none());
    let mut daily = bar_payload(20,18); put_u32(&mut daily,20,20240102);
    let decoded = decode_bars(daily.into(),&BarsQuery::new(id()?,ExPeriod::Day).with_bar_time(BarTime::End)?,RawPolicy::Omit)?;
    assert_eq!(decoded.value()[0].time().clock(),Some((15,0,0))); assert_eq!(decoded.value()[0].time_source(),BarTimeSource::SyntheticClose);
    assert!(eltdx::ex::time::packed_date(0).is_err()); assert!(eltdx::ex::time::day_date(20230229).is_err());
    assert!(eltdx::ex::time::time_of_day(1440,0).is_err());
    Ok(())
}
#[test]
fn minutes_have_no_fabricated_date_transactions_keep_seconds_and_signed_delta() -> TestResult {
    for (header,offset,historical) in [(12,10,false),(20,18,true)] {
        let mut raw = response(header,offset,18,1); raw[header..header+2].copy_from_slice(&570_u16.to_le_bytes());
        put_u32(&mut raw,header+2,0x80000000); put_u32(&mut raw,header+6,0x7fc01234);
        put_u32(&mut raw,header+10,u32::MAX); put_u32(&mut raw,header+14,987);
        let decoded = if historical { decode_history_minutes(raw.into(),RawPolicy::Omit)? } else { decode_minutes(raw.into(),RawPolicy::Omit)? };
        assert_eq!(decoded.value()[0].time().date(),None); assert_eq!(decoded.value()[0].time().clock(),Some((9,30,0)));
        assert_eq!(decoded.value()[0].avg_price().value().bits(),0x7fc01234);
    }
    let mut raw = response(16,14,16,1); raw[16..18].copy_from_slice(&601_u16.to_le_bytes());
    put_u32(&mut raw,18,12345); put_u32(&mut raw,22,u32::MAX); put_u32(&mut raw,26,(-3_i32) as u32);
    raw[30..32].copy_from_slice(&60059_u16.to_le_bytes());
    for historical in [false,true] {
        let decoded = if historical { decode_history_transactions(raw.clone().into(),RawPolicy::Omit)? } else { decode_transactions(raw.clone().into(),RawPolicy::Omit)? };
        let row = &decoded.value()[0]; assert_eq!(row.time().clock(),Some((10,1,59))); assert_eq!(row.nature(),6);
        assert_eq!(*row.price_raw().value(),12345); assert_eq!(*row.zengcang().value(),-3); assert_eq!(row.direction_raw(),60059);
    }
    raw[30..32].copy_from_slice(&10060_u16.to_le_bytes()); assert!(decode_transactions(raw.into(),RawPolicy::Omit).is_err());
    Ok(())
}
#[test]
fn complete_empty_responses_and_truncated_declared_rows() -> TestResult {
    assert!(decode_markets(Bytes::from(vec![0,0]))?.value().is_empty());
    assert!(decode_instruments(Bytes::from(vec![0;6]))?.value().is_empty());
    assert!(decode_quote_list(Bytes::from(vec![0,0]),QuoteCategory::Futures)?.value().is_empty());
    assert!(decode_bars(Bytes::from(vec![0;20]),&BarsQuery::new(id()?,ExPeriod::Day),RawPolicy::Omit)?.value().is_empty());
    assert!(decode_bars_range(Bytes::from(vec![0;14]),RawPolicy::Omit)?.value().is_empty());
    assert!(decode_minutes(Bytes::from(vec![0;12]),RawPolicy::Omit)?.value().is_empty());
    assert!(decode_history_minutes(Bytes::from(vec![0;20]),RawPolicy::Omit)?.value().is_empty());
    assert!(decode_transactions(Bytes::from(vec![0;16]),RawPolicy::Omit)?.value().is_empty());
    assert!(decode_history_transactions(Bytes::from(vec![0;16]),RawPolicy::Omit)?.value().is_empty());
    let truncated = |h,c,w| { let mut data = response(h,c,w,1); data.pop(); Bytes::from(data) };
    assert!(decode_markets(truncated(2,0,64)).is_err()); assert!(decode_instruments(truncated(6,4,64)).is_err());
    assert!(decode_quote_list(truncated(2,0,300),QuoteCategory::HongKong).is_err());
    assert!(decode_bars(truncated(20,18,32),&BarsQuery::new(id()?,ExPeriod::Day),RawPolicy::Omit).is_err());
    assert!(decode_bars_range(truncated(14,12,32),RawPolicy::Omit).is_err());
    assert!(decode_minutes(truncated(12,10,18),RawPolicy::Omit).is_err());
    assert!(decode_history_minutes(truncated(20,18,18),RawPolicy::Omit).is_err());
    assert!(decode_transactions(truncated(16,14,16),RawPolicy::Omit).is_err());
    assert!(decode_history_transactions(truncated(16,14,16),RawPolicy::Omit).is_err());
    assert!(decode_instrument_count(Bytes::from(vec![0;22])).is_err()); assert!(decode_quote(Bytes::from(vec![0;149]),RawPolicy::Omit).is_err());
    Ok(())
}
#[test]
fn explicit_host_configuration_is_pure_and_does_not_silently_fallback() -> TestResult {
    let config = ExConfig::default(); assert_eq!(config.timeout(),std::time::Duration::from_secs(8)); assert_eq!(config.raw(),RawPolicy::Omit);
    assert_eq!(config.endpoints()[0].port(),7727); assert!(config.auto_failover()); assert_eq!(config.endpoints().len(),16);
    assert_eq!(ProbeOptions::default().timeout(),std::time::Duration::from_secs(5)); assert_eq!(DiscoveryOptions::default().max_workers().get(),8);
    assert!(parse_hosts(["127.0.0.1:7727"],7727).is_err()); assert!(parse_hosts(["bad-host"],7727).is_err());
    assert_eq!(parse_hosts(["127.0.0.1","127.0.0.1"],7727)?.len(),1); assert_eq!(parse_hosts(["[::1]"],7727)?.len(),1);
    let snapshot = HostConfigSource::default().with_environment(EnvironmentSource::Value(Some("127.0.0.1; 127.0.0.2".into())));
    assert_eq!(load_hosts(&snapshot)?.len(),2);
    let explicit = snapshot.with_explicit(parse_hosts(["127.0.0.3"],7727)?)?.with_environment(EnvironmentSource::Value(Some("invalid".into())));
    assert_eq!(load_hosts(&explicit)?,parse_hosts(["127.0.0.3"],7727)?);
    assert!(load_hosts(&HostConfigSource::default().with_environment(EnvironmentSource::Value(Some("".into())))).is_err());
    assert!(parse_hosts_json("{\"hosts\":[]}",7727).is_err()); assert!(parse_hosts_json("{\"hosts\":[7]}",7727).is_err());
    assert!(parse_hosts_json("[]",7727).is_err()); assert!(parse_hosts_json("not json",7727).is_err());
    assert_eq!(parse_hosts_json("{\"schema_version\":1,\"hosts\":[\"127.0.0.1\"]}",7727)?.len(),1);
    assert_eq!(server_file_path("data"),std::path::Path::new("data").join("ex_servers.json"));
    Ok(())
}

#[test]
fn raw_projection_shares_payload_and_enforces_caller_allowance() -> TestResult {
    let payload = Bytes::from(vec![1,2,3,4]); let ptr = payload.as_ptr();
    assert!(project_raw(payload.clone(),None,&[(0,4)],RawPolicy::Omit,0)?.is_none());
    let retained = project_raw(payload.clone(),None,&[(1,3)],RawPolicy::RetainPayload,4)?.ok_or("raw")?;
    assert_eq!(retained.payloads()[0].as_ptr(),ptr); assert_eq!(retained.row_ranges(),&[(0,1,2)]);
    let (page,offset,length) = retained.row_ranges()[0];
    assert_eq!(&retained.payloads()[page][offset..offset+length],&[2,3]);
    assert!(project_raw(payload.clone(),None,&[(0,5)],RawPolicy::RetainPayload,100).is_err());
    assert!(project_raw(payload.clone(),None,&[(0,4)],RawPolicy::RetainWire,100).is_err());
    assert_eq!(project_raw(payload.clone(),None,&[(0,4)],RawPolicy::RetainPayload,3).err().map(|e|e.kind()),Some(ErrorKind::LimitExceeded));
    let wire = Bytes::from(plain_frame(&payload,1,1)?);
    assert!(project_raw(payload.clone(),Some(wire.clone()),&[(0,4)],RawPolicy::RetainWire,23).is_err());
    let all = project_raw(payload,Some(wire),&[(0,4)],RawPolicy::RetainWire,24)?.ok_or("raw")?;
    assert_eq!(all.wire_frames().ok_or("wire")?[0].len(),20);
    Ok(())
}

#[test]
fn explicit_file_config_roundtrip_and_invalid_source_precedence() -> TestResult {
    // This test is source only until validation is authorized. No filesystem
    // side effects occur during authoring or test discovery.
    let path = std::env::temp_dir().join(format!("eltdx-ex-{}-{}.json",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos()));
    let endpoints = parse_hosts(["127.0.0.1","[::1]"],7727)?;
    save_hosts(&path,&endpoints)?;
    let source = HostConfigSource::default().with_file(&path);
    assert_eq!(load_hosts(&source)?,endpoints);
    save_hosts(&path,&parse_hosts(["127.0.0.2"],7727)?)?;
    assert_eq!(load_hosts(&source)?,parse_hosts(["127.0.0.2"],7727)?);
    std::fs::write(&path,b"{invalid")?;
    assert!(load_hosts(&source).is_err());
    assert_eq!(load_hosts(&source.clone().with_environment(EnvironmentSource::Value(Some("127.0.0.3".into()))))?,parse_hosts(["127.0.0.3"],7727)?);
    assert!(save_hosts(&path,&parse_hosts(["127.0.0.1"],7000)?).is_err());
    std::fs::remove_file(&path)?;
    assert_eq!(load_hosts(&source)?.len(),16);
    Ok(())
}

#[test]
fn nondefault_queries_encode_exact_wire_widths_and_periods() -> TestResult {
    let max = ExInstrumentId::new(ExMarketId::new(255)?,ExCode::new("123456789")?);
    let instruments = instruments_request(&InstrumentsQuery::new(u32::MAX).with_count(65535)?);
    assert_eq!(&instruments.wire()[12..],&[255;6]);
    let list = quote_list_request(&QuoteListQuery::new(ExMarketId::new(255)?,QuoteCategory::Futures).with_start(65535)?.with_count(65535)?);
    assert_eq!(&list.wire()[12..],&[255,0,0,255,255,255,255,1,0]);
    for code in 0..=11 {
        let period = ExPeriod::known(code)?;
        let request = bars_request(&BarsQuery::new(max.clone(),period).with_start(u32::MAX).with_count(65535)?);
        assert_eq!(request.wire()[12],255); assert_eq!(&request.wire()[13..22],b"123456789");
        assert_eq!(&request.wire()[22..24],&code.to_le_bytes()); assert_eq!(&request.wire()[26..],&[255;6]);
    }
    let last = TradingDate::new(9999,12,31)?;
    assert!(HistoryMinutesQuery::new(max.clone(),TradingDate::new(10000,1,1)?).is_err());
    let current = transactions_request(&TransactionsQuery::new(max.clone()).with_start(i32::MAX as u32)?.with_count(65535)?);
    let history = history_transactions_request(&HistoryTransactionsQuery::new(max,last)?.with_start(i32::MAX as u32)?.with_count(65535)?);
    assert_eq!(&current.wire()[12..],&history.wire()[16..]);
    assert_eq!(&current.wire()[26..],&[255,255]);
    Ok(())
}

#[test]
fn day_boundaries_and_retained_rows_share_payload_allocation() -> TestResult {
    let mut bytes = bar_payload(20,18); bytes[22..24].copy_from_slice(&1439_u16.to_le_bytes());
    let payload = Bytes::from(bytes); let pointer = payload.as_ptr();
    let rows = decode_bars(payload.clone(),&BarsQuery::new(id()?,ExPeriod::Minute5).with_bar_time(BarTime::End)?,RawPolicy::RetainPayload)?;
    let row = &rows.value()[0]; assert_eq!(row.time().clock(),Some((0,4,0)));
    assert_eq!(row.time().date(),Some(TradingDate::new(2004,1,3)?));
    assert_eq!(row.record_raw().ok_or("raw")?.as_ptr(),pointer.wrapping_add(20));
    let mut invalid = bar_payload(20,18); invalid[22..24].copy_from_slice(&1440_u16.to_le_bytes());
    assert!(decode_bars(invalid.into(),&BarsQuery::new(id()?,ExPeriod::Minute1),RawPolicy::Omit).is_err());
    Ok(())
}

#[test]
fn command_headers_and_invalid_clock_fields_are_never_empty_success() -> TestResult {
    for length in 0..2 { assert!(decode_markets(Bytes::from(vec![0;length])).is_err()); }
    for length in 0..6 { assert!(decode_instruments(Bytes::from(vec![0;length])).is_err()); }
    for length in 0..12 { assert!(decode_minutes(Bytes::from(vec![0;length]),RawPolicy::Omit).is_err()); }
    for length in 0..20 { assert!(decode_history_minutes(Bytes::from(vec![0;length]),RawPolicy::Omit).is_err()); }
    for length in 0..16 {
        assert!(decode_transactions(Bytes::from(vec![0;length]),RawPolicy::Omit).is_err());
        assert!(decode_history_transactions(Bytes::from(vec![0;length]),RawPolicy::Omit).is_err());
    }
    let mut invalid = response(12,10,18,1); invalid[12..14].copy_from_slice(&1440_u16.to_le_bytes());
    assert_eq!(decode_minutes(invalid.into(),RawPolicy::Omit).err().map(|e|e.kind()),Some(ErrorKind::Protocol));
    let mut quote = vec![0;150]; quote[1]=0xff;
    assert!(!decode_quote(quote.into(),RawPolicy::Omit)?.diagnostics().is_empty());
    Ok(())
}

#[test]
fn quote_list_every_named_field_maps_to_its_source_slot_and_native_kind() -> TestResult {
    let futures = [
        ("BiShu",0,false),("ZuoJie",1,true),("JinKai",2,true),("ZuiGao",3,true),("ZuiDi",4,true),("MaiChu",5,true),
        ("KaiCang",6,false),("ZongLiang",8,false),("XianLiang",9,false),("ZongJinE",10,true),("NeiPan",11,false),
        ("WaiPan",12,false),("ChiCangLiang",14,false),("MaiRuJia",15,true),("MaiRuLiang",20,false),("MaiChuJia",25,true),("MaiChuLiang",30,false),
    ];
    let hk = [
        ("HuoYueDu",0,false),("ZuoShou",1,true),("JinKai",2,true),("ZuiGao",3,true),("ZuiDi",4,true),("XianJia",5,true),
        ("MaiRuJia",7,true),("ZongLiang",8,false),("XianLiang",9,false),("ZongJinE",10,true),("Nei",13,false),("Wai",14,false),
        ("MaiRuJia1",15,true),("MaiRuJia2",16,true),("MaiRuJia3",17,true),("MaiRuJia4",18,true),("MaiRuJia5",19,true),
        ("MaiRuLiang1",20,false),("MaiRuLiang2",21,false),("MaiRuLiang3",22,false),("MaiRuLiang4",23,false),("MaiRuLiang5",24,false),
        ("MaiChuJia1",25,true),("MaiChuJia2",26,true),("MaiChuJia3",27,true),("MaiChuJia4",28,true),("MaiChuJia5",29,true),
        ("MaiChuLiang1",30,false),("MaiChuLiang2",31,false),("MaiChuLiang3",32,false),("MaiChuLiang4",33,false),("MaiChuLiang5",34,false),
    ];
    for (category, expected) in [(QuoteCategory::Futures,futures.as_slice()),(QuoteCategory::HongKong,hk.as_slice())] {
        let mut bytes = response(2,0,300,1); bytes[2]=31; bytes[3]=b'X';
        for slot in 0..35 { put_u32(&mut bytes,12+slot*4,0x7f800001+slot as u32); }
        let decoded = decode_quote_list(bytes.into(),category)?; let row = &decoded.value()[0];
        assert_eq!(row.fields().len(),expected.len());
        for &(name,slot,is_float) in expected {
            let field = row.field(name).ok_or("field")?;
            assert_eq!(field.slot(),slot as u8);
            let bits = 0x7f800001+slot as u32;
            assert_eq!(field.value(),if is_float { NativeValue::Float(WireFloat::from_bits(bits)) } else { NativeValue::Unsigned(bits) });
            assert_eq!(field.meaning().unit(),&NativeUnit::Unknown);
        }
    }
    Ok(())
}

#[test]
fn filtered_rows_do_not_change_wire_consumption_and_returned_offset_survives() -> TestResult {
    let mut markets = response(2,0,64,2); markets[2]=1; markets[35]=31;
    let decoded = decode_markets(markets.into())?;
    assert_eq!(decoded.wire_row_count(),Some(2)); assert_eq!(decoded.value().len(),1);
    let mut instruments = response(6,4,64,0); put_u32(&mut instruments,0,u32::MAX);
    let decoded = decode_instruments(instruments.into())?;
    assert_eq!(decoded.response_offset(),Some(u32::MAX)); assert_eq!(decoded.wire_row_count(),Some(0));
    Ok(())
}

#[test]
fn pure_codecs_enforce_the_frame_body_ceiling() -> TestResult {
    assert_eq!(decode_markets(Bytes::from(vec![0;65536])).err().map(|e|e.kind()),Some(ErrorKind::LimitExceeded));
    assert_eq!(decode_quote(Bytes::from(vec![0;65536]),RawPolicy::Omit).err().map(|e|e.kind()),Some(ErrorKind::LimitExceeded));
    assert_eq!(WireProfile::Ex.setup_request(),Some(SETUP_REQUEST.as_slice()));
    assert!(WireProfile::MacEx.setup_request().is_none());
    assert!(TimeOfDay::new(24,0,0).is_err());
    assert_eq!(TimeOfDay::new(23,59,59)?.second(),59);
    Ok(())
}

#[test]
fn fixed_width_commands_reject_undeclared_tail_bytes_and_records() -> TestResult {
    fn decode(kind: u8, body: Bytes) -> Result<(), eltdx::Error> {
        match kind {
            0 => decode_markets(body).map(|_| ()),
            1 => decode_instruments(body).map(|_| ()),
            2 => decode_quote_list(body,QuoteCategory::Futures).map(|_| ()),
            3 => decode_quote_list(body,QuoteCategory::HongKong).map(|_| ()),
            4 => decode_bars(body,&BarsQuery::new(id()?,ExPeriod::Minute5),RawPolicy::Omit).map(|_| ()),
            5 => decode_bars_range(body,RawPolicy::Omit).map(|_| ()),
            6 => decode_minutes(body,RawPolicy::Omit).map(|_| ()),
            7 => decode_history_minutes(body,RawPolicy::Omit).map(|_| ()),
            8 => decode_transactions(body,RawPolicy::Omit).map(|_| ()),
            9 => decode_history_transactions(body,RawPolicy::Omit).map(|_| ()),
            _ => unreachable!("test case registry"),
        }
    }
    for (kind,header,count_offset,width) in [
        (0,2,0,64),(1,6,4,64),(2,2,0,300),(3,2,0,300),
        (4,20,18,32),(5,14,12,32),(6,12,10,18),(7,20,18,18),
        (8,16,14,16),(9,16,14,16),
    ] {
        for count in [0,1] {
            let complete = if count == 1 && (kind == 4 || kind == 5) {
                bar_payload(header,count_offset)
            } else { response(header,count_offset,width,count) };
            decode(kind,Bytes::from(complete.clone()))?;
            // Every partial-record suffix and an undeclared complete record
            // fail even when all declared records are otherwise valid.
            for tail_len in 1..=width {
                let mut malformed = complete.clone();
                malformed.resize(malformed.len()+tail_len,0);
                let error = decode(kind,Bytes::from(malformed)).err().ok_or("tail accepted")?;
                assert_eq!(error.kind(),ErrorKind::Protocol);
                assert!(error.context().iter().any(|(key,value)| key == "expected_bytes" && value == &format!("{}",complete.len())));
            }
        }
    }
    for tail_len in [1,4,23] {
        let mut scalar = vec![0;23]; scalar.resize(23+tail_len,0);
        assert_eq!(decode_instrument_count(scalar.into()).err().map(|e|e.kind()),Some(ErrorKind::Protocol));
    }
    for tail_len in [1,4,150] {
        let mut quote = vec![0;150]; quote.resize(150+tail_len,0);
        assert_eq!(decode_quote(quote.into(),RawPolicy::Omit).err().map(|e|e.kind()),Some(ErrorKind::Protocol));
    }
    Ok(())
}

#[test]
fn malformed_clocks_preserve_source_minutes_seconds_and_direction() -> TestResult {
    fn context<'a>(error: &'a eltdx::Error, key: &str) -> Option<&'a str> {
        error.context().iter().find(|(name,_)| name == key).map(|(_,value)|value.as_str())
    }
    let error = eltdx::ex::time::time_of_day(1440,60).err().ok_or("invalid clock accepted")?;
    assert_eq!(error.kind(),ErrorKind::Protocol);
    assert_eq!(context(&error,"raw_minutes"),Some("1440"));
    assert_eq!(context(&error,"raw_seconds"),Some("60"));
    for historical in [false,true] {
        let mut body = response(16,14,16,1);
        body[16..18].copy_from_slice(&1441_u16.to_le_bytes());
        body[30..32].copy_from_slice(&10060_u16.to_le_bytes());
        let result = if historical { decode_history_transactions(body.into(),RawPolicy::Omit) }
            else { decode_transactions(body.into(),RawPolicy::Omit) };
        let error = result.err().ok_or("invalid transaction clock accepted")?;
        assert_eq!(context(&error,"raw_minutes"),Some("1441"));
        assert_eq!(context(&error,"raw_seconds"),Some("60"));
        assert_eq!(context(&error,"direction_raw"),Some("10060"));
        assert_eq!(context(&error,"offset"),Some("16"));
    }
    Ok(())
}

#[test]
fn instrument_identity_forwarding_getters_match_preserved_id() -> TestResult {
    let mut body = response(6,4,64,1);
    body[7]=255; body[11..14].copy_from_slice(b"ABC");
    let decoded = decode_instruments(body.into())?;
    let row = &decoded.value()[0];
    assert_eq!(row.market(),row.id().market()); assert_eq!(row.market().get(),255);
    assert_eq!(row.code(),row.id().code()); assert_eq!(row.code(),"ABC");
    Ok(())
}
