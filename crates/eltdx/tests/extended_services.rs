//! Source-only public composition cases. Run only after all T01-T30 code exists.
#![cfg(feature = "mac")]
#[path = "support/extension_peer.rs"] mod wire;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::{Arc, atomic::{AtomicUsize, Ordering}};
use bytes::Bytes;
use eltdx::{ErrorKind, OperationOptions, PageOptions, RawPolicy, TradingDate};
use eltdx::mac::*;
use eltdx::types::{RawData, ResultMeta, SourceStamp};
use wire::{Case, bars, bars_request, eof, frame, join, peer, qfq, qfq_then_raw, setup_mac};

#[derive(Debug)]
struct Provider { calls: Arc<AtomicUsize>, batch: XdxrBatch, failure: Option<ErrorKind> }
impl XdxrProvider for Provider {
    fn events(&self, id: &MacInstrumentId, context: &XdxrContext) -> eltdx::Result<XdxrBatch> {
        self.calls.fetch_add(1,Ordering::SeqCst); context.check_boundary()?;
        assert_eq!(id,&wire::id()?); assert!(!context.remaining()?.is_zero());
        assert!(context.remaining_event_capacity()? > 0);
        if let Some(kind) = self.failure { return Err(eltdx::Error::new(kind,"explicit fixture provider failure")); }
        Ok(self.batch.clone())
    }
}
fn event(amount: f64) -> eltdx::Result<XdxrEvent> {
    Ok(XdxrEvent { fenhong: amount,..XdxrEvent::new(TradingDate::new(2026,11,18)?) })
}
fn provider(events: Vec<XdxrEvent>) -> (Arc<Provider>,Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    (Arc::new(Provider { calls: Arc::clone(&calls),batch: XdxrBatch::new(events),failure: None }),calls)
}
fn diagnostic(rows: &[Bar], stage: &str) -> bool { rows[0].meta().diagnostics().iter().any(|item| item.stage() == stage) }

#[test]
fn healthy_qfq_and_other_adjust_modes_never_refetch_or_call_provider() -> Case {
    for (adjust,price) in [(MacAdjust::Qfq,10.0),(MacAdjust::None,-1.0),(MacAdjust::Hfq,-1.0)] {
        let (endpoint,task) = peer(move |mut stream| {
            setup_mac(&mut stream)?; bars_request(&mut stream,0x1c,adjust.wire(),0,2)?;
            frame(&mut stream,&bars(&[(20261117,price),(20261118,20.0)])?)?; eof(&mut stream)
        })?;
        let (source,calls) = provider(vec![event(1.0)?]);
        let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(source).build()?;
        let rows = client.bars(&qfq()?.with_adjust(adjust),&OperationOptions::default())?;
        assert_eq!(rows[0].close().value().as_f64(),f64::from(price)); assert_eq!(calls.load(Ordering::SeqCst),0);
        client.close()?; join(task)?;
    }
    Ok(())
}

