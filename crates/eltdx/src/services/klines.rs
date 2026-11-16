use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use bytes::Bytes;
use eltdx_protocol::commands::delisted_bars::DelistedKlinesRequest;
use eltdx_protocol::commands::klines::{KlineKind as WireKind, KlinesRequest};
use eltdx_protocol::unit::{DateParts, KlinePeriod as WirePeriod, NormalizedCode};
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::Engine;

use crate::config::ClientConfig;
use crate::__python_compat::{LegacyBarSeries, LegacyBarTime};
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::metadata::MetadataSnapshot;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::types::klines::{validate_page_options, AdjustMode, BarTime, DelistedQuery, KlineBar,
    KlineKind, KlinePeriod, KlineQuery, KlineSeries};
use crate::types::{Amount, AmountUnit, BatchItem, BatchItemState, BatchOutcome, DataPage, EndEvidence,
    NumericValue, PageCursor, Price, Quantity, QuantityUnit, RawPolicy, ResultMeta, SecurityId,
    TradingDate, ValueProvenance};

use super::bar_collection::{order_bar_pages, walk_bar_pages, BarCollectionPolicy, BarOrder};
use super::pagination::{advance_offset, CollectionLimits};
use super::{aggregate_meta, allocation_error, checked_ids, endpoint_engine, result_meta,
    verify_cursor, ServiceSource};

const MAX_WIRE_OFFSET: u32 = u16::MAX as u32;

#[derive(Clone, Debug)]
pub struct KlineService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
    namespace: String,
}

