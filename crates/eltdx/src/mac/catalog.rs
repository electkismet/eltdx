//! Source-pinned directory composition on the existing MAC-EX owner. A bounded
//! scan validates every global row; it does not assume sorted market ranges.
use std::num::NonZeroU16;
use std::sync::Arc;
use crate::{Error, ErrorKind, PageOptions, Result, TimeoutPhase};
use crate::operation::{ChildPurpose, OperationContext};
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta};
use crate::ex::{commands as ex_commands, ExMarketId, InstrumentInfo};
use crate::ex::catalog::{Cache, CatalogKind, CatalogSnapshot, Key};
use crate::ex::session::Lease;
use crate::ex::transport::Work;
use super::{commands, decode, GoodsItem, MacMarketId, PageCompletion};
use super::pagination::{EndPolicy, Pagination, Target};
use super::session::{self, MacOwner, Reply};

pub type InstrumentCatalog = CatalogSnapshot<InstrumentInfo>;
pub type GoodsCatalog = CatalogSnapshot<GoodsItem>;
#[derive(Debug, Default)]
pub(crate) struct Catalogs { instruments: Cache<InstrumentInfo>, goods: Cache<GoodsItem> }
impl Catalogs {
    pub fn clear(&mut self) -> Result<()> { self.instruments.clear()?; self.goods.clear() }
}
/// Counts every child request, including the initial MAC probe, count fences
/// and fallback. The common parent's record/byte gates remain authoritative.
pub(crate) struct Budget { requests: usize, maximum: usize }
impl Budget {
    pub fn new(pages: &PageOptions) -> Self { Self { requests: 0,maximum: pages.max_pages() } }
    pub fn before(&mut self, context: &OperationContext) -> Result<()> {
        context.check_boundary(TimeoutPhase::Admission)?;
        if self.requests == self.maximum { return Err(limit("directory composition reached its total request limit")); }
        self.requests = self.requests.checked_add(1).ok_or_else(|| limit("directory request counter overflow"))?; Ok(())
    }
}

pub(crate) fn global_count(owner: &MacOwner, context: &OperationContext, lease: &Lease, budget: &mut Budget) -> Result<u32> {
    budget.before(context)?;
    let request = ex_commands::instrument_count_request();
    let reply = owner.exchange_leased_with_purpose(context,lease,encoded(request.wire().clone(),None),
        request.command(),ChildPurpose::Page,|body,_| ex_commands::decode_instrument_count(body))?;
    accepted_key(owner,context,ExMarketId::from_wire(0),CatalogKind::ExInstruments,&reply)?;
    Ok(reply.value)
}

fn key(owner: &MacOwner, context: &OperationContext, market: ExMarketId, kind: CatalogKind) -> Result<Option<Key>> {
    Ok(owner.session.connection_identity()?.map(|(endpoint,generation)| Key {
        owner: context.session_id,endpoint,generation,profile: owner.session.profile.name(),market,kind,raw: context.raw(),
        schema: crate::ex::catalog::CATALOG_SCHEMA_VERSION,
    }))
}
fn accepted_key<T>(owner: &MacOwner, context: &OperationContext, market: ExMarketId,
    kind: CatalogKind, reply: &Reply<T>) -> Result<Key> {
    let key = key(owner,context,market,kind)?.ok_or_else(source_changed)?;
    if reply.meta.source().endpoint() != Some(key.endpoint)
        || reply.meta.source().session_id() != format!("{}:{}",key.owner,key.generation) { return Err(source_changed()); }
    Ok(key)
}
fn encoded(wire: bytes::Bytes, source: Option<&Key>) -> Work {
    match source { Some(key) => Work::PinnedEncoded { wire,source: (key.endpoint,key.generation) },None => Work::Encoded(wire) }
}
fn mac_work(owner: &MacOwner, request: commands::MacRequest, source: Option<&Key>) -> Result<Work> {
    match source { Some(key) => owner.pinned_mac_work(request,(key.endpoint,key.generation)),None => owner.mac_work(request) }
}
fn cache_charge<T>(context: &OperationContext, snapshot: &CatalogSnapshot<T>) -> Result<()> {
    context.record_work(snapshot.rows.len(),snapshot.heap_bytes)?; context.check_boundary(TimeoutPhase::Aggregate)
}
fn snapshot_bytes<T>(rows: &Vec<T>, meta: &ResultMeta, extra: impl Fn(&T) -> usize) -> Result<usize> {
    let mut bytes = rows.capacity().checked_mul(std::mem::size_of::<T>()).ok_or_else(|| limit("catalog storage overflows"))?;
    for row in rows { bytes = bytes.checked_add(extra(row)).ok_or_else(|| limit("catalog strings overflow"))?; }
    let mut retained = crate::types::memory::MemoryCharge::default();
    meta.charge_heap(&mut retained)?;
    bytes = bytes.checked_add(retained.total()).ok_or_else(|| limit("catalog provenance overflows"))?;
    Ok(bytes)
}
fn instrument_heap(row: &InstrumentInfo) -> usize { row.id.code.capacity()+row.name.capacity()+row.desc.capacity() }

