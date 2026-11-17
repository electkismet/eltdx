//! MAC-EX catalog consumers. All requests retain the caller's parent operation
//! and the source generation that supplied the resolved directory identity.
use std::collections::HashSet;
use std::sync::Arc;
use crate::{Error, ErrorKind, PageOptions, Result, TimeoutPhase};
use crate::operation::OperationContext;
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta};
use crate::ex::{ExCode, ExInstrumentId, ExMarketId};
use crate::ex::session::Lease;
use super::{Bar, GoodsCatalog, MacInstrumentId, PageCompletion, PageResult, Quote};
use super::catalog::{self, Budget};
use super::ex_query::{MacroCode, MacroQuotesQuery, MacroBarsQuery, QuoteListQuery};
use super::session::MacOwner;

impl super::MacExClient {
    pub fn goods_count(&self, query: &super::ex_query::GoodsCountQuery, options: &crate::OperationOptions) -> Result<u32> {
        let pages = query.pages(); let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.goods_count",options)?;
        limits(&mut context,&pages,self.config().max_operation_bytes().get())?;
        let lease = self.inner.session.lease(&context)?; let mut budget = Budget::new(&pages);
        let count = if let Some(market) = query.market() {
            let snapshot = catalog::goods_snapshot(&self.inner,&context,&lease,market,&pages,&mut budget,false)?;
            self.inner.session.record_meta(snapshot.meta().clone())?;
            u32::try_from(snapshot.rows().len()).map_err(|_| limit("market goods count exceeds u32"))?
        } else { catalog::global_count(&self.inner,&context,&lease,&mut budget)? };
        context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept(); context.finish()?; Ok(count)
    }
    pub fn goods(&self, query: &super::ex_query::GoodsQuery, options: &crate::OperationOptions) -> Result<PageResult<super::GoodsItem>> {
        let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.goods",options)?;
        limits(&mut context,query.pages(),self.config().max_operation_bytes().get())?;
        let lease = self.inner.session.lease(&context)?; let mut budget = Budget::new(query.pages());
        let snapshot = catalog::goods_snapshot(&self.inner,&context,&lease,query.market(),query.pages(),&mut budget,false)?;
        let page = catalog::page(&context,&snapshot,query.start(),query.count().get())?;
        self.inner.session.record_meta(page.meta().clone())?;
        context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept(); context.finish()?; Ok(page)
    }
    pub fn instruments(&self, query: &super::ex_query::InstrumentsQuery, options: &crate::OperationOptions) -> Result<PageResult<crate::ex::InstrumentInfo>> {
        let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.instruments",options)?;
        limits(&mut context,query.pages(),self.config().max_operation_bytes().get())?;
        let lease = self.inner.session.lease(&context)?; let mut budget = Budget::new(query.pages());
        let snapshot = catalog::instruments(&self.inner,&context,&lease,query.market(),query.pages(),&mut budget,false,crate::operation::ChildPurpose::Page)?;
        let page = catalog::page(&context,&snapshot,query.start(),query.count().get())?;
        self.inner.session.record_meta(page.meta().clone())?;
        context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept(); context.finish()?; Ok(page)
    }
    pub fn macro_indicators(&self, options: &crate::OperationOptions) -> Result<Vec<super::GoodsItem>> {
        let pages = default_pages(); let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.macro_indicators",options)?;
        limits(&mut context,&pages,self.config().max_operation_bytes().get())?;
        let lease = self.inner.session.lease(&context)?; let mut budget = Budget::new(&pages);
        let snapshot = catalog::goods_snapshot(&self.inner,&context,&lease,ExMarketId::from_wire(38),&pages,&mut budget,false)?;
        let page = catalog::page(&context,&snapshot,0,u32::MAX)?;
        self.inner.session.record_meta(page.meta().clone())?;
        let rows = page.into_rows(); context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept(); context.finish()?; Ok(rows)
    }
    pub fn macro_quotes(&self, query: &MacroQuotesQuery, options: &crate::OperationOptions) -> Result<Vec<Quote>> {
        let pages = default_pages(); let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.macro_quotes",options)?;
        limits(&mut context,&pages,self.config().max_operation_bytes().get())?;
        let rows = macro_quotes(&self.inner,&context,query,&pages)?; context.finish()?; Ok(rows)
    }
    pub fn macro_bars(&self, query: &MacroBarsQuery, options: &crate::OperationOptions) -> Result<Vec<Bar>> {
        let pages = default_pages(); let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.macro_bars",options)?;
        limits(&mut context,&pages,self.config().max_operation_bytes().get())?;
        let rows = macro_bars(&self.inner,&context,query,&pages,self.config().xdxr_provider.clone())?;
        context.finish()?; Ok(rows)
    }
    pub fn quote_list(&self, query: &QuoteListQuery, options: &crate::OperationOptions) -> Result<PageResult<Quote>> {
        let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.quote_list",options)?;
        limits(&mut context,query.pages(),self.config().max_operation_bytes().get())?;
        let page = quote_list(&self.inner,&context,query)?; context.finish()?; Ok(page)
    }
    /// Immutable handles retain their original records and provenance when a
    /// later refresh/clear/source replacement publishes another snapshot.
    pub fn instrument_catalog(&self, market: ExMarketId, pages: &PageOptions, options: &crate::OperationOptions) -> Result<Arc<super::InstrumentCatalog>> {
        self.instrument_catalog_entry(market,pages,options,false)
    }
    pub fn refresh_instrument_catalog(&self, market: ExMarketId, pages: &PageOptions, options: &crate::OperationOptions) -> Result<Arc<super::InstrumentCatalog>> {
        self.instrument_catalog_entry(market,pages,options,true)
    }
    fn instrument_catalog_entry(&self, market: ExMarketId, pages: &PageOptions, options: &crate::OperationOptions,
        refresh: bool) -> Result<Arc<super::InstrumentCatalog>> {
        // Reuse the exact directory query validator for the public policy.
        super::ex_query::InstrumentsQuery::new(market).with_pages(pages.clone())?;
        let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.instrument_catalog",options)?;
        limits(&mut context,pages,self.config().max_operation_bytes().get())?;
        if refresh { self.inner.catalogs.lock().map_err(|_| Error::new(ErrorKind::Internal,"MAC-EX catalog lock poisoned"))?.clear()?; }
        let lease = self.inner.session.lease(&context)?; let mut budget = Budget::new(pages);
        let snapshot = catalog::instruments(&self.inner,&context,&lease,market,pages,&mut budget,refresh,crate::operation::ChildPurpose::Page)?;
        self.inner.session.record_meta(snapshot.meta().clone())?;
        context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept(); context.finish()?; Ok(snapshot)
    }
    pub fn goods_catalog(&self, market: ExMarketId, pages: &PageOptions, options: &crate::OperationOptions) -> Result<Arc<GoodsCatalog>> {
        self.goods_catalog_entry(market,pages,options,false)
    }
    pub fn refresh_goods_catalog(&self, market: ExMarketId, pages: &PageOptions, options: &crate::OperationOptions) -> Result<Arc<GoodsCatalog>> {
        self.goods_catalog_entry(market,pages,options,true)
    }
    fn goods_catalog_entry(&self, market: ExMarketId, pages: &PageOptions, options: &crate::OperationOptions,
        refresh: bool) -> Result<Arc<GoodsCatalog>> {
        super::ex_query::GoodsQuery::new(market).with_pages(pages.clone())?;
        let mut context = self.inner.context(std::time::Instant::now(),"mac-ex.goods_catalog",options)?;
        limits(&mut context,pages,self.config().max_operation_bytes().get())?;
        if refresh { self.inner.catalogs.lock().map_err(|_| Error::new(ErrorKind::Internal,"MAC-EX catalog lock poisoned"))?.clear()?; }
        let lease = self.inner.session.lease(&context)?; let mut budget = Budget::new(pages);
        let snapshot = catalog::goods_snapshot(&self.inner,&context,&lease,market,pages,&mut budget,refresh)?;
        self.inner.session.record_meta(snapshot.meta().clone())?;
        context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept(); context.finish()?; Ok(snapshot)
    }
    /// Invalidate lookup eligibility. Existing Arc handles remain unchanged;
    /// old in-flight loaders cannot republish into this new cache epoch.
    pub fn clear_catalog_cache(&self, options: &crate::OperationOptions) -> Result<()> {
        let context = self.inner.context(std::time::Instant::now(),"mac-ex.clear_catalog_cache",options)?;
        self.inner.catalogs.lock().map_err(|_| Error::new(ErrorKind::Internal,"MAC-EX catalog lock poisoned"))?.clear()?;
        context.finish()
    }
}