impl KlineService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }

    pub fn page(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions) -> Result<KlineSeries> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "klines_page", &self.config, options)?;
        let series = self.page_in_context(&mut context, id, query, None)?;
        context.finish()?;
        Ok(series)
    }

    pub fn collect(&self, id: &SecurityId, query: &KlineQuery, options: &OperationOptions) -> Result<KlineSeries> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "klines_collect", &self.config, options)?;
        let series = self.collect_in_context(&mut context, id, query)?;
        context.finish()?; Ok(series)
    }

    pub(crate) fn collect_in_context(&self, context: &mut OperationContext, id: &SecurityId, query: &KlineQuery) -> Result<KlineSeries> {
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        validate_page_options(query.pages())?;
        let continuation_pages = query.pages().max_pages().saturating_sub(1);
        context.set_work_limits(WorkLimits { pages: std::num::NonZeroUsize::new(continuation_pages),
            records: Some(query.pages().max_records()), bytes: None })?;
        self.collect_with_parent_context(context, id, query)
    }

    pub(crate) fn collect_with_parent_context(&self, context: &OperationContext, id: &SecurityId, query: &KlineQuery) -> Result<KlineSeries> {
        context.check_boundary(TimeoutPhase::Admission)?;
        checked_ids(std::slice::from_ref(id))?; query.validate()?;
        validate_page_options(query.pages())?;
        let engine = self.engine_for_cursor(&context, query.cursor())?;
        let metadata = self.capture_metadata(&context, std::slice::from_ref(id))?;
        verify_cursor(query.cursor(), ordinary_fingerprint(&context, id, query, resolve_kind(id, query.kind()))?,
            query.pages().page_size(), &context, Some(metadata.version()))?;
        let mut pin = self.executor.acquire_pin(&engine, &context)?;
        let result = self.collect_ordinary_with(&context, id, query, &metadata, &engine, Some(pin.handle()?));
        match result {
            Ok(series) => { self.executor.release_pin(context, &mut pin, None)?; Ok(series) }
            Err(root) => match self.executor.release_pin(context, &mut pin, Some(root)) {
                Err(error) => Err(error), Ok(()) => Err(Error::new(ErrorKind::Internal, "K-line child cleanup lost its root failure")),
            },
        }
    }

    pub fn page_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "klines_page_many", &self.config, options)?;
        if query.cursor().is_some() { return Err(query_error("cursor", "many queries do not accept continuation")); }
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        checked_ids(ids)?;
        query.validate()?;
        let request_count = query.count();
        let work_records = ids.len().checked_mul(request_count)
            .ok_or_else(|| limit_error("batch record limit overflows"))?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(work_records), bytes: None })?;
        let requests = ids.iter().map(|id| self.ordinary_request(id, query, u32::from(query.start()), request_count)).collect::<Result<Vec<_>>>()?;
        let engine = self.engine_for_cursor(&context, query.cursor())?;
        let metadata = self.capture_metadata(&context, ids)?;
        let mut pin = self.executor.acquire_pin(&engine, &context)?;
        let result = (|| {
            let mut items = Vec::new();
            let mut metas = Vec::new();
            items.try_reserve(ids.len()).map_err(allocation_error)?;
            metas.try_reserve(ids.len()).map_err(allocation_error)?;
            for (index, (id, request)) in ids.iter().zip(requests).enumerate() {
                context.check_boundary(TimeoutPhase::Admission)?;
                let series = self.page_ordinary_with(&context, id, query, request, &metadata, &engine, Some(pin.handle()?))?;
                metas.push(series.meta().clone());
                items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(series)));
            }
            let meta = aggregate_meta(&context, &metas, Some(metadata.version()))?;
            Ok(BatchOutcome::with_meta(items, meta))
        })();
        self.finish_pinned(&context, &mut pin, result)
    }

    pub fn collect_many(&self, ids: &[SecurityId], query: &KlineQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "klines_collect_many", &self.config, options)?;
        if query.cursor().is_some() { return Err(query_error("cursor", "many queries do not accept continuation")); }
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        checked_ids(ids)?;
        let _ = query.validate()?;
        validate_page_options(query.pages())?;
        let pages = ids.len().checked_mul(query.pages().max_pages())
            .ok_or_else(|| limit_error("batch page limit overflows"))?;
        let records = ids.len().checked_mul(query.pages().max_records())
            .ok_or_else(|| limit_error("batch record limit overflows"))?;
        context.set_work_limits(WorkLimits { pages: pages.checked_sub(ids.len()).and_then(std::num::NonZeroUsize::new),
            records: Some(records), bytes: None })?;
        let engine = self.engine_for_cursor(&context, query.cursor())?;
        let metadata = self.capture_metadata(&context, ids)?;
        let mut pin = self.executor.acquire_pin(&engine, &context)?;
        let result = (|| {
            let mut items = Vec::new();
            let mut metas = Vec::new();
            items.try_reserve(ids.len()).map_err(allocation_error)?;
            metas.try_reserve(ids.len()).map_err(allocation_error)?;
            for (index, id) in ids.iter().enumerate() {
                context.check_boundary(TimeoutPhase::Admission)?;
                let series = self.collect_ordinary_with(&context, id, query, &metadata, &engine, Some(pin.handle()?))?;
                metas.push(series.meta().clone());
                items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(series)));
            }
            let meta = aggregate_meta(&context, &metas, Some(metadata.version()))?;
            Ok(BatchOutcome::with_meta(items, meta))
        })();
        self.finish_pinned(&context, &mut pin, result)
    }

    pub fn delisted_page(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions) -> Result<KlineSeries> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "delisted_klines_page", &self.config, options)?;
        let series = self.delisted_page_in_context(&mut context, id, query, None)?;
        context.finish()?;
        Ok(series)
    }

    pub fn delisted_collect(&self, id: &SecurityId, query: &DelistedQuery, options: &OperationOptions) -> Result<KlineSeries> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "delisted_klines_collect", &self.config, options)?;
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        validate_page_options(query.pages())?;
        let continuation_pages = query.pages().max_pages().saturating_sub(1);
        context.set_work_limits(WorkLimits { pages: std::num::NonZeroUsize::new(continuation_pages),
            records: Some(query.pages().max_records()), bytes: None })?;
        let engine = self.engine_for_cursor(&context, query.cursor())?;
        let metadata = self.capture_metadata(&context, std::slice::from_ref(id))?;
        verify_cursor(query.cursor(), delisted_fingerprint(&context, id)?, query.pages().page_size(),
            &context, Some(metadata.version()))?;
        let mut pin = self.executor.acquire_pin(&engine, &context)?;
        let result = self.collect_delisted_with(&context, id, query, &metadata, &engine, Some(pin.handle()?));
        self.finish_pinned(&context, &mut pin, result)
    }

    pub fn delisted_page_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "delisted_klines_page_many", &self.config, options)?;
        if query.cursor().is_some() { return Err(query_error("cursor", "many queries do not accept continuation")); }
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        checked_ids(ids)?;
        query.validate()?;
        let count = query.count();
        let records = ids.len().checked_mul(count).ok_or_else(|| limit_error("batch record limit overflows"))?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(records), bytes: None })?;
        let requests = ids.iter().map(|id| self.delisted_request(id, u32::from(query.start()), count)).collect::<Result<Vec<_>>>()?;
        let engine = self.engine.clone();
        let metadata = self.capture_metadata(&context, ids)?;
        let mut pin = self.executor.acquire_pin(&engine, &context)?;
        let result = (|| {
            let mut items = Vec::new(); let mut metas = Vec::new();
            items.try_reserve(ids.len()).map_err(allocation_error)?; metas.try_reserve(ids.len()).map_err(allocation_error)?;
            for (index, (id, request)) in ids.iter().zip(requests).enumerate() {
                let series = self.page_delisted_with(&context, id, query, request, &metadata, &engine, Some(pin.handle()?))?;
                metas.push(series.meta().clone()); items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(series)));
            }
            Ok(BatchOutcome::with_meta(items, aggregate_meta(&context, &metas, Some(metadata.version()))?))
        })();
        self.finish_pinned(&context, &mut pin, result)
    }

    pub fn delisted_collect_many(&self, ids: &[SecurityId], query: &DelistedQuery, options: &OperationOptions) -> Result<BatchOutcome<KlineSeries>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "delisted_klines_collect_many", &self.config, options)?;
        if query.cursor().is_some() { return Err(query_error("cursor", "many queries do not accept continuation")); }
        if ids.is_empty() { context.finish()?; return Ok(BatchOutcome::new(Vec::new())); }
        checked_ids(ids)?;
        let _ = query.validate()?;
        let pages = ids.len().checked_mul(query.pages().max_pages()).ok_or_else(|| limit_error("batch page limit overflows"))?;
        let records = ids.len().checked_mul(query.pages().max_records()).ok_or_else(|| limit_error("batch record limit overflows"))?;
        context.set_work_limits(WorkLimits { pages: pages.checked_sub(ids.len()).and_then(std::num::NonZeroUsize::new), records: Some(records), bytes: None })?;
        let engine = self.engine.clone();
        let metadata = self.capture_metadata(&context, ids)?;
        let mut pin = self.executor.acquire_pin(&engine, &context)?;
        let result = (|| {
            let mut items = Vec::new(); let mut metas = Vec::new();
            items.try_reserve(ids.len()).map_err(allocation_error)?; metas.try_reserve(ids.len()).map_err(allocation_error)?;
            for (index, id) in ids.iter().enumerate() {
                let series = self.collect_delisted_with(&context, id, query, &metadata, &engine, Some(pin.handle()?))?;
                metas.push(series.meta().clone()); items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(series)));
            }
            Ok(BatchOutcome::with_meta(items, aggregate_meta(&context, &metas, Some(metadata.version()))?))
        })();
        self.finish_pinned(&context, &mut pin, result)
    }

    fn page_in_context(&self, context: &mut OperationContext, id: &SecurityId, query: &KlineQuery,
        pin: Option<&eltdx_runtime::engine::PinHandle>) -> Result<KlineSeries> {
        checked_ids(std::slice::from_ref(id))?;
        query.validate()?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(query.count()), bytes: None })?;
        self.page_with_parent_context(context, id, query, pin)
    }

    /// Bounded single-page child; keeps the parent's limits and lifetime.
    pub(crate) fn page_with_parent_context(&self, context: &OperationContext, id: &SecurityId, query: &KlineQuery,
        pin: Option<&eltdx_runtime::engine::PinHandle>) -> Result<KlineSeries> {
        context.check_boundary(TimeoutPhase::Admission)?;
        checked_ids(std::slice::from_ref(id))?; query.validate()?;
        let request = self.ordinary_request(id, query, u32::from(query.start()), query.count())?;
        let engine = self.engine_for_cursor(context, query.cursor())?;
        let metadata = self.capture_metadata(context, std::slice::from_ref(id))?;
        let resolved_kind = resolve_kind(id, query.kind());
        let fingerprint = ordinary_fingerprint(context, id, query, resolved_kind)?;
        verify_cursor(query.cursor(), fingerprint, query.count(), context, Some(metadata.version()))?;
        self.page_ordinary_with(context, id, query, request, &metadata, &engine, pin)
    }

    fn page_ordinary_with(&self, context: &OperationContext, id: &SecurityId, query: &KlineQuery,
        request: KlinesRequest, metadata: &Arc<MetadataSnapshot>, engine: &Engine,
        pin: Option<&eltdx_runtime::engine::PinHandle>) -> Result<KlineSeries> {
        let start = u32::from(request.start);
        let page_size = usize::from(request.count);
        let (decoded, _) = self.execute(context, engine, pin, CommandRequest::Klines(request), ChildPurpose::Required)?;
        let CommandResponse::Klines(response) = &decoded.response else { return Err(wrong_response("klines")); };
        validate_count(response.bars.len(), page_size)?;
        context.record_work(response.bars.len(), 0)?;
        let actual_anchor = response.anchor_date.map(date_from_parts).transpose()?;
        if query.anchor_date().is_some() && actual_anchor.is_some() && actual_anchor != query.anchor_date() {
            return Err(Error::new(ErrorKind::Protocol, "K-line response anchor differs from request"));
        }
        let anchor = actual_anchor.or(query.anchor_date());
        let bars = response.bars.iter().map(|bar| project_bar(bar, query.period(), query.bar_time(), false)).collect::<Result<Vec<_>>>()?;
        let source = ServiceSource::from_reply(&decoded);
        let ranges = response.bars.iter().map(|bar| bar.record_range).collect::<Vec<_>>();
        let meta = result_meta(context, &self.config, Some(metadata), &source, Some(decoded.payload.clone()),
            decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "klines.page");
        let next = advance_offset(start, bars.len())?;
        let (end, cursor) = if bars.is_empty() { (EndEvidence::EmptyPage, None) }
            else if next > MAX_WIRE_OFFSET { (EndEvidence::RequestedWindow, None) }
            else { (EndEvidence::Unknown, Some(PageCursor::new(next, page_size,
                ordinary_fingerprint(context, id, query, resolve_kind(id, query.kind()))?, context.session_id,
                Some(source.host), Some(metadata.version())))) };
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(KlineSeries::new(id.clone(), query.period(), resolve_kind(id, query.kind()), query.adjust(), anchor,
            query.bar_time(), DataPage::new(bars, u64::from(start), page_size, cursor, end, meta.clone()), 1,
            (context.raw() != RawPolicy::Omit).then(|| decoded.payload.clone())))
    }

    fn collect_ordinary_with(&self, context: &OperationContext, id: &SecurityId, query: &KlineQuery,
        metadata: &Arc<MetadataSnapshot>, engine: &Engine, pin: Option<&eltdx_runtime::engine::PinHandle>) -> Result<KlineSeries> {
        validate_page_options(query.pages())?;
        let page_size = query.pages().page_size();
        let resolved_kind = resolve_kind(id, query.kind());
        let fingerprint = ordinary_fingerprint(context, id, query, resolved_kind)?;
        verify_cursor(query.cursor(), fingerprint, page_size, context, Some(metadata.version()))?;
        let collection_start = query.cursor().map_or(u32::from(query.start()), PageCursor::next_offset);
        ensure_wire_offset(collection_start)?;
        let limits = CollectionLimits { pages: Some(query.pages().max_pages()), records: Some(query.pages().max_records()) };
        let mut pages = Vec::<Vec<KlineBar>>::new();
        let mut metas = Vec::<ResultMeta>::new();
        let mut anchor = query.anchor_date();
        let pages_read = walk_bar_pages(context, collection_start, page_size,
            BarCollectionPolicy::Strict { limits, repeated_page: "K-line pagination repeated a response page",
                offset_error: "K-line continuation offset exceeds the wire range before EOF" },
            |offset, count, purpose| {
            let request = self.ordinary_request(id, query, offset, count)?;
            let (decoded, page_payload) = self.execute(context, engine, pin, CommandRequest::Klines(request), purpose)?;
            let CommandResponse::Klines(response) = &decoded.response else { return Err(wrong_response("klines")); };
            validate_count(response.bars.len(), count)?;
            let actual = response.bars.len();
            Ok((decoded, actual, page_payload))
        }, |decoded| {
            let CommandResponse::Klines(response) = &decoded.response else { return Err(wrong_response("klines")); };
            if response.bars.is_empty() {
                let source = ServiceSource::from_reply(&decoded);
                metas.try_reserve(1).map_err(allocation_error)?;
                metas.push(page_meta(context, &self.config, metadata, &source, &decoded, &[], "klines.page"));
                return Ok(());
            }
            let page_anchor = response.anchor_date.map(date_from_parts).transpose()?;
            if let (Some(existing), Some(found)) = (anchor, page_anchor) {
                if existing != found { return Err(Error::new(ErrorKind::Protocol, "K-line anchor changed between pages")); }
            }
            if anchor.is_none() { anchor = page_anchor; }
            let bars = response.bars.iter().map(|bar| project_bar(bar, query.period(), query.bar_time(), false)).collect::<Result<Vec<_>>>()?;
            let source = ServiceSource::from_reply(&decoded);
            let ranges = response.bars.iter().map(|bar| bar.record_range).collect::<Vec<_>>();
            metas.try_reserve(1).map_err(allocation_error)?;
            metas.push(page_meta(context, &self.config, metadata, &source, &decoded, &ranges, "klines.page"));
            pages.try_reserve(1).map_err(allocation_error)?;
            pages.push(bars);
            Ok(())
        })?;
        finish_collection(context, id, query.period(), resolved_kind, query.adjust(), anchor,
            query.bar_time(), pages, metas, pages_read, collection_start, page_size, metadata.version())
    }

    fn delisted_page_in_context(&self, context: &mut OperationContext, id: &SecurityId, query: &DelistedQuery,
        pin: Option<&eltdx_runtime::engine::PinHandle>) -> Result<KlineSeries> {
        checked_ids(std::slice::from_ref(id))?;
        let start = query.validate()?;
        context.set_work_limits(WorkLimits { pages: None, records: Some(query.count()), bytes: None })?;
        let request = self.delisted_request(id, start, query.count())?;
        let engine = self.engine_for_cursor(context, query.cursor())?;
        let metadata = self.capture_metadata(context, std::slice::from_ref(id))?;
        let fingerprint = delisted_fingerprint(context, id)?;
        verify_cursor(query.cursor(), fingerprint, query.count(), context, Some(metadata.version()))?;
        self.page_delisted_with(context, id, query, request, &metadata, &engine, pin)
    }

    fn page_delisted_with(&self, context: &OperationContext, id: &SecurityId, _query: &DelistedQuery,
        request: DelistedKlinesRequest, metadata: &Arc<MetadataSnapshot>, engine: &Engine,
        pin: Option<&eltdx_runtime::engine::PinHandle>) -> Result<KlineSeries> {
        let start = u32::from(request.start);
        let page_size = usize::from(request.count);
        let (decoded, _) = self.execute(context, engine, pin, CommandRequest::DelistedKlines(request), ChildPurpose::Required)?;
        let CommandResponse::DelistedKlines(response) = &decoded.response else { return Err(wrong_response("delisted klines")); };
        validate_count(response.bars.len(), page_size)?;
        context.record_work(response.bars.len(), 0)?;
        let bars = response.bars.iter().map(|bar| project_bar(bar, KlinePeriod::Day, BarTime::Start, true)).collect::<Result<Vec<_>>>()?;
        let source = ServiceSource::from_reply(&decoded);
        let ranges = response.bars.iter().map(|bar| bar.record_range).collect::<Vec<_>>();
        let meta = result_meta(context, &self.config, Some(metadata), &source, Some(decoded.payload.clone()),
            decoded.wire.as_ref().map(|wire| wire.raw.clone()), &ranges, "delisted_klines.page");
        let next = advance_offset(start, bars.len())?;
        let (end, cursor) = if bars.is_empty() { (EndEvidence::EmptyPage, None) }
            else if next > MAX_WIRE_OFFSET { (EndEvidence::RequestedWindow, None) }
            else { (EndEvidence::Unknown, Some(PageCursor::new(next, page_size, delisted_fingerprint(context, id)?,
                context.session_id, Some(source.host), Some(metadata.version())))) };
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(KlineSeries::new(id.clone(), KlinePeriod::Day, KlineKind::Stock, AdjustMode::None, None, BarTime::Start,
            DataPage::new(bars, u64::from(start), page_size, cursor, end, meta), 1,
            (context.raw() != RawPolicy::Omit).then(|| decoded.payload.clone())))
    }

    fn collect_delisted_with(&self, context: &OperationContext, id: &SecurityId, query: &DelistedQuery,
        metadata: &Arc<MetadataSnapshot>, engine: &Engine, pin: Option<&eltdx_runtime::engine::PinHandle>) -> Result<KlineSeries> {
        validate_page_options(query.pages())?;
        let page_size = query.pages().page_size();
        let fingerprint = delisted_fingerprint(context, id)?;
        verify_cursor(query.cursor(), fingerprint, page_size, context, Some(metadata.version()))?;
        let start = query.cursor().map_or(u32::from(query.start()), PageCursor::next_offset);
        ensure_wire_offset(start)?;
        let limits = CollectionLimits { pages: Some(query.pages().max_pages()), records: Some(query.pages().max_records()) };
        let mut pages = Vec::new();
        let mut metas = Vec::<ResultMeta>::new();
        let mut first_raw = None;
        let pages_read = walk_bar_pages(context, start, page_size,
            BarCollectionPolicy::Strict { limits, repeated_page: "delisted pagination repeated a response page",
                offset_error: "delisted continuation offset exceeds the wire range before EOF" },
            |offset, count, purpose| {
            let request = self.delisted_request(id, offset, count)?;
            let (decoded, page_payload) = self.execute(context, engine, pin, CommandRequest::DelistedKlines(request), purpose)?;
            let CommandResponse::DelistedKlines(response) = &decoded.response else { return Err(wrong_response("delisted klines")); };
            validate_count(response.bars.len(), count)?;
            let actual = response.bars.len();
            Ok((decoded, actual, page_payload))
        }, |decoded| {
            let CommandResponse::DelistedKlines(response) = &decoded.response else { return Err(wrong_response("delisted klines")); };
            let source = ServiceSource::from_reply(&decoded);
            let ranges = response.bars.iter().map(|bar| bar.record_range).collect::<Vec<_>>();
            metas.try_reserve(1).map_err(allocation_error)?;
            metas.push(page_meta(context, &self.config, metadata, &source, &decoded, &ranges, "delisted_klines.page"));
            if first_raw.is_none() && !response.bars.is_empty() { first_raw = Some(decoded.payload.clone()); }
            let decoded_bars = response.bars.iter().map(|bar| project_bar(bar, KlinePeriod::Day, BarTime::Start, true)).collect::<Result<Vec<_>>>()?;
            pages.try_reserve(1).map_err(allocation_error)?;
            pages.push(decoded_bars);
            Ok(())
        })?;
        let (ordered_bars, row_order) = order_bar_pages(pages, BarOrder::Chronological,
            |bar| (bar.time().date(), bar.time().clock()))?;
        let meta = aggregate_meta(context, &metas, Some(metadata.version()))?.with_raw_row_order(&row_order)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(KlineSeries::new(id.clone(), KlinePeriod::Day, KlineKind::Stock, AdjustMode::None, None,
            BarTime::Start, DataPage::new(ordered_bars, u64::from(start), page_size, None, EndEvidence::EmptyPage, meta),
            pages_read, (context.raw() != RawPolicy::Omit).then(|| first_raw.unwrap_or_default())))
    }

    fn ordinary_request(&self, id: &SecurityId, query: &KlineQuery, start: u32, count: usize) -> Result<KlinesRequest> {
        ensure_wire_offset(start)?;
        if count == 0 || count > 800 { return Err(query_error("count", "must be between 1 and 800")); }
        let (raw, parameter) = query.period().wire()?;
        let kind = resolve_kind(id, query.kind());
        let protocol_kind = match kind { KlineKind::Stock | KlineKind::Auto => WireKind::Stock, KlineKind::Index => WireKind::Index };
        let anchor = query.anchor_date().map(date_to_raw).transpose()?.unwrap_or(0);
        KlinesRequest::new(normalized_id(id)?, WirePeriod::new(raw, parameter), start as u16, count as u16,
            query.adjust().wire(), anchor, protocol_kind).map_err(Error::from_protocol)
    }

    fn delisted_request(&self, id: &SecurityId, start: u32, count: usize) -> Result<DelistedKlinesRequest> {
        ensure_wire_offset(start)?;
        if count == 0 || count > 800 { return Err(query_error("count", "must be between 1 and 800")); }
        DelistedKlinesRequest::new(normalized_id(id)?, start as u16, count as u16, false).map_err(Error::from_protocol)
    }

    fn engine_for_cursor(&self, _context: &OperationContext, cursor: Option<&PageCursor>) -> Result<Engine> {
        endpoint_engine(&self.executor, &self.config, &self.engine, cursor.and_then(PageCursor::endpoint))
    }

    // OHLC uses a fixed wire price scale and needs no directory fetch. Capture
    // any already published catalogs to bind continuation to their versions.
    fn capture_metadata(&self, context: &OperationContext, ids: &[SecurityId]) -> Result<Arc<MetadataSnapshot>> {
        context.current_market_date()?;
        super::capture_existing_metadata(context, &self.namespace, ids)
    }

    fn execute(&self, context: &OperationContext, engine: &Engine, pin: Option<&eltdx_runtime::engine::PinHandle>,
        request: CommandRequest, purpose: ChildPurpose) -> Result<(DecodedReply, Bytes)> {
        context.check_boundary(TimeoutPhase::Admission)?;
        let raw = match pin {
            Some(pin) => self.executor.execute_pinned_raw(engine, pin, context, request, purpose, None)?,
            None => self.executor.execute_raw(engine, context, request, purpose, None)?,
        };
        // Keep exact page evidence during collection even when diagnostic raw is omitted.
        // The collector drops it before returning; it is never added to Omit results.
        let page_payload = raw.response.data.clone();
        Ok((self.executor.decode_received(context, raw)?, page_payload))
    }

    fn finish_pinned<T>(&self, context: &OperationContext, pin: &mut crate::session::PinnedSession, result: Result<T>) -> Result<T> {
        match result {
            Ok(value) => { self.executor.release_pin(context, pin, None)?; context.finish()?; Ok(value) }
            Err(root) => match self.executor.release_pin(context, pin, Some(root)) {
                Err(error) => Err(error),
                Ok(()) => Err(Error::new(ErrorKind::Internal, "temporary K-line failure lost its root cause")),
            },
        }
    }
}