pub(crate) trait CatalogRow: Clone { fn heap_bytes(&self) -> Result<usize>; }
impl CatalogRow for InstrumentInfo {
    fn heap_bytes(&self) -> Result<usize> {
        let mut charge = crate::types::memory::MemoryCharge::default();
        charge.string(&self.id.code)?; charge.string(&self.name)?; charge.string(&self.desc)?; Ok(charge.total())
    }
}
impl CatalogRow for GoodsItem { fn heap_bytes(&self) -> Result<usize> { GoodsItem::heap_bytes(self) } }

/// Reserve the retained metadata copies before aggregation. Raw buffers may
/// share backing storage; conservatively charging them again keeps this bound
/// independent of Bytes internals. Vector growth gets a separate allowance.
pub(crate) fn merge_meta(context: &OperationContext, metas: &[ResultMeta], version: Option<u64>) -> Result<ResultMeta> {
    let mut charge = crate::types::memory::MemoryCharge::default();
    for meta in metas { context.check_boundary(TimeoutPhase::Aggregate)?; meta.charge_heap(&mut charge)?; }
    context.record_work(0,charge.total().checked_mul(2).ok_or_else(|| limit("directory metadata allocation overflows"))?)?;
    crate::services::aggregate_meta(context,metas,version)
}

pub(crate) fn instruments(owner: &MacOwner, context: &OperationContext, lease: &Lease,
    market: ExMarketId, pages: &PageOptions, budget: &mut Budget, refresh: bool,
    first_purpose: ChildPurpose) -> Result<Arc<InstrumentCatalog>> {
    let initial = key(owner,context,market,CatalogKind::ExInstruments)?;
    if !refresh {
        if let Some(source) = &initial {
            let cached = owner.catalogs.lock().map_err(|_| poisoned())?.instruments.get(source);
            if let Some(cached) = cached { cache_charge(context,&cached)?; return Ok(cached); }
        }
    }
    let (epoch,version) = owner.catalogs.lock().map_err(|_| poisoned())?.instruments.begin_load()?;
    budget.before(context)?;
    let request = ex_commands::instrument_count_request();
    let bound = if first_purpose == ChildPurpose::Page { None } else { initial.as_ref() };
    let count = owner.exchange_leased_with_purpose(context,lease,encoded(request.wire().clone(),bound),
        request.command(),first_purpose,|body,_| ex_commands::decode_instrument_count(body))?;
    let source = accepted_key(owner,context,market,CatalogKind::ExInstruments,&count)?;
    let total = count.value;
    if u64::from(total) > pages.max_records() as u64 { return Err(limit("global directory count exceeds the bounded scan record limit")); }
    let mut metas = vec![count.meta]; let mut rows = Vec::new(); let mut selected = Vec::new();
    let mut offset = 0_u32; let mut previous = None; let mut sorted = true;
    let mut first = None; let mut last = None; let mut insertion = None;
    while offset < total {
        budget.before(context)?;
        let requested = (total-offset).min(u32::try_from(pages.page_size()).map_err(|_| limit("directory page size exceeds u32"))?).min(1000);
        let query = crate::ex::InstrumentsQuery::new(offset).with_count(requested)?;
        let request = ex_commands::instruments_request(&query); let sponsor = context.sponsor();
        let reply = owner.exchange_leased_with_purpose(context,lease,encoded(request.wire().clone(),Some(&source)),
            request.command(),ChildPurpose::Page,move |body,_| {
                let prefix = body.get(..6).ok_or_else(|| protocol("EX directory page lacks its prefix"))?;
                let observed_offset = u32::from_le_bytes(prefix[..4].try_into().map_err(|_| protocol("invalid EX directory offset"))?);
                let observed_count = u16::from_le_bytes([prefix[4],prefix[5]]);
                if observed_offset != offset || u32::from(observed_count)>requested {
                    return Err(protocol("EX directory page offset/count differs from its request"));
                }
                let charge = usize::from(observed_count).checked_mul(std::mem::size_of::<InstrumentInfo>()+35*6
                    +std::mem::size_of::<(usize,usize)>()+3*(std::mem::size_of::<Diagnostic>()+256))
                    .ok_or_else(|| limit("directory decode allocation overflows"))?;
                sponsor.record_bytes(charge)?;
                let decoded = ex_commands::decode_instruments(body)?;
                if decoded.response_offset() != Some(offset) || decoded.wire_row_count().is_none_or(|count| u32::from(count)>requested) {
                    return Err(protocol("EX directory page offset/count differs from its request"));
                }
                Ok(decoded)
            })?;
        let observed = accepted_key(owner,context,market,CatalogKind::ExInstruments,&reply)?;
        if observed != source { return Err(source_changed()); }
        let consumed = u32::try_from(reply.value.len()).map_err(|_| limit("directory page length exceeds u32"))?;
        if consumed == 0 { return Err(protocol("EX directory ended before its same-source advertised count")); }
        for (index,row) in reply.value.into_iter().enumerate() {
            if index%256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
            let id = row.market().get();
            if previous.is_some_and(|previous| previous>id) { sorted = false; } previous = Some(id);
            let global = offset.checked_add(u32::try_from(index).map_err(|_| limit("directory index exceeds u32"))?)
                .ok_or_else(|| limit("global directory index overflow"))?;
            if id >= market.get() { insertion.get_or_insert(global); }
            if row.market() == market {
                first.get_or_insert(global); last = Some(global+1);
                context.record_work(0,2*(std::mem::size_of::<InstrumentInfo>()+std::mem::size_of::<usize>()))?;
                rows.try_reserve(1).map_err(crate::services::allocation_error)?;
                selected.try_reserve(1).map_err(crate::services::allocation_error)?;
                selected.push(global as usize); rows.push(row);
            }
        }
        offset = offset.checked_add(consumed).ok_or_else(|| limit("global directory offset overflows"))?;
        metas.try_reserve(1).map_err(crate::services::allocation_error)?; metas.push(reply.meta);
    }
    // A second count detects growth/shrinkage during the bounded scan. This is
    // source consistency evidence, not a claim that the server offers MVCC.
    budget.before(context)?; let request = ex_commands::instrument_count_request();
    let fence = owner.exchange_leased_with_purpose(context,lease,encoded(request.wire().clone(),Some(&source)),
        request.command(),ChildPurpose::Page,|body,_| ex_commands::decode_instrument_count(body))?;
    if fence.value != total { return Err(protocol("EX directory count changed during its snapshot scan").with_context("reason","DirectoryChanged")); }
    metas.push(fence.meta);
    let mut meta = merge_meta(context,&metas,Some(version))?.with_raw_row_selection(&selected)?
        .with_diagnostic(diag("BoundedDirectoryScan",EnrichmentStatus::Succeeded,None,
            format!("global_count={total}; matching={}; verified_sorted={sorted}; count rechecked; no server atomic snapshot is asserted",rows.len())));
    if !sorted { meta = meta.with_diagnostic(diag("DirectoryUnsorted",EnrichmentStatus::Unavailable,None,
        "all global rows were scanned; matching market occurrences retained in source order".into())); }
    let sorted_bounds = if sorted {
        let absent = insertion.unwrap_or(total);
        Some((first.unwrap_or(absent),last.unwrap_or(absent)))
    } else { None };
    let heap_bytes = snapshot_bytes(&rows,&meta,instrument_heap)?;
    context.record_work(0,heap_bytes)?;
    let result = Arc::new(CatalogSnapshot { key: source,version,rows,meta,global_count: Some(total),sorted_bounds,heap_bytes });
    context.check_boundary(TimeoutPhase::Aggregate)?;
    owner.catalogs.lock().map_err(|_| poisoned())?.instruments.publish(epoch,Arc::clone(&result))?;
    owner.session.record_meta(result.meta.clone())?; Ok(result)
}
pub(crate) fn goods_snapshot(owner: &MacOwner, context: &OperationContext, lease: &Lease,
    market: ExMarketId, pages: &PageOptions, budget: &mut Budget, refresh: bool) -> Result<Arc<GoodsCatalog>> {
    let initial = key(owner,context,market,CatalogKind::MacGoods)?;
    if !refresh {
        if let Some(source) = &initial {
            let cached = owner.catalogs.lock().map_err(|_| poisoned())?.goods.get(source);
            if let Some(cached) = cached { cache_charge(context,&cached)?; return Ok(cached); }
        }
    }
    let (epoch,version) = owner.catalogs.lock().map_err(|_| poisoned())?.goods.begin_load()?;
    let mut progress = Pagination::new(0,Target::All,pages.clone(),NonZeroU16::new(1000).unwrap(),u32::MAX,EndPolicy::EmptyOrShort)?;
    let mut rows = Vec::new(); let mut metas = Vec::new(); let mut source = None;
    while let Some(page) = progress.next_request()? {
        budget.before(context)?;
        let request = commands::goods_request(MacMarketId::new(u16::from(market.get())),page.offset(),u32::from(page.count()))?;
        let requested = page.count(); let sponsor = context.sponsor();
        let reply = owner.exchange_leased_with_purpose(context,lease,mac_work(owner,request,source.as_ref())?,0x2562,
            ChildPurpose::Page,move |body,_| {
                let prefix = body.get(..2).ok_or_else(|| protocol("MAC goods page lacks its count"))?;
                let observed = u16::from_le_bytes([prefix[0],prefix[1]]);
                if observed>requested { return Err(protocol("MAC goods page exceeds its requested count")); }
                sponsor.record_bytes(usize::from(observed)*(std::mem::size_of::<GoodsItem>()+23*6+std::mem::size_of::<(usize,usize)>()))?;
                session::rows(decode::goods(&body)?,requested,2,48)
            })?;
        let observed = accepted_key(owner,context,market,CatalogKind::MacGoods,&reply)?;
        if source.as_ref().is_some_and(|key| *key != observed) { return Err(source_changed()); } source = Some(observed);
        progress.accept(reply.value.len())?;
        context.record_work(0,reply.value.len().checked_mul(2*std::mem::size_of::<GoodsItem>()).ok_or_else(|| limit("goods collection allocation overflows"))?)?;
        rows.try_reserve(reply.value.len()).map_err(crate::services::allocation_error)?; rows.extend(reply.value);
        metas.try_reserve(1).map_err(crate::services::allocation_error)?; metas.push(reply.meta);
    }
    if rows.is_empty() {
        // Empty is evidence for trying the supported EX directory, not proof
        // that this MAC command is unavailable for every market on this host.
        let catalog = instruments(owner,context,lease,market,pages,budget,refresh,ChildPurpose::Fallback)?;
        let leading = merge_meta(context,&metas,None)?;
        metas = vec![leading,catalog.meta.clone()];
        for (index,info) in catalog.rows.iter().enumerate() {
            if index%256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
            context.record_work(0,2*std::mem::size_of::<GoodsItem>()+info.heap_bytes()?+info.code().len()*6)?;
            rows.try_reserve(1).map_err(crate::services::allocation_error)?;
            rows.push(GoodsItem::from_instrument(info,u32::try_from(index).map_err(|_| limit("derived goods index exceeds u32"))?));
        }
    }
    let source = source.ok_or_else(source_changed)?;
    let mut meta = merge_meta(context,&metas,Some(version))?;
    if rows.first().is_some_and(|row| row.source_instrument().is_some()) || rows.is_empty() {
        meta = meta.with_diagnostic(diag("EmptyGoodsExFallback",EnrichmentStatus::Succeeded,None,
            "empty MAC goods page; EX directory retains actual code/name/desc; placeholders are derived".into()));
    }
    let mut heap_bytes = rows.capacity().checked_mul(std::mem::size_of::<GoodsItem>()).ok_or_else(|| limit("goods snapshot allocation overflows"))?;
    for row in &rows { heap_bytes = heap_bytes.checked_add(row.heap_bytes()?).ok_or_else(|| limit("goods snapshot strings overflow"))?; }
    let mut retained = crate::types::memory::MemoryCharge::default(); meta.charge_heap(&mut retained)?;
    heap_bytes = heap_bytes.checked_add(retained.total()).ok_or_else(|| limit("goods snapshot metadata overflows"))?;
    context.record_work(0,heap_bytes)?;
    let result = Arc::new(CatalogSnapshot { key: source,version,rows,meta,global_count: None,sorted_bounds: None,heap_bytes });
    context.check_boundary(TimeoutPhase::Aggregate)?;
    owner.catalogs.lock().map_err(|_| poisoned())?.goods.publish(epoch,Arc::clone(&result))?;
    owner.session.record_meta(result.meta.clone())?; Ok(result)
}

