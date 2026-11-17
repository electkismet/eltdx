//! Authored public simple-method fixtures. No execution before T01-T30 sources
//! are complete. Synthetic login replies do not establish production evidence.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"] mod wire;
use bytes::Bytes;
use eltdx::{CancellationToken, ErrorKind, OperationOptions, RawPolicy, TradingDate};
use eltdx::mac::*;
use eltdx::mac::handshake::LoginReplyRules;
use wire::{Case, eof, frame, join, peer, request, setup_mac};

fn u16_at(bytes: &[u8], at: usize) -> u16 { u16::from_le_bytes(bytes[at..at+2].try_into().unwrap()) }
fn u32_at(bytes: &[u8], at: usize) -> u32 { u32::from_le_bytes(bytes[at..at+4].try_into().unwrap()) }
fn put16(bytes: &mut [u8], at: usize, value: u16) { bytes[at..at+2].copy_from_slice(&value.to_le_bytes()); }
fn put32(bytes: &mut [u8], at: usize, value: u32) { bytes[at..at+4].copy_from_slice(&value.to_le_bytes()); }
fn symbol() -> eltdx::Result<MacInstrumentId> { Ok(MacInstrumentId::new(MacMarketId::new(74),MacCode::new("MSFT")?)) }
fn identity(bytes: &[u8]) {
    assert_eq!(u16_at(bytes,0),74); assert_eq!(&bytes[2..6],b"MSFT"); assert!(bytes[6..24].iter().all(|byte| *byte == 0));
}
fn tail() -> Vec<u8> {
    let mut bytes = vec![0;120]; bytes[..4].copy_from_slice(b"NAME"); bytes[44] = 3;
    put16(&mut bytes,45,74); put32(&mut bytes,56,20261117); put32(&mut bytes,60,100102);
    put32(&mut bytes,64,0x8000_0000); put32(&mut bytes,88,1234); put32(&mut bytes,92,0x7fc0_1234);
    bytes[96..108].fill(0xaa); bytes
}
fn chart() -> Vec<u8> {
    let mut bytes = vec![0;53]; put16(&mut bytes,0,74); bytes[2..6].copy_from_slice(b"MSFT");
    put32(&mut bytes,24,20261117); put16(&mut bytes,33,1); put16(&mut bytes,35,571);
    put32(&mut bytes,37,0x8000_0000); put32(&mut bytes,41,0x4120_0000); put32(&mut bytes,45,123);
    put32(&mut bytes,49,0x7fc0_4321); bytes.extend_from_slice(&tail()); bytes
}
fn sampling() -> Vec<u8> {
    let mut bytes = vec![0;50]; put16(&mut bytes,40,2); put32(&mut bytes,42,0x8000_0000); put32(&mut bytes,46,0x7fc0_1111); bytes
}
fn json_body(text: &str) -> Vec<u8> { let mut bytes = vec![0xff;27]; bytes.extend_from_slice(text.as_bytes()); bytes }

#[test]
fn tick_chart_and_sampling_keep_source_bits_and_have_distinct_request_shapes() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?; let body = request(&mut stream,0x1c,0x122d)?;
        identity(&body); assert_eq!(body.len(),38); assert_eq!(u32_at(&body,24),20261116);
        assert_eq!(&body[28..],&[1,0,0,0,0,0,0,0,0,0]); frame(&mut stream,&chart())?;
        let body = request(&mut stream,0x1c,0x254d)?; identity(&body); assert_eq!(u16_at(&body,24),1); assert_eq!(u16_at(&body,26),20);
        frame(&mut stream,&sampling())?; eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainWire).build()?;
    let query = TickChartQuery::new(symbol()?).with_date(Some(TradingDate::new(2026,11,16)?))?;
    let value = client.tick_chart(&query,&OperationOptions::default())?;
    assert_eq!(value.date_raw(),20261117); assert_eq!(value.ticks()[0].time().clock(),Some((9,31,0)));
    assert_eq!(value.ticks()[0].price().value().bits(),0x8000_0000); assert_eq!(value.snapshot().amount().value().bits(),0x7fc0_1234);
    let meta = client.diagnostics()?.latest_meta().cloned().unwrap(); assert_eq!(meta.source().command(),Some(0x122d));
    assert_eq!(meta.raw().unwrap().payloads()[0].as_ref(),chart().as_slice());
    let samples = client.chart_sampling(&ChartSamplingQuery::new(symbol()?),&OperationOptions::default())?;
    assert_eq!(samples.iter().map(|value| value.bits()).collect::<Vec<_>>(),[0x8000_0000,0x7fc0_1111]);
    assert_eq!(client.diagnostics()?.latest_meta().unwrap().source().session_id(),meta.source().session_id()); client.close()?; join(task)
}