fn finish_collection(context: &OperationContext, id: &SecurityId, period: KlinePeriod, kind: KlineKind,
    adjust: AdjustMode, anchor: Option<TradingDate>, bar_time: BarTime, pages: Vec<Vec<KlineBar>>,
    metas: Vec<ResultMeta>, pages_read: usize, start: u32, page_size: usize, metadata_version: u64) -> Result<KlineSeries> {
    let (bars, row_order) = order_bar_pages(pages, BarOrder::ReversePages, |_| ())?;
    let legacy_raw = (context.raw() != RawPolicy::Omit).then(Bytes::new);
    let meta = aggregate_meta(context, &metas, Some(metadata_version))?.with_raw_row_order(&row_order)?;
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(KlineSeries::new(id.clone(), period, kind, adjust, anchor, bar_time,
        DataPage::new(bars, u64::from(start), page_size, None, EndEvidence::EmptyPage, meta), pages_read, legacy_raw))
}

fn project_bar(row: &eltdx_protocol::commands::klines::KlineBar, period: KlinePeriod, bar_time: BarTime,
    delisted: bool) -> Result<KlineBar> {
    let source_time = event_time(row.time)?;
    let mut time = source_time;
    if !delisted && bar_time == BarTime::End {
        if let Some(minutes) = period.minute_width() {
            time = align_bar_time(source_time, minutes, BarClockPolicy::AdvanceDate)?;
        }
    }
    let price = |value| Price::with_value_and_provenance(
        NumericValue::ScaledInteger { value, scale: 3 }, ValueProvenance::WireRule);
    let volume = Quantity::with_provenance(NumericValue::F64(row.volume_lots), QuantityUnit::Lots, ValueProvenance::WireRule);
    let amount = Amount::with_provenance(NumericValue::F64(row.amount), AmountUnit::Unknown, ValueProvenance::WireRule);
    Ok(KlineBar::new(time, source_time, price(row.open_price_milli), price(row.close_price_milli), price(row.high_price_milli),
        price(row.low_price_milli), row.last_close_price_milli.map(price), volume, amount,
        row.up_count.map(u32::from), row.down_count.map(u32::from), row.volume_raw, row.amount_raw,
        row.unknown_suffix_raw))
}