#[test]
fn repair_keeps_original_refetch_and_provider_evidence_but_selects_only_returned_raw_rows() -> Case {
    let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let source_meta = ResultMeta::new(SourceStamp::new(None,None,"explicit-event-source",None,None),Vec::new(),
        Some(RawData::new(vec![Bytes::from_static(b"event")],Some(vec![Bytes::from_static(b"event wire")]),vec![(0,0,5)])));
    let calls = Arc::new(AtomicUsize::new(0));
    let source = Arc::new(Provider { calls: Arc::clone(&calls),batch: XdxrBatch::new(vec![event(1.0)?]).with_meta(source_meta),failure: None });
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainWire).with_xdxr_provider(source).build()?;
    let rows = client.bars(&qfq()?,&OperationOptions::default())?;
    assert!((rows[0].close().value().as_f64()-9.0).abs() < 1e-12);
    assert_eq!(rows[0].close().value().bits(),10.0_f32.to_bits());
    assert_eq!(rows[0].close().value().source(),BarPriceSource::LocalForwardAdjustment);
    assert_eq!(rows[1].close().value().source(),BarPriceSource::Wire);
    assert_eq!(rows[0].amount().value().as_f64(),1234.5);
    assert_eq!(rows[0].volume().value().bits(),0x8000_0000);
    assert_eq!(rows[0].adjustment_events()[0].affected_rows(),1);
    assert_eq!(calls.load(Ordering::SeqCst),1); assert!(diagnostic(&rows,"AdjustmentFallbackApplied"));
    assert!(std::ptr::eq(rows[0].meta(),rows[1].meta()));
    let raw = rows[0].meta().raw().expect("retained repair sources");
    assert_eq!(raw.payloads().len(),3); assert_eq!(raw.wire_frames().expect("all wires").len(),3);
    assert_eq!(raw.row_ranges(),&[(1,33,36),(1,69,36)]);
    assert_eq!(u32::from_le_bytes(raw.payloads()[0][41..45].try_into()?),(-1.0_f32).to_bits());
    assert_eq!(raw.payloads()[2].as_ref(),b"event");
    assert_eq!(rows[0].record_raw().expect("unadjusted record").as_ref(),&raw.payloads()[1][33..69]);
    assert_eq!(rows[0].meta().sources()[2].session_id(),"explicit-event-source");
    let latest = client.diagnostics()?.latest_meta().expect("final aggregate").clone();
    assert_eq!(latest.raw().expect("latest raw").row_ranges(),raw.row_ranges());
    client.close()?; join(task)
}

#[test]
fn absent_provider_still_refetches_then_preserves_original_qfq_with_diagnostic() -> Case {
    let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(RawPolicy::RetainPayload).build()?;
    let rows = client.bars(&qfq()?,&OperationOptions::default())?;
    assert_eq!(rows[0].close().value().as_f64(),-1.0); assert_eq!(rows[0].close().value().source(),BarPriceSource::Wire);
    assert!(diagnostic(&rows,"AdjustmentFallbackUnavailable"));
    let raw = rows[0].meta().raw().expect("both observations retained");
    assert_eq!(raw.payloads().len(),2); assert_eq!(raw.row_ranges(),&[(0,33,36),(0,69,36)]);
    assert!(raw.wire_frames().is_none()); client.close()?; join(task)
}

#[test]
fn strict_unavailable_and_still_invalid_are_errors_with_final_diagnostics() -> Case {
    for still_invalid in [false,true] {
        let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
        let mut builder = MacClient::builder().with_endpoints(vec![endpoint]);
        if still_invalid { builder = builder.with_xdxr_provider(provider(vec![event(20.0)?]).0); }
        let client = builder.build()?;
        let error = client.bars(&qfq()?.with_strict_adjustment(true),&OperationOptions::default()).expect_err("strict repair failure");
        let (kind,stage) = if still_invalid { (ErrorKind::Protocol,"AdjustmentStillInvalid") }
            else { (ErrorKind::MetadataUnavailable,"AdjustmentFallbackUnavailable") };
        assert_eq!(error.kind(),kind); assert!(error.context().iter().any(|(key,value)| key == "adjustment" && value == stage));
        assert!(client.diagnostics()?.latest_meta().expect("strict final metadata").diagnostics().iter().any(|item| item.stage() == stage));
        client.close()?; join(task)?;
    }
    Ok(())
}

#[test]
fn empty_or_all_skipped_events_do_not_publish_unadjusted_prices_as_successful_qfq() -> Case {
    for events in [Vec::new(),vec![XdxrEvent { category: 2,..event(1.0)? }],vec![event(f64::NAN)?]] {
        let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
        let expected_events = events.len(); let (source,calls) = provider(events);
        let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(source).build()?;
        let rows = client.bars(&qfq()?,&OperationOptions::default())?;
        assert_eq!(rows[0].close().value().as_f64(),-1.0); assert!(diagnostic(&rows,"AdjustmentFallbackUnavailable"));
        assert_eq!(rows[0].adjustment_events().len(),expected_events);
        assert!(rows[0].adjustment_events().iter().all(|event| event.skipped_reason().is_some()));
        assert_eq!(calls.load(Ordering::SeqCst),1); client.close()?; join(task)?;
    }
    Ok(())
}

