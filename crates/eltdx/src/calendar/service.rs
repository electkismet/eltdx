use std::sync::{Arc, Mutex};
use std::time::Instant;
use eltdx_runtime::engine::Engine;
use crate::config::ClientConfig;
use crate::hq::executor::HqExecutor;
use crate::operation::{OperationContext, OperationOptions};
use crate::services::KlineService;
use crate::types::{AdjustMode, CalendarQuery, CalendarSnapshot, EndEvidence, KlineKind, KlinePeriod, KlineQuery};
use crate::{Error, ErrorKind, Result, SecurityId, TimeoutPhase, TradingDate};

#[derive(Debug, Default)]
struct CacheState { generation: u64, disabled: bool, entries: Vec<Arc<CalendarSnapshot>> }
/// Owner-local immutable snapshots. A clear or newer refresh prevents an older
/// in-flight build from publishing. Cache cardinality is bounded independently.
#[derive(Debug, Default)]
pub(crate) struct CalendarCache { state: Mutex<CacheState> }
impl CalendarCache {
    fn get(&self, id: &SecurityId, today: TradingDate) -> Result<Option<Arc<CalendarSnapshot>>> {
        Ok(self.state.lock().map_err(|_| poisoned())?.entries.iter()
            .find(|entry| entry.benchmark() == id && entry.loaded_on() == today).cloned())
    }
    fn begin(&self) -> Result<u64> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        state.generation = state.generation.checked_add(1).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "calendar generation exhausted"))?;
        Ok(state.generation)
    }
    fn publish(&self, generation: u64, snapshot: Arc<CalendarSnapshot>) -> Result<()> {
        let mut state = self.state.lock().map_err(|_| poisoned())?;
        if state.disabled || state.generation != generation { return Ok(()); }
        state.entries.retain(|entry| entry.benchmark() != snapshot.benchmark() && entry.loaded_on() == snapshot.loaded_on());
        if state.entries.len() == 16 { state.entries.remove(0); }
        state.entries.try_reserve(1).map_err(crate::services::allocation_error)?;
        state.entries.push(snapshot); Ok(())
    }
    fn clear(&self) {
        let mut state = self.state.lock().unwrap_or_else(|poison| poison.into_inner());
        // Exhaustion permanently disables publication rather than wrapping into
        // the identity of a build invalidated by this clear.
        if state.generation == u64::MAX { state.disabled = true; }
        state.generation = state.generation.saturating_add(1); state.entries.clear();
    }
}