#[test]
fn multiday_chart_keeps_short_first_day_and_requested_date_day_count() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?; let body = request(&mut stream,0x1c,0x123e)?; identity(&body);
        assert_eq!(u32_at(&body,24),20261116); assert_eq!(u16_at(&body,28),2); assert_eq!(u16_at(&body,30),1);
        let mut bytes = vec![0;71+42]; put16(&mut bytes,0,74); bytes[2..6].copy_from_slice(b"MSFT");
        put32(&mut bytes,24,20261116); put32(&mut bytes,28,20261117); put16(&mut bytes,64,2); put16(&mut bytes,67,2); put16(&mut bytes,69,3);
        for (index,minute) in [571,572,573].into_iter().enumerate() { put16(&mut bytes,71+14*index,minute); put16(&mut bytes,81+14*index,100+index as u16); }
        bytes.extend_from_slice(&tail()); frame(&mut stream,&bytes)?; eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).build()?;
    let query = TickChartsQuery::new(symbol()?).with_days(2)?.with_date(Some(TradingDate::new(2026,11,16)?))?;
    let chart = client.tick_charts(&query,&OperationOptions::default())?;
    assert_eq!(chart.days().len(),2); assert_eq!(chart.days()[0].date(),TradingDate::new(2026,11,16)?);
    assert_eq!(chart.days()[0].ticks().len(),1); assert_eq!(chart.days()[1].ticks().len(),2);
    assert_eq!(*chart.days()[1].ticks()[0].vol().value(),101); assert!(chart.days()[0].ticks()[0].momentum().is_none()); client.close()?; join(task)
}

#[test]
fn snapshot_auction_and_server_dates_keep_native_fields_and_distinct_clocks() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?; let body = request(&mut stream,0x1c,0x122a)?; identity(&body); assert_eq!(u32_at(&body,24),1);
        let mut bytes = vec![0;196]; put16(&mut bytes,8,74); bytes[10..14].copy_from_slice(b"MSFT");
        put32(&mut bytes,96,20261117); put32(&mut bytes,100,100102); put32(&mut bytes,132,321); put32(&mut bytes,186,0x8000_0000);
        bytes[194..].copy_from_slice(&[0xcc,0xdd]); frame(&mut stream,&bytes)?;
        let body = request(&mut stream,0x1c,0x123d)?; identity(&body); assert_eq!(u32_at(&body,24),0); assert_eq!(u32_at(&body,28),500);
        let mut bytes = vec![0;52]; put32(&mut bytes,24,1); put32(&mut bytes,36,34201); put32(&mut bytes,40,0x4120_0000);
        put32(&mut bytes,44,300); bytes[48..52].copy_from_slice(&(-17_i32).to_le_bytes()); frame(&mut stream,&bytes)?;
        let body = request(&mut stream,0x1c,0x120f)?; assert_eq!(body.len(),68); assert_eq!(&body[..4],&[4,0,0x2d,0x31]);
        let mut bytes = vec![0;79]; put32(&mut bytes,22,20261117); put32(&mut bytes,63,20260230); put16(&mut bytes,30,570); put16(&mut bytes,32,690);
        put32(&mut bytes,71,12); put32(&mut bytes,75,34); frame(&mut stream,&bytes)?; eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainPayload).build()?;
    let snap = client.symbol_info(&SymbolInfoQuery::new(symbol()?),&OperationOptions::default())?;
    assert_eq!(snap.time().unwrap().clock(),Some((10,1,2))); assert_eq!(*snap.vol().value(),321); assert_eq!(snap.unknown_tail(),&[0xcc,0xdd]);
    let auctions = client.auction(&AuctionQuery::new(symbol()?),&OperationOptions::default())?;
    assert_eq!(auctions[0].time().clock(),Some((9,30,1))); assert_eq!(*auctions[0].unmatched().value(),-17);
    assert_eq!(client.diagnostics()?.latest_meta().unwrap().raw().unwrap().row_ranges(),&[(0,36,16)]);
    let session = client.server_info(&OperationOptions::default())?;
    assert_eq!(session.today().parsed(),Some(TradingDate::new(2026,11,17)?)); assert_eq!(session.last_trading_day().parsed(),None);
    assert_eq!(session.sessions_1()[0].open_minutes(),570); assert_eq!(session.market_param_2(),Some(34));
    assert!(client.diagnostics()?.latest_meta().unwrap().diagnostics().iter().any(|value| value.stage() == "server-date"));
    client.close()?; join(task)
}