pub(crate) fn default_pages() -> PageOptions { super::query::page_defaults(600,2000,1_200_000) }
pub(crate) fn limits(context: &mut OperationContext, pages: &PageOptions, bytes: usize) -> Result<()> {
    context.set_request_limit(std::num::NonZeroUsize::new(pages.max_pages()).ok_or_else(|| limit("composition requires a positive request limit"))?)?;
    context.set_work_limits(crate::operation::WorkLimits { pages: std::num::NonZeroUsize::new(pages.max_pages()),
        records: Some(pages.max_records()),bytes: Some(bytes) })
}
fn resolve(context: &OperationContext, selector: &MacroCode, catalog: &GoodsCatalog) -> Result<ExInstrumentId> {
    let text = selector.as_str(); let mut matches = Vec::new(); let mut codes = HashSet::new();
    for (index,row) in catalog.rows().iter().enumerate() {
        if index%256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        let code = row.actual_code();
        let suffix = code.split_once('_').map(|(_,suffix)| suffix);
        let matched = if selector.is_full_code() { code == text }
            else { suffix == Some(text) || row.display_name() == Some(text) };
        if matched && !codes.contains(code) {
            context.record_work(0,code.len()*2+4*std::mem::size_of::<String>()+4*std::mem::size_of::<&str>())?;
            codes.try_reserve(1).map_err(crate::services::allocation_error)?;
            codes.insert(code);
            matches.try_reserve(1).map_err(crate::services::allocation_error)?; matches.push(code.to_owned());
        }
    }
    match matches.as_slice() {
        [code] => Ok(ExInstrumentId::new(ExMarketId::from_wire(38),ExCode::new(code.clone())?)),
        [] => Err(Error::new(ErrorKind::MissingData,"unknown exact macro indicator selector")
            .with_context("selector",text).with_context("market","38")),
        _ => Err(Error::new(ErrorKind::InvalidArgument,"ambiguous exact macro indicator selector")
            .with_context("selector",text).with_context("candidates",matches.join(", "))),
    }
}
fn resolve_all(owner: &MacOwner, context: &OperationContext, lease: &Lease,
    selectors: &[MacroCode], pages: &PageOptions, budget: &mut Budget) -> Result<(Vec<ExInstrumentId>,Arc<GoodsCatalog>,ResultMeta)> {
    let catalog = catalog::goods_snapshot(owner,context,lease,ExMarketId::from_wire(38),pages,budget,false)?;
    context.record_work(0,selectors.len().checked_mul(std::mem::size_of::<ExInstrumentId>()).ok_or_else(|| limit("resolved macro IDs overflow"))?)?;
    let mut ids = Vec::new(); ids.try_reserve_exact(selectors.len()).map_err(crate::services::allocation_error)?;
    // EX fallback items carry both real names and descriptions. If native goods
    // supplies only codes, an unresolved exact name can use the same-source EX
    // directory. Invalid/ambiguous native matches are never silently replaced.
    let mut names = None;
    for selector in selectors {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        match resolve(context,selector,&catalog) {
            Ok(id) => ids.push(id),
            Err(error) if error.kind() == ErrorKind::MissingData && !selector.is_full_code()
                && catalog.rows().iter().any(|row| row.source_instrument().is_none()) => {
                if names.is_none() {
                    names = Some(catalog::instruments(owner,context,lease,ExMarketId::from_wire(38),pages,budget,false,
                        crate::operation::ChildPurpose::Metadata)?);
                }
                let instruments = names.as_ref().expect("installed exact-name directory");
                let mut candidates = Vec::new(); let mut seen = HashSet::new();
                for (index,row) in instruments.rows().iter().enumerate() {
                    if index%256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
                    if (row.name() == selector.as_str() || row.code().split_once('_').is_some_and(|(_,suffix)| suffix == selector.as_str()))
                        && !seen.contains(row.code()) {
                        context.record_work(0,8*std::mem::size_of::<&str>()+row.code().len()*2)?;
                        seen.try_reserve(1).map_err(crate::services::allocation_error)?;
                        candidates.try_reserve(1).map_err(crate::services::allocation_error)?;
                        seen.insert(row.code()); candidates.push(row.code());
                    }
                }
                match candidates.as_slice() {
                    [code] => ids.push(ExInstrumentId::new(ExMarketId::from_wire(38),ExCode::new(*code)?)),
                    [] => return Err(error),
                    _ => return Err(Error::new(ErrorKind::InvalidArgument,"ambiguous macro name in EX directory")
                        .with_context("selector",selector.as_str()).with_context("candidates",candidates.join(", "))),
                }
            }
            Err(error) => return Err(error),
        }
    }
    // Preserve exact-name lookup provenance alongside the base goods catalog.
    let meta = if let Some(names) = names {
        catalog::merge_meta(context,&[catalog.meta().clone(),names.meta().clone()],Some(catalog.version()))?
    } else {
        let mut charge = crate::types::memory::MemoryCharge::default(); catalog.meta().charge_heap(&mut charge)?;
        context.record_work(0,charge.total())?; catalog.meta().clone()
    };
    Ok((ids,catalog,meta))
}
pub(crate) fn macro_quotes(owner: &MacOwner, context: &OperationContext, query: &MacroQuotesQuery,
    pages: &PageOptions) -> Result<Vec<Quote>> {
    let lease = owner.session.lease(context)?; let mut budget = Budget::new(pages);
    let (ids,catalog,directory_meta) = resolve_all(owner,context,&lease,query.codes(),pages,&mut budget)?;
    let source = (catalog.endpoint(),catalog.generation());
    let auxiliary = directory_meta.raw().map_or(0,|raw| raw.row_ranges().len());
    let mut output = Vec::new(); let mut stages = vec![directory_meta]; let mut selected = Vec::new();
    for batch in ids.chunks(80) {
        budget.before(context)?;
        let mut wire_ids = Vec::new();
        context.record_work(0,batch.len()*(std::mem::size_of::<MacInstrumentId>()+9*6))?;
        wire_ids.try_reserve_exact(batch.len()).map_err(crate::services::allocation_error)?;
        for id in batch { wire_ids.push(MacInstrumentId::try_from(id)?); }
        let mut rows = super::client::quotes_leased(owner,context,&lease,&wire_ids,query.fields(),Some(source))?;
        // A quote response may reorder/omit rows. Preserve request occurrences
        // by identity; reuse one source observation for duplicate occurrences.
        let meta = rows.first().map(|row| row.meta().clone()).or_else(|| owner.diagnostics().ok().and_then(|value| value.latest_meta().cloned()))
            .ok_or_else(|| Error::new(ErrorKind::Internal,"macro quote batch lacks response metadata"))?;
        let base = stages.iter().skip(1).try_fold(auxiliary,|total,stage| total.checked_add(stage.raw().map_or(0,|raw| raw.row_ranges().len())))
            .ok_or_else(|| limit("macro quote source index overflow"))?;
        for id in &wire_ids {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let index = rows.iter().position(|row| row.id() == id).ok_or_else(|| Error::new(ErrorKind::MissingData,
                "resolved macro indicator is absent from its quote response").with_context("code",id.code().as_str()))?;
            context.record_work(0,2*(std::mem::size_of::<Quote>()+std::mem::size_of::<usize>())+rows[index].clone_heap_bytes()?)?;
            output.try_reserve(1).map_err(crate::services::allocation_error)?;
            selected.try_reserve(1).map_err(crate::services::allocation_error)?;
            output.push(rows[index].clone()); selected.push(base+index);
        }
        rows.clear(); stages.try_reserve(1).map_err(crate::services::allocation_error)?; stages.push(meta);
    }
    let meta = catalog::merge_meta(context,&stages,None)?.with_raw_row_selection(&selected)?;
    let shared = Arc::new(meta.clone()); for row in &mut output { row.set_meta(Arc::clone(&shared)); }
    owner.session.record_meta(meta)?; context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept(); Ok(output)
}