#[derive(Clone, Copy)]
enum BarClockPolicy { AdvanceDate, SameDate }

fn align_bar_time(source: crate::types::EventTime, minutes: u16, policy: BarClockPolicy) -> Result<crate::types::EventTime> {
    match policy {
        BarClockPolicy::AdvanceDate => source.add_minutes(i64::from(minutes)),
        BarClockPolicy::SameDate => {
            let date = source.date().ok_or_else(|| wrong_response("dated K-line time"))?;
            let (hour, minute, second) = source.clock().ok_or_else(|| wrong_response("K-line clock"))?;
            let total = u32::from(hour) * 60 + u32::from(minute) + u32::from(minutes);
            crate::types::EventTime::shanghai(date, ((total / 60) % 24) as u8, (total % 60) as u8, second)
        }
    }
}

/// Compatibility entry to the same pagination/order machinery as the public
/// service. It keeps first-page request/anchor/raw metadata and per-wire timing.
pub(crate) fn legacy_bar_series(
    context: &OperationContext, request: CommandRequest, collect: bool,
    maximum_pages: Option<usize>, bar_time: LegacyBarTime,
    mut fetch: impl FnMut(CommandRequest, ChildPurpose) -> Result<DecodedReply>,
) -> Result<LegacyBarSeries> {
    if !context.is_legacy() { return Err(query_error("profile", "legacy bars require a compatibility operation")); }
    let (start, count, order, api) = match &request {
        CommandRequest::Klines(request) => (request.start, request.count, BarOrder::ReversePages, "bars"),
        CommandRequest::DelistedKlines(request) => (request.start, request.count, BarOrder::Chronological, "delisted_bars"),
        _ => return Err(query_error("command", "legacy bars require an OHLC request")),
    };
    if !collect {
        let mut reply = fetch(request, ChildPurpose::Required)?;
        align_legacy_bars(&mut reply.response, &bar_time)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        return Ok(LegacyBarSeries { response: reply.response, request_count: usize::from(count) });
    }
    let mut first_response = None;
    let mut pages = Vec::new();
    walk_bar_pages(context, u32::from(start), usize::from(count),
        BarCollectionPolicy::Legacy { maximum_pages, api }, |offset, size, purpose| {
            let mut page_request = request.clone();
            // walk_bar_pages checks the wire offset before constructing a child.
            match &mut page_request {
                CommandRequest::Klines(request) => { request.start = offset as u16; request.count = size as u16; }
                CommandRequest::DelistedKlines(request) => { request.start = offset as u16; request.count = size as u16; }
                _ => return Err(wrong_response("legacy bars request")),
            }
            let mut reply = fetch(page_request, purpose)?;
            align_legacy_bars(&mut reply.response, &bar_time)?;
            let actual = match &reply.response {
                CommandResponse::Klines(response) => response.bars.len(),
                CommandResponse::DelistedKlines(response) => response.bars.len(),
                _ => return Err(wrong_response("legacy bars")),
            };
            // Legacy did not reject repeated bytes. A repeated page can contain
            // valid equal-time bars; the caller's page cap remains authoritative.
            Ok((reply.response, actual, Bytes::new()))
        }, |mut response| {
            let bars = match &mut response {
                CommandResponse::Klines(response) => std::mem::take(&mut response.bars),
                CommandResponse::DelistedKlines(response) => std::mem::take(&mut response.bars),
                _ => return Err(wrong_response("legacy bars")),
            };
            pages.try_reserve(1).map_err(allocation_error)?;
            pages.push(bars);
            if first_response.is_none() { first_response = Some(response); }
            Ok(())
        })?;
    let (bars, _) = order_bar_pages(pages, order, |bar| {
        let time = bar.time;
        (time.date.year, time.date.month, time.date.day, time.hour, time.minute, time.second)
    })?;
    let request_count = bars.len();
    let mut response = first_response.ok_or_else(|| wrong_response("first legacy bar page"))?;
    match &mut response {
        CommandResponse::Klines(response) => { response.bars = bars; response.raw_payload = Bytes::new(); }
        CommandResponse::DelistedKlines(response) => response.bars = bars,
        _ => return Err(wrong_response("legacy bar aggregate")),
    }
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(LegacyBarSeries { response, request_count })
}