#[test]
fn belonging_boards_and_flow_use_distinct_heads_tags_and_opaque_prefix() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        for (head,tag,json) in [(1,b"Stock_GLHQ".as_slice(),r#"[[3,10004,"880001","Board",0,null,1,2,3]]"#),
            (2,b"Stock_ZJLX".as_slice(),r#"[["10.5",2,0,null],[0,0,0,"7.5",3]]"#)] {
            let body = request(&mut stream,head,0x1218)?; assert_eq!(body.len(),47); assert_eq!(u16_at(&body,0),74);
            assert_eq!(&body[2..10],b"MSFT\0\0\0\0"); assert_eq!(&body[26..36],tag); frame(&mut stream,&json_body(json))?;
        }
        eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainPayload).build()?;
    let boards = client.belonging_boards(&BelongingBoardsQuery::new(symbol()?)?,&OperationOptions::default())?;
    assert_eq!(boards[0].name(),"Board"); assert_eq!(boards[0].close().value(),Some(0.0)); assert_eq!(boards[0].pre_close().presence(),responses::JsonPresence::Null);
    let flow = client.capital_flow(&CapitalFlowQuery::new(symbol()?)?,&OperationOptions::default())?.unwrap();
    assert_eq!(flow.main_net().value(),Some(8.5)); assert_eq!(flow.small_net().value(),None); assert_eq!(flow.mid_net().value(),Some(3.0));
    assert_eq!(flow.large_net().value(),Some(7.5)); assert!(flow.date().is_none());
    let meta = client.diagnostics()?.latest_meta().cloned().unwrap(); assert!(meta.raw().unwrap().row_ranges().is_empty());
    assert_eq!(&meta.raw().unwrap().payloads()[0][..27],&[0xff;27]); client.close()?; join(task)
}