pub(crate) fn macro_bars(owner: &MacOwner, context: &OperationContext, query: &MacroBarsQuery,
    pages: &PageOptions, provider: Option<Arc<dyn super::XdxrProvider>>) -> Result<Vec<Bar>> {
    let lease = owner.session.lease(context)?; let mut budget = Budget::new(pages);
    let (ids,catalog,directory_meta) = resolve_all(owner,context,&lease,std::slice::from_ref(query.code()),pages,&mut budget)?;
    let query = query.bars().clone().with_id(ids.into_iter().next().ok_or_else(|| Error::new(ErrorKind::Internal,"macro resolution returned no ID"))?)?;
    let source = (catalog.endpoint(),catalog.generation()); lease.accept();
    // The normal bar collector acquires the same gate for each exchange. Exact
    // generation-pinned work makes an intervening source change an error.
    let mut bars = super::services::bars_with_source(owner,context,query.common(),provider,Some(source))?;
    let offset = directory_meta.raw().map_or(0,|raw| raw.row_ranges().len());
    context.record_work(0,bars.rows.len().checked_mul(std::mem::size_of::<usize>()).ok_or_else(|| limit("macro bar raw mapping overflows"))?)?;
    let end = offset.checked_add(bars.rows.len()).ok_or_else(|| limit("macro bar raw index overflows"))?;
    let mut selected = Vec::new(); selected.try_reserve_exact(bars.rows.len()).map_err(crate::services::allocation_error)?;
    selected.extend(offset..end);
    bars.meta = catalog::merge_meta(context,&[directory_meta,bars.meta],None)?.with_raw_row_selection(&selected)?;
    let meta = Arc::new(bars.meta.clone()); for row in &mut bars.rows { row.set_meta(Arc::clone(&meta)); }
    owner.session.record_meta(bars.meta)?; context.check_boundary(TimeoutPhase::Aggregate)?; Ok(bars.rows)
}

