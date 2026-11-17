//! Page orchestration on the existing owner. Frame decoding and over-response
//! rejection occur inside its worker before a socket is reusable.
use std::num::NonZeroU16;
use std::sync::Arc;
use crate::{Error, ErrorKind, PageOptions, RawPolicy, Result, TimeoutPhase, TradingDate};
use crate::operation::{ChildPurpose, OperationContext};
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta};
use crate::ex::transport::Work;
use super::{commands, decode, Bar, BoardInfo, FieldSelection, MacInstrumentId, PageCompletion, PageResult, Quote, Transaction};
use super::enums::{FilterMask, SortOrder, SortType};
use super::models::FieldContext;
use super::pagination::{EndPolicy, MergeOrder, PageRequest, PageRows, Pagination, Target};
use super::query::{BarsQuery, BoardsQuery, TransactionsQuery};
use super::session::{self, MacOwner, Reply};

pub(crate) trait RowMeta {
    fn attach(&mut self, meta: Arc<ResultMeta>);
}
macro_rules! row_meta { ($($name:ty),+ $(,)?) => { $(
    impl RowMeta for $name { fn attach(&mut self, meta: Arc<ResultMeta>) { self.set_meta(meta); } }
)+ }; }
row_meta!(Quote, Bar, Transaction, BoardInfo, super::Unusual);

pub(crate) struct Collection<T> {
    pub rows: Vec<T>, pub meta: ResultMeta, pub next: u32,
    pub completion: PageCompletion, pub total: Option<u64>,
}
impl<T> Collection<T> {
    pub fn page(self) -> Result<PageResult<T>> {
        PageResult::new(self.rows,self.total,Some(self.next),self.completion,self.meta)
    }
}
/// Metadata remains in response order. Only raw row references and whole data
/// pages are reordered for bars; duplicate rows within each page survive.
fn collect<T: RowMeta>(owner: &MacOwner, context: &OperationContext, mut progress: Pagination,
    order: MergeOrder, mut fetch: impl FnMut(PageRequest) -> Result<Reply<(Option<u64>,Vec<T>)>>) -> Result<Collection<T>> {
    let mut output = PageRows::new(order); let mut row_order = PageRows::new(order);
    let mut metas = Vec::new(); let mut total = None; let mut total_seen = false; let mut total_changed = false;
    while let Some(request) = progress.next_request()? {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let reply = fetch(request)?;
        let wire_rows = reply.wire_count.ok_or_else(|| protocol("MAC page has no decoded wire count"))?;
        if usize::from(wire_rows) != reply.value.1.len() { return Err(protocol("MAC result projection changed wire occurrence count")); }
        let first = progress.rows(); progress.accept(usize::from(wire_rows))?;
        let (observed_total,mut rows) = reply.value;
        if total_seen && total != observed_total { total_changed = true; }
        if !total_seen { total = observed_total; total_seen = true; }
        let shared_meta = Arc::new(reply.meta);
        for row in &mut rows { row.attach(Arc::clone(&shared_meta)); }
        row_order.push((first..progress.rows()).collect());
        output.push(rows); metas.push(shared_meta);
        context.check_boundary(TimeoutPhase::Aggregate)?;
    }
    let completion = progress.completion().ok_or_else(|| protocol("MAC collection lacks terminal evidence"))?;
    let pages: Vec<_> = metas.iter().map(|meta| meta.as_ref().clone()).collect();
    let mut meta = crate::services::aggregate_meta(context,&pages,None)?;
    if order == MergeOrder::PrependPages { meta = meta.with_raw_row_order(&row_order.into_rows())?; }
    if total_changed {
        total = None;
        meta = meta.with_diagnostic(Diagnostic::new("mac","server-total",EnrichmentStatus::Unavailable,None,
            Some("server total changed between response pages; no single total is asserted".into())));
    }
    context.check_boundary(TimeoutPhase::Aggregate)?;
    owner.session.record_meta(meta.clone())?;
    Ok(Collection { rows: output.into_rows(),meta,next: progress.next_offset(),completion,total })
}

