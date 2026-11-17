//! MAC compositions share the public operation, including repair fetches and
//! explicitly injected event sources. No HQ client or implicit identity cache.
use std::sync::Arc;
use std::time::{Duration, Instant};
use crate::{Error, ErrorKind, RawPolicy, Result, TimeoutPhase};
use crate::operation::{ChildPurpose, OperationContext};
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta, SourceStamp};
use crate::ex::control::Control;
use super::{Bar, BarsQuery, MacAdjust, MacInstrumentId, XdxrEvent};
use super::collect::{self, Collection};
use super::session::MacOwner;
use super::util::{self, AdjustmentEvent};

/// An owner-free view of the enclosing bars operation. A provider must check
/// this boundary during CPU work and bound its own I/O by remaining(). It must
/// not synchronously reenter the same MAC owner whose serial lease it holds.
/// Uncooperative providers cannot be killed: unconfirmed cleanup is reported
/// as CancelUnconfirmed and the registered worker keeps its serial slot.
#[derive(Debug)]
pub struct XdxrContext { control: Control, raw: RawPolicy, maximum_attempts: u32 }
impl XdxrContext {
    pub fn check_boundary(&self) -> Result<()> { self.control.check(TimeoutPhase::Aggregate) }
    pub const fn deadline(&self) -> Instant { self.control.deadline }
    pub fn remaining(&self) -> Result<Duration> {
        self.check_boundary()?;
        Ok(self.control.deadline.saturating_duration_since(Instant::now()))
    }
    pub const fn raw_policy(&self) -> RawPolicy { self.raw }
    /// Upper bound before provider metadata and retained allocations are charged.
    pub fn remaining_event_capacity(&self) -> Result<usize> {
        self.check_boundary()?;
        Ok(self.control.sponsor.remaining_bytes()? / std::mem::size_of::<XdxrEvent>())
    }
    /// Explicit external providers can charge streamed input as it arrives.
    /// SDK retention is also charged when the returned batch is accepted.
    pub fn record_bytes(&self, count: usize) -> Result<()> {
        self.check_boundary()?; self.control.sponsor.record_bytes(count)
    }
    /// Providers that read an external source must create one request per
    /// logical source query, then call start_attempt immediately before each
    /// connect/send. In-memory providers do not consume physical attempts.
    pub fn source_request(&self) -> Result<XdxrSourceRequest> {
        self.check_boundary()?;
        Ok(XdxrSourceRequest { authorization: self.control.sponsor.fallback_authorization()?,
            control: self.control.clone(),started: 0,maximum: self.maximum_attempts })
    }
}

/// One provider query's finite attempts on the parent operation's shared gate.
/// The first attempt and each retry consume an extra allowance. No refund is
/// made for an already started request; a failed reservation starts no I/O.
pub struct XdxrSourceRequest {
    authorization: Arc<dyn eltdx_runtime::execution::AttemptAuthorization>,
    control: Control, started: u32, maximum: u32,
}
impl std::fmt::Debug for XdxrSourceRequest {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.debug_struct("XdxrSourceRequest").field("started",&self.started).field("maximum",&self.maximum).finish_non_exhaustive()
    }
}
impl XdxrSourceRequest {
    pub const fn started_attempts(&self) -> u32 { self.started }
    pub fn start_attempt(&mut self) -> Result<()> {
        self.control.check(TimeoutPhase::Send)?;
        let next = self.started.checked_add(1).filter(|next| *next <= self.maximum)
            .ok_or_else(|| Error::new(ErrorKind::LimitExceeded,"XDXR source request reached its physical attempt limit")
                .with_context("limit","wire_attempts").with_context("configured",format!("{}",self.maximum)))?;
        let reserved = self.authorization.reserve(next,None).map_err(Error::from_runtime)?;
        reserved.start_before(self.control.deadline).map_err(Error::from_runtime)?;
        self.started = next; Ok(())
    }
}