pub(crate) fn quote_list(owner: &MacOwner, context: &OperationContext, query: &QuoteListQuery) -> Result<PageResult<Quote>> {
    let lease = owner.session.lease(context)?; let mut budget = Budget::new(query.pages());
    let catalog = catalog::goods_snapshot(owner,context,&lease,query.market(),query.pages(),&mut budget,false)?;
    let page = catalog::page(context,&catalog,query.start(),query.requested_goods())?;
    let mut wire_ids = Vec::new();
    context.record_work(0,page.rows().len()*(std::mem::size_of::<MacInstrumentId>()+9*6))?;
    wire_ids.try_reserve_exact(page.rows().len()).map_err(crate::services::allocation_error)?;
    for item in page.rows() {
        let id = ExInstrumentId::new(query.market(),ExCode::new(item.actual_code())?);
        wire_ids.push(MacInstrumentId::try_from(&id)?);
    }
    let source_rows = if wire_ids.is_empty() { Vec::new() } else {
        budget.before(context)?;
        super::client::quotes_leased(owner,context,&lease,&wire_ids,&super::FieldSelection::mac_ex_default(),Some((catalog.endpoint(),catalog.generation())))?
    };
    let mut stages = vec![catalog.meta().clone()];
    if !wire_ids.is_empty() { stages.push(owner.diagnostics()?.latest_meta().cloned().ok_or_else(|| Error::new(ErrorKind::Internal,"list quote response lacks metadata"))?); }
    let offset = catalog.meta().raw().map_or(0,|raw| raw.row_ranges().len());
    let mut rows = Vec::new(); let mut selected = Vec::new();
    for id in &wire_ids {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let index = source_rows.iter().position(|row| row.id() == id).ok_or_else(|| Error::new(ErrorKind::MissingData,
            "source-order list identity is absent from its quote reply").with_context("code",id.code().as_str()))?;
        context.record_work(0,2*(std::mem::size_of::<Quote>()+std::mem::size_of::<usize>())+source_rows[index].clone_heap_bytes()?)?;
        rows.try_reserve(1).map_err(crate::services::allocation_error)?;
        selected.try_reserve(1).map_err(crate::services::allocation_error)?;
        rows.push(source_rows[index].clone()); selected.push(offset+index);
    }
    let mut meta = catalog::merge_meta(context,&stages,None)?.with_raw_row_selection(&selected)?;
    if query.count().get()>80 {
        meta = meta.with_diagnostic(Diagnostic::new("mac-ex","SourceOrder80",EnrichmentStatus::Succeeded,None,
            Some("quote_list selects at most min(count,80) directory occurrences; sorting remains source order".into())));
    }
    let shared = Arc::new(meta.clone()); for row in &mut rows { row.set_meta(Arc::clone(&shared)); }
    let completion = if page.completion() == PageCompletion::EndOfData { PageCompletion::EndOfData }
        else if query.count().get()>80 { PageCompletion::MoreAvailable } else { page.completion() };
    owner.session.record_meta(meta.clone())?; context.check_boundary(TimeoutPhase::Aggregate)?; lease.accept();
    PageResult::new(rows,page.server_total(),page.next_offset(),completion,meta)
}
fn limit(message: &'static str) -> Error { Error::new(ErrorKind::LimitExceeded,message).with_context("domain","mac-ex") }