#[test]
fn still_invalid_finite_adjustment_preserves_original_and_detailed_attempt_evidence() -> Case {
    let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(provider(vec![event(20.0)?]).0).build()?;
    let rows = client.bars(&qfq()?,&OperationOptions::default())?;
    assert_eq!(rows[0].close().value().as_f64(),-1.0); assert!(diagnostic(&rows,"AdjustmentStillInvalid"));
    assert_eq!(rows[0].adjustment_events()[0].factor(),Some(-1.0));
    assert_eq!(rows[0].close().value().source(),BarPriceSource::Wire);
    client.close()?; join(task)
}

#[test]
fn provider_errors_including_cancel_and_timeout_are_never_downgraded() -> Case {
    for kind in [ErrorKind::Io,ErrorKind::Cancelled,ErrorKind::Timeout] {
        let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
        let source = Arc::new(Provider { calls: Arc::new(AtomicUsize::new(0)),batch: XdxrBatch::new(Vec::new()),failure: Some(kind) });
        let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(source).build()?;
        let error = client.bars(&qfq()?,&OperationOptions::default()).expect_err("provider failure propagates");
        assert_eq!(error.kind(),kind); client.close()?; join(task)?;
    }
    Ok(())
}

#[test]
fn empty_refetch_avoids_provider_and_bad_refetch_is_not_a_successful_fallback() -> Case {
    for malformed in [false,true] {
        let (endpoint,task) = peer(move |mut stream| {
            setup_mac(&mut stream)?; bars_request(&mut stream,0x1c,1,0,2)?;
            frame(&mut stream,&bars(&[(20261117,-1.0),(20261118,20.0)])?)?;
            bars_request(&mut stream,0x1c,0,0,2)?;
            let mut body = bars(&[])?; if malformed { body[27..29].copy_from_slice(&1_u16.to_le_bytes()); }
            frame(&mut stream,&body)?; eof(&mut stream)
        })?;
        let (source,calls) = provider(vec![event(1.0)?]);
        let client = MacClient::builder().with_endpoints(vec![endpoint]).with_xdxr_provider(source).build()?;
        let result = client.bars(&qfq()?,&OperationOptions::default());
        if malformed { assert_eq!(result.expect_err("malformed raw page").kind(),ErrorKind::Protocol); }
        else { let rows = result?; assert_eq!(rows[0].close().value().as_f64(),-1.0); assert!(diagnostic(&rows,"AdjustmentFallbackUnavailable")); }
        assert_eq!(calls.load(Ordering::SeqCst),0); client.close()?; join(task)?;
    }
    Ok(())
}

#[test]
fn zero_extra_budget_refuses_the_first_refetch_before_sending_and_retains_the_original_cause() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?; bars_request(&mut stream,0x1c,1,0,2)?;
        frame(&mut stream,&bars(&[(20261117,-1.0),(20261118,20.0)])?)?; eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_extra_attempt_budget(0).build()?;
    let error = client.bars(&qfq()?,&OperationOptions::default()).expect_err("fallback reservation denied");
    assert_eq!(error.kind(),ErrorKind::LimitExceeded);
    assert!(error.context().iter().any(|(key,value)| key == "limit" && value == "extra_attempts"));
    assert!(std::error::Error::source(&error).is_some()); client.close()?; join(task)
}

#[test]
fn only_first_refetch_page_consumes_extra_and_sorted_repair_raw_mapping_matches_returned_rows() -> Case {
    let (endpoint,task) = peer(|mut stream| {
        setup_mac(&mut stream)?;
        for (adjust,last_price) in [(1,-1.0),(0,10.0)] {
            bars_request(&mut stream,0x1c,adjust,0,2)?;
            frame(&mut stream,&bars(&[(20261118,20.0),(20261119,30.0)])?)?;
            bars_request(&mut stream,0x1c,adjust,2,1)?;
            frame(&mut stream,&bars(&[(20261117,last_price)])?)?;
        }
        eof(&mut stream)
    })?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_extra_attempt_budget(1)
        .with_raw(RawPolicy::RetainPayload).with_xdxr_provider(provider(vec![event(1.0)?]).0).build()?;
    let query = qfq()?.with_count(3)?.with_pages(PageOptions::new(2,3,10)?)?;
    let rows = client.bars(&query,&OperationOptions::default())?;
    assert_eq!(rows.iter().map(|row| row.close().value().as_f64()).collect::<Vec<_>>(),[9.0,20.0,30.0]);
    assert_eq!(rows[0].meta().raw().expect("four source replies").row_ranges(),&[(3,33,36),(2,33,36),(2,69,36)]);
    client.close()?; join(task)
}