fn align_legacy_bars(response: &mut CommandResponse, bar_time: &LegacyBarTime) -> Result<()> {
    let CommandResponse::Klines(response) = response else {
        return if matches!(response, CommandResponse::DelistedKlines(_)) { Ok(()) }
            else { Err(wrong_response("legacy bars")) };
    };
    match bar_time {
        LegacyBarTime::Start => return Ok(()),
        LegacyBarTime::DeferredInvalid(message) => return Err(Error::new(ErrorKind::InvalidArgument, message.clone())
            .with_context("field", "bar_time")),
        LegacyBarTime::End => {},
    }
    let width = match response.request.period.raw {
        0 => Some(5), 1 => Some(15), 2 => Some(30), 3 => Some(60), 7 => Some(1),
        8 => Some(response.request.period.parameter.max(1)), _ => None,
    };
    if let Some(width) = width {
        for bar in &mut response.bars {
            let aligned = align_bar_time(event_time(bar.time)?, width, BarClockPolicy::SameDate)?;
            let (hour, minute, _) = aligned.clock().ok_or_else(|| wrong_response("aligned bar clock"))?;
            // Preserve original date, timezone, seconds, wire scalars and hex.
            bar.time.hour = hour;
            bar.time.minute = minute;
        }
    }
    Ok(())
}

