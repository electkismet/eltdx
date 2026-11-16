//! Required statistics evidence shared by daily capital and shortline helpers.
use std::collections::HashMap;
use std::sync::Arc;

use crate::calendar::CalendarService;
use crate::operation::OperationContext;
use crate::services::{allocation_error, ResourceService};
use crate::types::{CalendarQuery, CalendarSnapshot, StatsQuery, StatsResource, TradingDate};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use super::cache::{CachedSource, StatsCacheKey};
use super::service::HelperService;

impl HelperService {
    pub(super) fn calendar_in_context(&self, context: &OperationContext) -> Result<Arc<CalendarSnapshot>> {
        CalendarService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone())
            .load_in_context(context, &CalendarQuery::new(), false)
    }

    pub(super) fn previous_in_calendar(&self, snapshot: &CalendarSnapshot, target: TradingDate) -> Result<TradingDate> {
        snapshot.require_coverage(target)?;
        let index = snapshot.days().partition_point(|date| *date < target);
        index.checked_sub(1).and_then(|index| snapshot.days().get(index)).copied()
            .ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "calendar has no observed previous trading day")
                .with_context("target", format!("{target}")))
    }

    /// Only a successfully date-validated resource can be reused. A refresh
    /// failure remains an error; an older cache is not silently marked fresh.
    pub(super) fn aligned_stats_in_context(&self, context: &OperationContext, query: &StatsQuery,
        refresh: bool, target: TradingDate, previous: TradingDate) -> Result<(Arc<StatsResource>, TradingDate, bool)> {
        query.validate()?; context.check_boundary(TimeoutPhase::Admission)?;
        let key = StatsCacheKey { query: query.clone(), target, previous };
        let stamp = self.cache_stamp(context, None)?;
        let cache = context.owner().helpers();
        let (generation, hit) = cache.stats(&key, stamp)?;
        if !refresh {
            if let Some(hit) = hit {
                let resource = hit.result?;
                let date = validate_dates(&resource, target, previous, || context.check_boundary(TimeoutPhase::Aggregate))?;
                return Ok((resource, date, false));
            }
        }
        let service = ResourceService::new(self.executor.clone(), self.engine.clone(), self.config.clone());
        let resource = Arc::new(service.read_stats_in_context(context, query)?);
        let date = validate_dates(&resource, target, previous, || context.check_boundary(TimeoutPhase::Aggregate))?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        cache.publish_stats(generation, key, CachedSource::new(Ok(resource.clone()), stamp, true)?)?;
        Ok((resource, date, true))
    }
}

pub(super) fn validate_dates(resource: &StatsResource, target: TradingDate, previous: TradingDate,
    mut boundary: impl FnMut() -> Result<()>) -> Result<TradingDate> {
    let stat = main_date(resource.stat().iter().map(|row| row.stats_date()), "tdxstat.cfg", &mut boundary)?;
    let stat2 = main_date(resource.stat2().iter().map(|row| row.stats_date()), "tdxstat2.cfg", &mut boundary)?;
    if stat != stat2 {
        return Err(format_error("the two statistics files have different main dates")
            .with_context("stat_date", format!("{stat}")).with_context("stat2_date", format!("{stat2}")));
    }
    if stat != target && stat != previous {
        return Err(date_error("statistics main date is neither target nor previous trading day")
            .with_context("stats_date", format!("{stat}")).with_context("target", format!("{target}"))
            .with_context("previous", format!("{previous}")));
    }
    boundary()?; Ok(stat)
}

fn main_date(dates: impl Iterator<Item = Option<TradingDate>>, member: &str,
    boundary: &mut impl FnMut() -> Result<()>) -> Result<TradingDate> {
    let mut counts = HashMap::<TradingDate, usize>::new(); let mut total = 0_usize;
    for date in dates {
        boundary()?;
        total = total.checked_add(1).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "statistics row count overflows"))?;
        if let Some(date) = date {
            if !counts.contains_key(&date) { counts.try_reserve(1).map_err(allocation_error)?; }
            *counts.entry(date).or_default() += 1;
        }
    }
    let (date, count) = counts.into_iter().max_by_key(|(date, count)| (*count, *date))
        .ok_or_else(|| format_error("statistics file has no valid dated rows").with_context("member", member))?;
    // Inclusive 95%, checked with integer arithmetic: invalid/missing dates
    // remain in the denominator rather than artificially improving coverage.
    if (count as u128) * 100 < (total as u128) * 95 {
        return Err(format_error("statistics main-date coverage is below 95 percent")
            .with_context("member", member).with_context("main_count", format!("{count}"))
            .with_context("row_count", format!("{total}")));
    }
    boundary()?; Ok(date)
}
fn format_error(message: &str) -> Error { Error::new(ErrorKind::ResourceFormat, message).with_context("domain", "statistics") }
fn date_error(message: &str) -> Error { format_error(message).with_context("code", "stats_date_mismatch") }

#[cfg(test)]
mod source_cases {
    use super::*;
    #[test]
    fn main_date_requires_inclusive_95_percent_in_each_file_and_counts_undated_rows() {
        let day = TradingDate::new(2026, 11, 16).unwrap();
        let good = std::iter::repeat_n(Some(day), 19).chain(std::iter::once(None));
        assert_eq!(main_date(good, "test", &mut || Ok(())).unwrap(), day);
        let bad = std::iter::repeat_n(Some(day), 18).chain(std::iter::repeat_n(None, 2));
        assert_eq!(main_date(bad, "test", &mut || Ok(())).unwrap_err().kind(), ErrorKind::ResourceFormat);
        assert_eq!(main_date([None, None].into_iter(), "test", &mut || Ok(())).unwrap_err().kind(), ErrorKind::ResourceFormat);
        assert_eq!(main_date([Some(day)].into_iter(), "test", &mut || Err(Error::new(ErrorKind::Cancelled, "stop")))
            .unwrap_err().kind(), ErrorKind::Cancelled);
    }
}