pub(crate) fn page<T: CatalogRow>(context: &OperationContext, snapshot: &CatalogSnapshot<T>, start: u32, count: u32) -> Result<super::PageResult<T>> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    let start_index = usize::try_from(start).map_err(|_| limit("market starting offset exceeds platform capacity"))?.min(snapshot.rows.len());
    let end = start_index.saturating_add(count as usize).min(snapshot.rows.len());
    let mut charge = crate::types::memory::MemoryCharge::default();
    charge.array::<T>(end-start_index)?; charge.array::<usize>(end-start_index)?;
    for (index,row) in snapshot.rows[start_index..end].iter().enumerate() {
        if index%256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        charge.add(row.heap_bytes()?)?;
    }
    snapshot.meta.charge_heap(&mut charge)?;
    charge.array::<(usize,usize,usize)>(end-start_index)?; charge.array::<bool>(end-start_index)?;
    context.record_work(0,charge.total())?;
    let mut selected = Vec::new(); let mut rows = Vec::new();
    selected.try_reserve_exact(end-start_index).map_err(crate::services::allocation_error)?;
    rows.try_reserve_exact(end-start_index).map_err(crate::services::allocation_error)?;
    for index in start_index..end {
        if (index-start_index)%256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        selected.push(index); rows.push(snapshot.rows[index].clone());
    }
    let meta = snapshot.meta.clone().with_raw_row_selection(&selected)?;
    let consumed = u32::try_from(rows.len()).map_err(|_| limit("market page length exceeds u32"))?;
    let next = start.checked_add(consumed).ok_or_else(|| limit("market continuation overflows"))?;
    let completion = if end == snapshot.rows.len() { PageCompletion::EndOfData } else { PageCompletion::RequestedCount };
    super::PageResult::new(rows,Some(snapshot.rows.len() as u64),Some(next),completion,meta)
}
fn diag(stage: &'static str, status: EnrichmentStatus, error: Option<ErrorKind>, message: String) -> Diagnostic {
    Diagnostic::new("mac-ex-catalog",stage,status,error,Some(message))
}
fn limit(message: &'static str) -> Error { Error::new(ErrorKind::LimitExceeded,message).with_context("domain","mac-ex-catalog") }
fn protocol(message: &'static str) -> Error { Error::new(ErrorKind::Protocol,message).with_context("domain","mac-ex-catalog") }
fn source_changed() -> Error { protocol("directory source changed during its pinned snapshot").with_context("reason","SourceChanged") }
fn poisoned() -> Error { Error::new(ErrorKind::Internal,"MAC-EX catalog cache lock poisoned") }