fn event_time(value: eltdx_protocol::unit::DateTimeParts) -> Result<crate::types::EventTime> {
    let date = date_from_parts(value.date)?;
    crate::types::EventTime::shanghai(date, value.hour, value.minute, value.second)
        .map_err(|error| Error::new(ErrorKind::Protocol, "protocol returned an invalid K-line time").with_source(error))
}

fn date_from_parts(value: DateParts) -> Result<TradingDate> {
    let year = u16::try_from(value.year).map_err(|_| Error::new(ErrorKind::Protocol, "date year is outside the SDK range"))?;
    TradingDate::new(year, value.month, value.day).map_err(|error| Error::new(ErrorKind::Protocol,
        "protocol returned an invalid date").with_source(error))
}

fn date_to_raw(value: TradingDate) -> Result<u32> {
    u32::from(value.year()).checked_mul(10_000)
        .and_then(|year| year.checked_add(u32::from(value.month()) * 100))
        .and_then(|date| date.checked_add(u32::from(value.day())))
        .ok_or_else(|| query_error("anchor_date", "date cannot be represented on the wire"))
}

fn normalized_id(id: &SecurityId) -> Result<NormalizedCode> {
    NormalizedCode::parse(&id.to_string()).map_err(|error| Error::new(ErrorKind::InvalidArgument,
        "invalid HQ security identity").with_source(error))
}