#[derive(Debug)]
struct ReservedCapacityProvider;
impl XdxrProvider for ReservedCapacityProvider {
    fn events(&self, _: &MacInstrumentId, context: &XdxrContext) -> eltdx::Result<XdxrBatch> {
        context.check_boundary()?;
        let mut events = Vec::with_capacity(4096); events.push(event(1.0)?);
        Ok(XdxrBatch::new(events))
    }
}
#[test]
fn provider_reserved_storage_counts_against_the_existing_operation_byte_cap() -> Case {
    let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
    let client = MacClient::builder().with_endpoints(vec![endpoint]).with_max_operation_bytes(NonZeroUsize::new(4096).expect("byte cap"))
        .with_xdxr_provider(Arc::new(ReservedCapacityProvider)).build()?;
    let error = client.bars(&qfq()?,&OperationOptions::default()).expect_err("provider capacity over budget");
    assert_eq!(error.kind(),ErrorKind::LimitExceeded); client.close()?; join(task)
}

#[derive(Debug)]
struct PhysicalSourceProvider;
impl XdxrProvider for PhysicalSourceProvider {
    fn events(&self, _: &MacInstrumentId, context: &XdxrContext) -> eltdx::Result<XdxrBatch> {
        let mut request = context.source_request()?; assert_eq!(request.started_attempts(),0);
        request.start_attempt()?; assert_eq!(request.started_attempts(),1);
        Ok(XdxrBatch::new(vec![event(1.0)?]))
    }
}
#[test]
fn explicit_external_source_uses_parent_extra_gate_but_in_memory_events_do_not() -> Case {
    for extra in [1,2] {
        let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
        let client = MacClient::builder().with_endpoints(vec![endpoint]).with_extra_attempt_budget(extra)
            .with_wire_attempt_limit(NonZeroU32::MIN).with_xdxr_provider(Arc::new(PhysicalSourceProvider)).build()?;
        let result = client.bars(&qfq()?,&OperationOptions::default());
        if extra == 1 { assert_eq!(result.expect_err("only refetch allowance remained").kind(),ErrorKind::LimitExceeded); }
        else { assert!(diagnostic(&result?,"AdjustmentFallbackApplied")); }
        client.close()?; join(task)?;
    }
    Ok(())
}

#[test]
fn provider_raw_follows_operation_policy_and_malformed_ranges_fail_before_publication() -> Case {
    for (policy,malformed) in [(RawPolicy::Omit,false),(RawPolicy::RetainPayload,false),(RawPolicy::RetainWire,true)] {
        let (endpoint,task) = peer(|mut stream| qfq_then_raw(&mut stream))?;
        let meta = ResultMeta::new(SourceStamp::new(None,None,"explicit source",None,None),Vec::new(),
            Some(RawData::new(vec![Bytes::from_static(b"event")],Some(vec![Bytes::from_static(b"wire")]),vec![(0,0,if malformed { 6 } else { 5 })])));
        let source = Arc::new(Provider { calls: Arc::new(AtomicUsize::new(0)),batch: XdxrBatch::new(vec![event(1.0)?]).with_meta(meta),failure: None });
        let client = MacClient::builder().with_endpoints(vec![endpoint]).with_raw(policy).with_xdxr_provider(source).build()?;
        let result = client.bars(&qfq()?,&OperationOptions::default());
        if malformed { assert_eq!(result.expect_err("provider raw bounds").kind(),ErrorKind::InvalidArgument); }
        else { let rows = result?; if policy == RawPolicy::Omit { assert!(rows[0].meta().raw().is_none()); }
            else { assert!(rows[0].meta().raw().expect("retained payload only").wire_frames().is_none()); } }
        client.close()?; join(task)?;
    }
    Ok(())
}