#[test]
fn file_meta_chunk_and_offsets_keep_separate_layouts_and_original_payloads() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?; let body = request(&mut stream,0x1c,0x1215)?;
        assert_eq!(body.len(),104); assert_eq!(u32_at(&body,0),17); assert_eq!(&body[4..15],b"fixture.dat");
        let mut bytes = vec![0;43]; put32(&mut bytes,0,17); put32(&mut bytes,4,30001); bytes[8] = 0xff; bytes[9..13].copy_from_slice(b"abcd"); bytes[41..].copy_from_slice(&[0x12,0x34]); frame(&mut stream,&bytes)?;
        let body = request(&mut stream,0x1c,0x1217)?; assert_eq!(body.len(),112);
        assert_eq!((u32_at(&body,0),u32_at(&body,4),u32_at(&body,8)),(3,19,4)); assert_eq!(&body[12..23],b"fixture.dat");
        frame(&mut stream,&[1,2,3,4,5,6,7,8,0xab,0xcd])?;
        let body = request(&mut stream,0x1c,0x124a)?; assert_eq!(body.len(),13); assert_eq!((u32_at(&body,0),u32_at(&body,4)),(9,23));
        frame(&mut stream,&[0,0,1,0,2,0,0,0])?; eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainWire).build()?; let file = RemoteFileName::new("fixture.dat")?;
    let meta = client.file_meta(&FileMetaQuery::new(file.clone()).with_offset(17),&OperationOptions::default())?;
    assert_eq!((meta.offset(),meta.size(),meta.flag()),(17,30001,-1)); assert_eq!(meta.hash(),"abcd"); assert_eq!(meta.unknown_tail(),&[0x12,0x34]);
    let chunk = client.download_chunk(&DownloadChunkQuery::new(file).with_index(3)?.with_offset(19)?.with_size(4)?,&OperationOptions::default())?;
    assert_eq!(chunk,[0xab,0xcd]); assert_eq!(client.diagnostics()?.latest_meta().unwrap().raw().unwrap().payloads()[0].as_ref(),&[1,2,3,4,5,6,7,8,0xab,0xcd]);
    let offsets = client.kline_offsets(&KlineOffsetsQuery::new().with_offset(9)?.with_count(23)?,&OperationOptions::default())?;
    assert_eq!((offsets.total(),offsets.returned()),(256,2)); client.close()?; join(task)
}

#[test]
fn native_goods_single_page_keeps_opaque_slots_and_never_claims_unproven_eof() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?; let body = request(&mut stream,0x1c,0x2562)?;
        assert_eq!(body.len(),10); assert_eq!(u16_at(&body,0),38); assert_eq!((u32_at(&body,2),u32_at(&body,6)),(7,2));
        let mut bytes = vec![0;50]; put16(&mut bytes,0,1); put16(&mut bytes,2,123); bytes[4..9].copy_from_slice(b"1_GDP");
        put16(&mut bytes,27,7); put32(&mut bytes,29,98765); bytes[33] = 9; put32(&mut bytes,34,0x7fc0_1234); put32(&mut bytes,38,0x8000_0000);
        put16(&mut bytes,46,10); put16(&mut bytes,48,11); frame(&mut stream,&bytes)?; eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainPayload).build()?;
    let page = client.goods(&GoodsQuery::new(MacMarketId::new(38)).with_start(7)?.with_count(2)?,&OperationOptions::default())?;
    assert_eq!(page.completion(),PageCompletion::Unknown); assert_eq!(page.next_offset(),Some(8)); assert_eq!(page.server_total(),None);
    let row = &page.rows()[0]; assert_eq!(row.actual_code(),"1_GDP"); assert!(!row.is_derived()); assert_eq!(row.index(),98765);
    assert_eq!(row.code()[0].bits(),0x7fc0_1234); assert_eq!(row.code()[1].bits(),0x8000_0000); assert_eq!(page.meta().raw().unwrap().row_ranges(),&[(0,2,48)]);
    client.close()?; join(task)
}

#[test]
fn mac_ex_shared_chart_and_sampling_use_the_same_login_owner_and_head_rewrite() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        request(&mut stream,1,0x2454)?; frame(&mut stream,b"fixture accepted")?;
        let body = request(&mut stream,1,0x122d)?; identity(&body); assert_eq!(u32_at(&body,24),0); frame(&mut stream,&chart())?;
        let body = request(&mut stream,1,0x254d)?; identity(&body); frame(&mut stream,&sampling())?; eof(&mut stream)
    })?;
    let rules = LoginReplyRules::new(vec![Bytes::from_static(b"fixture accepted")],vec![Bytes::from_static(b"fixture denied")],"synthetic simple fixture")?;
    let client = MacExClient::builder().with_endpoints(vec![endpoint]).with_login_reply_rules(rules).build()?;
    let id = eltdx::ex::ExInstrumentId::new(eltdx::ex::ExMarketId::from_wire(74),eltdx::ex::ExCode::new("MSFT")?);
    let chart = client.tick_chart(&ex_query::TickChartQuery::new(id.clone())?,&OperationOptions::default())?;
    assert_eq!(chart.id().market().get(),74); assert_eq!(chart.ticks().len(),1);
    let source = client.diagnostics()?.latest_meta().unwrap().source().session_id().to_owned();
    let values = client.chart_sampling(&ex_query::ChartSamplingQuery::new(id)?,&OperationOptions::default())?;
    assert_eq!(values[1].bits(),0x7fc0_1111); assert_eq!(client.diagnostics()?.latest_meta().unwrap().source().session_id(),source); client.close()?; join(task)
}