pub(crate) struct ListPlan<'a> {
    pub code: u32, pub start: u32, pub target: Target, pub fields: &'a FieldSelection,
    pub sort: SortType, pub order: SortOrder, pub exclude: FilterMask,
    pub pages: PageOptions, pub field_context: FieldContext,
}
pub(crate) fn quote_list(owner: &MacOwner, context: &OperationContext, plan: ListPlan<'_>) -> Result<Collection<Quote>> {
    let progress = Pagination::new(plan.start,plan.target,plan.pages,maximum(80),u32::MAX,EndPolicy::EmptyOrShort)?;
    collect(owner,context,progress,MergeOrder::Append,|request| {
        let command = commands::board_members_request(plan.code,plan.sort.wire(),request.offset(),request.count(),
            plan.order.wire() as u8,plan.fields,plan.exclude.bits())?;
        let requested = request.count(); let field_context = plan.field_context;
        owner.exchange_with_purpose(context,owner.mac_work(command)?,0x122c,ChildPurpose::Page,move |body,_| {
            let (total,rows,bitmap) = commands::decode_quote_rows(&body,field_context)?;
            let values = rows.into_iter().map(Quote::from_row).collect::<Result<Vec<_>>>()?;
            session::rows(values,requested,26,super::bitmap::quote_row_width(&bitmap)?)
                .map(|decoded| decoded.map(|rows| (Some(u64::from(total)),rows)))
        })
    })
}

pub(crate) fn bars(owner: &MacOwner, context: &OperationContext, query: &BarsQuery) -> Result<Collection<Bar>> {
    bars_with_purpose(owner,context,query,ChildPurpose::Page)
}
pub(crate) fn bars_with_purpose(owner: &MacOwner, context: &OperationContext, query: &BarsQuery,
    first_purpose: ChildPurpose) -> Result<Collection<Bar>> {
    bars_with_source(owner,context,query,first_purpose,None)
}
pub(crate) fn bars_with_source(owner: &MacOwner, context: &OperationContext, query: &BarsQuery,
    first_purpose: ChildPurpose, source: Option<(super::Endpoint,u64)>) -> Result<Collection<Bar>> {
    let progress = Pagination::new(query.start(),Target::Count(query.count()),query.pages().clone(),maximum(700),u32::MAX,EndPolicy::EmptyOrShort)?;
    let mut first = true;
    collect(owner,context,progress,MergeOrder::PrependPages,|request| {
        let purpose = if first { first_purpose } else { ChildPurpose::Page }; first = false;
        bars_page_with_source(owner,context,query,request.offset(),request.count(),purpose,source)
            .map(|reply| Reply { value: (None,reply.value),meta: reply.meta,wire_count: reply.wire_count,response_offset: reply.response_offset })
    })
}
/// The one complete MAC bar decode/projection used by ordinary collections,
/// QFQ refetches and board history windows. No composite copies its layout.
pub(crate) fn bars_page(owner: &MacOwner, context: &OperationContext, query: &BarsQuery,
    offset: u32, requested: u16, purpose: ChildPurpose) -> Result<Reply<Vec<Bar>>> {
    bars_page_with_source(owner,context,query,offset,requested,purpose,None)
}
fn bars_page_with_source(owner: &MacOwner, context: &OperationContext, query: &BarsQuery,
    offset: u32, requested: u16, purpose: ChildPurpose, source: Option<(super::Endpoint,u64)>) -> Result<Reply<Vec<Bar>>> {
    let command = commands::bars_request(query.id(),query.period(),query.times().get(),offset,requested,query.adjust())?;
    let work = match source { Some(source) => owner.pinned_mac_work(command,source)?,None => owner.mac_work(command)? };
    let query = query.clone();
    owner.exchange_with_purpose(context,work,0x122e,purpose,move |body,raw| {
        let records = commands::decode_bars(&body,query.period())?;
        let values = records.into_iter().enumerate().map(|(index,value)| {
            let offset = 33+index*36;
            Bar::from_record(value,&query,(raw != RawPolicy::Omit).then(|| body.slice(offset..offset+36)))
        }).collect::<Result<Vec<_>>>()?;
        let mut decoded = session::rows(values,requested,33,36)?;
        if !query.period().has_known_semantics() { decoded.diagnostics.push(Diagnostic::new("mac","bar-time",EnrichmentStatus::Unavailable,None,
            Some("explicit unknown wire period retains raw date/time without assigning calendar semantics".into()))); }
        Ok(decoded)
    })
}