fn resolve_kind(id: &SecurityId, kind: KlineKind) -> KlineKind {
    if kind != KlineKind::Auto { return kind; }
    let number = id.number();
    let index = match id.market() {
        crate::types::HqMarket::Shanghai => ["000", "880", "881", "999"].iter().any(|prefix| number.starts_with(prefix)),
        crate::types::HqMarket::Shenzhen => number.starts_with("399"),
        crate::types::HqMarket::Beijing => number.starts_with("899"),
    };
    if index { KlineKind::Index } else { KlineKind::Stock }
}

fn ordinary_fingerprint(context: &OperationContext, id: &SecurityId, query: &KlineQuery, resolved: KlineKind) -> Result<u64> {
    let mut state = std::collections::hash_map::DefaultHasher::new();
    "hq:052d".hash(&mut state); id.hash(&mut state); query.period().hash(&mut state);
    query.adjust().hash(&mut state); query.anchor_date().hash(&mut state); resolved.hash(&mut state);
    query.bar_time().hash(&mut state); context.current_market_date()?.hash(&mut state); Ok(state.finish())
}

fn delisted_fingerprint(context: &OperationContext, id: &SecurityId) -> Result<u64> {
    let mut state = std::collections::hash_map::DefaultHasher::new();
    "hq:052b".hash(&mut state); id.hash(&mut state); context.current_market_date()?.hash(&mut state); Ok(state.finish())
}

fn ensure_wire_offset(value: u32) -> Result<()> {
    if value > MAX_WIRE_OFFSET { return Err(limit_error("continuation offset exceeds the wire range")); }
    Ok(())
}

fn validate_count(actual: usize, requested: usize) -> Result<()> {
    if actual > requested { return Err(Error::new(ErrorKind::Protocol, "K-line response exceeds requested count")); }
    Ok(())
}

fn page_meta(context: &OperationContext, config: &ClientConfig, metadata: &MetadataSnapshot,
    source: &ServiceSource, decoded: &DecodedReply, ranges: &[eltdx_protocol::RecordRange], name: &str) -> ResultMeta {
    result_meta(context, config, Some(metadata), source, Some(decoded.payload.clone()),
        decoded.wire.as_ref().map(|wire| wire.raw.clone()), ranges, name)
}

fn query_error(field: &'static str, message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("field", field)
}

fn wrong_response(name: &str) -> Error { Error::new(ErrorKind::Protocol, format!("{name} request returned the wrong response kind")) }
fn limit_error(message: &'static str) -> Error { Error::new(ErrorKind::LimitExceeded, message) }