/// An explicitly supplied source may attach its actual provenance. The default
/// stamp only says injected; it does not claim a server command or HQ identity.
#[derive(Clone, Debug)]
pub struct XdxrBatch { events: Vec<XdxrEvent>, meta: ResultMeta }
impl XdxrBatch {
    pub fn new(events: Vec<XdxrEvent>) -> Self {
        Self { events,meta: ResultMeta::new(SourceStamp::new(None,None,"injected-xdxr-provider",None,None),Vec::new(),None) }
    }
    pub fn with_meta(mut self, meta: ResultMeta) -> Self { self.meta = meta; self }
    pub fn events(&self) -> &[XdxrEvent] { &self.events }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

/// Called only for nonempty, invalid server QFQ after an unadjusted refetch.
/// Errors propagate; cancellation, timeout and failed event retrieval are never
/// converted into successful original prices. Calls run in the existing owner
/// task registry and share the bars operation's deadline and byte allowance.
pub trait XdxrProvider: std::fmt::Debug + Send + Sync {
    fn events(&self, id: &MacInstrumentId, context: &XdxrContext) -> Result<XdxrBatch>;
}

pub(crate) fn bars(owner: &MacOwner, context: &OperationContext, query: &BarsQuery,
    provider: Option<Arc<dyn XdxrProvider>>) -> Result<Collection<Bar>> {
    bars_with_source(owner,context,query,provider,None)
}
pub(crate) fn bars_with_source(owner: &MacOwner, context: &OperationContext, query: &BarsQuery,
    provider: Option<Arc<dyn XdxrProvider>>, source: Option<(super::Endpoint,u64)>) -> Result<Collection<Bar>> {
    let original = collect::bars_with_source(owner,context,query,ChildPurpose::Page,source)?;
    if !super::python_legacy::repair_needed_with(|| Ok(query.adjust() == MacAdjust::Qfq),
        || Ok(!original.rows.is_empty()),|| bad_prices(context,&original.rows))? {
        return Ok(original);
    }
    // This is a separate logical fallback, not a retry of the QFQ request. The
    // first physical refetch consumes one shared extra; normal next pages do not.
    // Keep the original query's identity, target, pages and display alignment.
    context.remember_recoverable(Error::new(ErrorKind::Protocol,"server QFQ contains nonpositive or nonfinite OHLC")
        .with_context("domain","mac").with_context("adjustment","InvalidServerQfq"));
    let raw_query = query.clone().with_adjust(MacAdjust::None).with_strict_adjustment(false);
    let unadjusted = collect::bars_with_source(owner,context,&raw_query,ChildPurpose::Fallback,source)?;
    let mut stages = vec![original.meta.clone(),unadjusted.meta.clone()];
    let rejected_ranges = original.meta.raw().map_or(0,|raw| raw.row_ranges().len());
    if unadjusted.rows.is_empty() {
        return fallback(owner,context,query,original,&stages,Fallback::Unavailable,
            "unadjusted refetch returned no bars",None);
    }
    let Some(provider) = provider else {
        return fallback(owner,context,query,original,&stages,Fallback::Unavailable,
            "no XDXR provider was explicitly configured",None);
    };
    let batch = load_events(owner,context,query.id(),provider)?;
    stages.push(batch.meta);
    if batch.events.is_empty() {
        return fallback(owner,context,query,original,&stages,Fallback::Unavailable,
            "the explicitly configured XDXR provider returned no events",None);
    }
    let adjusted = util::forward_adjustment_with_boundary(&unadjusted.rows,&batch.events,
        || context.check_boundary(TimeoutPhase::Aggregate))?;
    let applied = adjusted.events().iter().filter(|event| event.factor().is_some()).count();
    let evidence = adjusted.shared_events();
    let computation = Diagnostic::new("mac","local-forward-adjustment",
        if applied > 0 { EnrichmentStatus::Succeeded } else { EnrichmentStatus::Unavailable },None,
        Some(format!("events={}; applied={applied}; skipped={}; detailed events are shared by returned rows",adjusted.events().len(),adjusted.events().len()-applied)));
    if let Some(last) = stages.last_mut() { *last = last.clone().with_diagnostic(computation); }
    if bad_prices(context,adjusted.rows())? {
        return fallback(owner,context,query,original,&stages,Fallback::StillInvalid,
            "local forward adjustment still contains nonpositive or nonfinite OHLC",Some(evidence));
    }
    if applied == 0 {
        return fallback(owner,context,query,original,&stages,Fallback::Unavailable,
            "no XDXR event produced an applicable finite factor",Some(evidence));
    }
    // Retain every original/refetch/provider payload in chronological source
    // order; only primary row references select the returned refetch series.
    let mut selected = Vec::new(); selected.try_reserve(adjusted.source_indices().len()).map_err(crate::services::allocation_error)?;
    for (position,&index) in adjusted.source_indices().iter().enumerate() {
        if position % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        selected.push(rejected_ranges.checked_add(index).ok_or_else(|| Error::new(ErrorKind::LimitExceeded,"MAC repair raw index overflows"))?);
    }
    let meta = crate::services::aggregate_meta(context,&stages,None)?.with_raw_row_selection(&selected)?
        .with_diagnostic(Diagnostic::new("mac","AdjustmentFallbackApplied",EnrichmentStatus::Succeeded,None,
            Some("invalid server QFQ replaced by the explicitly sourced unadjusted series and local XDXR calculation".into())));
    let mut result = Collection { rows: adjusted.into_rows(),meta,next: unadjusted.next,
        completion: unadjusted.completion,total: unadjusted.total };
    attach_meta(context,&mut result,None)?;
    owner.session.record_meta(result.meta.clone())?;
    Ok(result)
}

fn load_events(owner: &MacOwner, context: &OperationContext, id: &MacInstrumentId,
    provider: Arc<dyn XdxrProvider>) -> Result<XdxrBatch> {
    let lease = owner.session.lease(context)?;
    let id = id.clone(); let raw = context.raw(); let maximum_attempts = owner.config.wire_attempt_limit().get();
    let batch = crate::ex::worker::run(Arc::clone(&owner.session.stopped),context,lease.worker_slot(),move |control| {
        let view = XdxrContext { control,raw,maximum_attempts };
        view.check_boundary()?;
        let mut batch = provider.events(&id,&view)?;
        view.check_boundary()?;
        let mut memory = crate::types::memory::MemoryCharge::default();
        // Charge retained capacity, not just length. A tiny result backed by an
        // enormous provider allocation cannot evade this operation's byte cap.
        memory.array::<XdxrEvent>(batch.events.capacity())?;
        batch.meta.charge_heap(&mut memory)?;
        view.control.sponsor.record_bytes(memory.total())?;
        for (index,_) in batch.events.iter().enumerate() { if index % 256 == 0 { view.check_boundary()?; } }
        if let Some(retained) = batch.meta.raw() {
            for (index,&(payload,offset,len)) in retained.row_ranges().iter().enumerate() {
                if index % 256 == 0 { view.check_boundary()?; }
                let bytes = retained.payloads().get(payload).ok_or_else(|| Error::new(ErrorKind::InvalidArgument,"XDXR provider raw range names an absent payload"))?;
                if offset.checked_add(len).is_none_or(|end| end > bytes.len()) {
                    return Err(Error::new(ErrorKind::InvalidArgument,"XDXR provider raw range exceeds its payload"));
                }
            }
        }
        batch.meta = batch.meta.with_raw_policy(raw);
        view.check_boundary()?;
        Ok(batch)
    }).map_err(|error| context.annotate(error,TimeoutPhase::Aggregate,None).with_context("domain","mac").with_context("stage","xdxr-provider"))?;
    context.record_work(batch.events.len(),0)?;
    context.check_boundary(TimeoutPhase::Aggregate)?;
    lease.accept(); Ok(batch)
}

#[derive(Clone, Copy)]
enum Fallback { Unavailable, StillInvalid }
impl Fallback {
    const fn stage(self) -> &'static str { match self { Self::Unavailable => "AdjustmentFallbackUnavailable",Self::StillInvalid => "AdjustmentStillInvalid" } }
    const fn error_kind(self) -> ErrorKind { match self { Self::Unavailable => ErrorKind::MetadataUnavailable,Self::StillInvalid => ErrorKind::Protocol } }
}
fn fallback(owner: &MacOwner, context: &OperationContext, query: &BarsQuery, mut original: Collection<Bar>,
    stages: &[ResultMeta], status: Fallback, message: &'static str, events: Option<Arc<[AdjustmentEvent]>>) -> Result<Collection<Bar>> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    let mut selected = Vec::new(); selected.try_reserve(original.rows.len()).map_err(crate::services::allocation_error)?;
    selected.extend(0..original.rows.len());
    original.meta = crate::services::aggregate_meta(context,stages,None)?.with_raw_row_selection(&selected)?
        .with_diagnostic(Diagnostic::new("mac",status.stage(),EnrichmentStatus::Unavailable,Some(status.error_kind()),Some(message.into())));
    owner.session.record_meta(original.meta.clone())?;
    if query.strict_adjustment() {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        return Err(context.annotate(Error::new(status.error_kind(),message)
            .with_context("domain","mac").with_context("adjustment",status.stage()),TimeoutPhase::Aggregate,Some(0x122e)));
    }
    attach_meta(context,&mut original,events)?;
    Ok(original)
}
fn attach_meta(context: &OperationContext, result: &mut Collection<Bar>, events: Option<Arc<[AdjustmentEvent]>>) -> Result<()> {
    let meta = Arc::new(result.meta.clone());
    for (index,row) in result.rows.iter_mut().enumerate() {
        if index % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        row.set_meta(Arc::clone(&meta));
        if let Some(events) = &events { row.set_adjustment_events(Arc::clone(events)); }
    }
    context.check_boundary(TimeoutPhase::Aggregate)
}
fn bad_prices(context: &OperationContext, bars: &[Bar]) -> Result<bool> {
    for chunk in bars.chunks(256) {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if util::has_bad_prices(chunk) { return Ok(true); }
    }
    context.check_boundary(TimeoutPhase::Aggregate)?; Ok(false)
}