#[test]
fn malformed_simple_payloads_fail_at_public_entry_without_reusing_the_socket() -> Case {
    for command in [0x122d,0x123e,0x254d,0x122a,0x123d,0x120f,0x124a,0x1215,0x1217,0x2562,0x1218] {
        let (endpoint,task) = peer(move |mut stream| {
            setup_mac(&mut stream)?; request(&mut stream,if command == 0x1218 { 2 } else { 0x1c },command)?;
            frame(&mut stream,&[])?; eof(&mut stream)
        })?;
        let client = MacClient::builder().with_endpoints(vec![endpoint]).build()?; let options = OperationOptions::default(); let id = symbol()?;
        let result = match command {
            0x122d => client.tick_chart(&TickChartQuery::new(id),&options).map(|_| ()),
            0x123e => client.tick_charts(&TickChartsQuery::new(id),&options).map(|_| ()),
            0x254d => client.chart_sampling(&ChartSamplingQuery::new(id),&options).map(|_| ()),
            0x122a => client.symbol_info(&SymbolInfoQuery::new(id),&options).map(|_| ()),
            0x123d => client.auction(&AuctionQuery::new(id),&options).map(|_| ()),
            0x120f => client.server_info(&options).map(|_| ()),
            0x124a => client.kline_offsets(&KlineOffsetsQuery::new(),&options).map(|_| ()),
            0x1215 => client.file_meta(&FileMetaQuery::new(RemoteFileName::new("a")?),&options).map(|_| ()),
            0x1217 => client.download_chunk(&DownloadChunkQuery::new(RemoteFileName::new("a")?),&options).map(|_| ()),
            0x2562 => client.goods(&GoodsQuery::new(MacMarketId::new(38)),&options).map(|_| ()),
            _ => client.capital_flow(&CapitalFlowQuery::new(id)?,&options).map(|_| ()),
        };
        assert_eq!(result.unwrap_err().kind(),ErrorKind::Protocol); assert!(client.host()?.is_none()); client.close()?; join(task)?;
    }
    Ok(())
}

#[test]
fn precancelled_simple_services_do_not_open_a_connection() -> Case {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST,0))?;
    let client = MacClient::builder().with_endpoints(vec![listener.local_addr()?]).build()?;
    let token = CancellationToken::default(); token.cancel(); let options = OperationOptions::default().with_cancellation(token);
    assert_eq!(client.tick_chart(&TickChartQuery::new(symbol()?),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.tick_charts(&TickChartsQuery::new(symbol()?),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.chart_sampling(&ChartSamplingQuery::new(symbol()?),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.symbol_info(&SymbolInfoQuery::new(symbol()?),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.auction(&AuctionQuery::new(symbol()?),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.server_info(&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.capital_flow(&CapitalFlowQuery::new(symbol()?)?,&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.belonging_boards(&BelongingBoardsQuery::new(symbol()?)?,&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.goods(&GoodsQuery::new(MacMarketId::new(38)),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.file_meta(&FileMetaQuery::new(RemoteFileName::new("a")?),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.download_chunk(&DownloadChunkQuery::new(RemoteFileName::new("a")?),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    assert_eq!(client.kline_offsets(&KlineOffsetsQuery::new(),&options).unwrap_err().kind(),ErrorKind::Cancelled);
    listener.set_nonblocking(true)?; assert_eq!(listener.accept().unwrap_err().kind(),std::io::ErrorKind::WouldBlock); client.close()?; Ok(())
}