#[derive(Clone, Debug)]
pub struct CalendarService { executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String }
impl CalendarService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self { Self { executor, engine, config, namespace } }
    /// Clock failures stay explicit; the current civil date is no claim that
    /// the exchange is open or that this date is covered by a loaded calendar.
    pub fn today(&self) -> Result<TradingDate> { self.executor.owner().current_shanghai_date() }
    pub fn refresh(&self, query: &CalendarQuery, options: &OperationOptions) -> Result<CalendarSnapshot> {
        let entry = Instant::now(); let mut context = self.executor.begin_context_at(entry, "calendar_refresh", &self.config, options)?;
        let snapshot = self.load(&mut context, query, true)?;
        context.finish()?; Ok((*snapshot).clone())
    }
    pub fn is_workday(&self, date: TradingDate, query: &CalendarQuery, options: &OperationOptions) -> Result<bool> {
        let entry = Instant::now(); let mut context = self.executor.begin_context_at(entry, "calendar_is_workday", &self.config, options)?;
        let snapshot = self.load(&mut context, query, false)?; snapshot.require_coverage(date)?;
        let result = snapshot.days().binary_search(&date).is_ok(); context.finish()?; Ok(result)
    }
    pub fn range(&self, mut start: TradingDate, mut end: TradingDate, descending: bool, query: &CalendarQuery, options: &OperationOptions) -> Result<Vec<TradingDate>> {
        let entry = Instant::now(); let mut context = self.executor.begin_context_at(entry, "calendar_range", &self.config, options)?;
        if start > end { std::mem::swap(&mut start, &mut end); }
        let snapshot = self.load(&mut context, query, false)?; snapshot.require_coverage(start)?; snapshot.require_coverage(end)?;
        let left = snapshot.days().partition_point(|date| *date < start); let right = snapshot.days().partition_point(|date| *date <= end);
        let mut dates = Vec::new(); dates.try_reserve(right - left).map_err(crate::services::allocation_error)?;
        dates.extend_from_slice(&snapshot.days()[left..right]); if descending { dates.reverse(); }
        context.finish()?; Ok(dates)
    }
    pub fn next(&self, date: TradingDate, include_self: bool, query: &CalendarQuery, options: &OperationOptions) -> Result<Option<TradingDate>> {
        let entry = Instant::now(); self.neighbor(entry, date, include_self, true, query, options)
    }
    pub fn previous(&self, date: TradingDate, include_self: bool, query: &CalendarQuery, options: &OperationOptions) -> Result<Option<TradingDate>> {
        let entry = Instant::now(); self.neighbor(entry, date, include_self, false, query, options)
    }
    fn neighbor(&self, entry: Instant, date: TradingDate, include_self: bool, forward: bool, query: &CalendarQuery, options: &OperationOptions) -> Result<Option<TradingDate>> {
        let mut context = self.executor.begin_context_at(entry, if forward { "calendar_next" } else { "calendar_previous" }, &self.config, options)?;
        let snapshot = self.load(&mut context, query, false)?; snapshot.require_coverage(date)?;
        let index = if forward { snapshot.days().partition_point(|at| if include_self { *at < date } else { *at <= date }) }
            else { snapshot.days().partition_point(|at| if include_self { *at <= date } else { *at < date }) };
        let result = if forward { snapshot.days().get(index).copied() } else { index.checked_sub(1).and_then(|index| snapshot.days().get(index)).copied() };
        context.finish()?; Ok(result)
    }
    pub fn clear_cache(&self) { self.executor.owner().calendar().clear(); }
    fn load(&self, context: &mut OperationContext, query: &CalendarQuery, force: bool) -> Result<Arc<CalendarSnapshot>> {
        // Preserve the standalone K-line workload allowance before any child.
        context.set_work_limits(crate::operation::WorkLimits {
            pages: std::num::NonZeroUsize::new(query.pages().max_pages().saturating_sub(1)),
            records: Some(query.pages().max_records()), bytes: None })?;
        self.load_in_context(context, query, force)
    }
    pub(crate) fn load_in_context(&self, context: &OperationContext, query: &CalendarQuery, force: bool) -> Result<Arc<CalendarSnapshot>> {
        query.validate()?; context.check_boundary(TimeoutPhase::Admission)?;
        let today = self.today()?; let cache = self.executor.owner().calendar();
        if !force && !query.refresh() { if let Some(snapshot) = cache.get(query.benchmark(), today)? { return Ok(snapshot); } }
        let generation = cache.begin()?;
        let klines = KlineService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone());
        let query_bars = KlineQuery::new().with_period(KlinePeriod::Day)?.with_kind(KlineKind::Index)?
            .with_adjust(AdjustMode::None)?.with_pages(query.pages().clone())?;
        let series = klines.collect_with_parent_context(context, query.benchmark(), &query_bars)?;
        if series.bars().end() != EndEvidence::EmptyPage { return Err(Error::new(ErrorKind::MetadataUnavailable, "calendar refresh has no complete source evidence")); }
        let mut days = Vec::new(); days.try_reserve(series.bars().items().len()).map_err(crate::services::allocation_error)?;
        for bar in series.bars().items() { days.push(bar.source_time().date().ok_or_else(|| Error::new(ErrorKind::Protocol, "calendar daily bar has no date"))?); }
        let snapshot = Arc::new(CalendarSnapshot::new(query.benchmark().clone(), days, today, series.meta().clone()));
        context.check_boundary(TimeoutPhase::Aggregate)?;
        if self.today()? != today { return Err(Error::new(ErrorKind::MetadataUnavailable, "Shanghai date changed during calendar refresh")); }
        cache.publish(generation, snapshot.clone())?; Ok(snapshot)
    }
}
fn poisoned() -> Error { Error::new(ErrorKind::Internal, "calendar cache is poisoned") }

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Arc<CalendarSnapshot> {
        let date = TradingDate::new(2026, 1, 2).unwrap();
        Arc::new(CalendarSnapshot::new("sh000001".parse().unwrap(), vec![date], date, crate::types::ResultMeta::default()))
    }
    #[test]
    fn clear_and_newer_refresh_prevent_stale_publication_including_generation_exhaustion() {
        let cache = CalendarCache::default(); let snapshot = snapshot();
        let old = cache.begin().unwrap(); cache.clear(); cache.publish(old, snapshot.clone()).unwrap();
        assert!(cache.get(snapshot.benchmark(), snapshot.loaded_on()).unwrap().is_none());
        let old = cache.begin().unwrap(); let newer = cache.begin().unwrap();
        cache.publish(newer, snapshot.clone()).unwrap(); cache.publish(old, snapshot.clone()).unwrap();
        assert!(Arc::ptr_eq(&cache.get(snapshot.benchmark(), snapshot.loaded_on()).unwrap().unwrap(), &snapshot));
        cache.state.lock().unwrap().generation = u64::MAX - 1;
        let last = cache.begin().unwrap(); cache.clear(); cache.publish(last, snapshot.clone()).unwrap();
        assert!(cache.get(snapshot.benchmark(), snapshot.loaded_on()).unwrap().is_none());
        assert_eq!(cache.begin().unwrap_err().kind(), ErrorKind::LimitExceeded);
    }
}