pub(crate) fn boards(owner: &MacOwner, context: &OperationContext, query: &BoardsQuery, target: Target) -> Result<Collection<BoardInfo>> {
    // The source's first short page still permits another request. Require an
    // empty page for an all-boards scan rather than overstate that observation.
    let progress = Pagination::new(0,target,query.pages().clone(),maximum(150),u32::from(u16::MAX),EndPolicy::EmptyOnly)?;
    collect(owner,context,progress,MergeOrder::Append,|request| {
        let offset = u16::try_from(request.offset()).map_err(|_| super::query::invalid("MAC board start exceeds u16"))?;
        let command = commands::board_list_request(query.board_type().wire(),offset,request.count())?;
        let requested = request.count();
        owner.exchange_with_purpose(context,owner.mac_work(command)?,0x1231,ChildPurpose::Page,move |body,_| {
            let page = decode::boards(&body)?;
            // total_raw is retained by the pure BoardPage codec. Its relation
            // to paired_count is not established as a global board total.
            session::rows(page.into_items(),requested,4,160).map(|decoded| decoded.map(|rows| (None,rows)))
        })
    })
}

pub(crate) fn transactions(owner: &MacOwner, context: &OperationContext, query: &TransactionsQuery) -> Result<Collection<Transaction>> {
    let progress = Pagination::new(query.start(),Target::Count(query.count()),query.pages().clone(),maximum(1000),u32::MAX,EndPolicy::EmptyOrShort)?;
    mac_transactions(owner,context,query.id(),query.date(),progress)
}
fn mac_transactions(owner: &MacOwner, context: &OperationContext, id: &MacInstrumentId,
    date: Option<TradingDate>, progress: Pagination) -> Result<Collection<Transaction>> {
    collect(owner,context,progress,MergeOrder::Append,|request| {
        let command = commands::transactions_request(id,super::query::date_wire(date)?,request.offset(),request.count())?;
        let id = id.clone(); let requested = request.count();
        owner.exchange(context,owner.mac_work(command)?,0x122f,move |body,raw| {
            let slots = commands::decode_transactions(&body)?;
            let rows = slots.into_iter().enumerate().map(|(index,value)| {
                let offset = 39+index*18;
                Transaction::from_slot(&id,value,(raw != RawPolicy::Omit).then(|| body.slice(offset..offset+18)))
            }).collect::<Result<Vec<_>>>()?;
            session::rows(rows,requested,39,18).map(|decoded| decoded.map(|rows| (None,rows)))
        })
    })
}
pub(crate) struct ExTransactionPlan<'a> {
    pub id: &'a crate::ex::ExInstrumentId, pub date: Option<TradingDate>, pub start: u32,
    pub target: Target, pub pages: PageOptions,
}
pub(crate) fn ex_transactions(owner: &MacOwner, context: &OperationContext, plan: ExTransactionPlan<'_>) -> Result<Collection<Transaction>> {
    let id = MacInstrumentId::try_from(plan.id)?;
    let hong_kong = matches!(plan.id.market().get(),27|31|48|49|71|98);
    let max = if hong_kong { 1800 } else { 1000 };
    let progress = Pagination::new(plan.start,plan.target,plan.pages,maximum(max),
        if hong_kong { i32::MAX as u32 } else { u32::MAX },EndPolicy::EmptyOrShort)?;
    if !hong_kong { return mac_transactions(owner,context,&id,plan.date,progress); }
    collect(owner,context,progress,MergeOrder::Append,|request| {
        let (work,command) = if let Some(date) = plan.date {
            let query = crate::ex::HistoryTransactionsQuery::new(plan.id.clone(),date)?
                .with_start(request.offset())?.with_count(u32::from(request.count()))?;
            (Work::Request(crate::ex::commands::history_transactions_request(&query)),0x2406)
        } else {
            let query = crate::ex::TransactionsQuery::new(plan.id.clone())
                .with_start(request.offset())?.with_count(u32::from(request.count()))?;
            (Work::Request(crate::ex::commands::transactions_request(&query)),0x23fc)
        };
        let id = id.clone(); let requested = request.count();
        // Work::Request keeps ordinary EX bytes intact, while the immutable
        // session still performs MAC-EX login on every replacement connection.
        owner.exchange(context,work,command,move |body,raw| {
            let decoded = crate::ex::commands::decode_transactions(body,raw)?;
            if decoded.value().len() > usize::from(requested) { return Err(protocol("HK response exceeds requested wire page")); }
            decoded.try_map(|rows| rows.into_iter().map(|value| Transaction::from_ex(&id,value)).collect::<Result<Vec<_>>>())
                .map(|decoded| decoded.map(|rows| (None,rows)))
        })
    })
}
fn maximum(value: u16) -> NonZeroU16 { NonZeroU16::new(value).expect("positive fixed MAC page maximum") }
fn protocol(message: &'static str) -> Error { Error::new(ErrorKind::Protocol,message).with_context("domain","mac") }
