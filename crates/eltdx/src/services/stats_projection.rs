//! Checked public projection, leaving legacy numeric/date values available for
//! the compatibility bridge. Optional conversion failures are explicit diagnostics.
use super::{allocation_error, identity_from_market};
use crate::operation::OperationContext;
use crate::stats::{ParsedStats, SourceInteger, StatsKey};
use crate::types::{
    Amount, Diagnostic, EnrichmentStatus, NumericValue, Quantity, QuantityUnit, ResultMeta,
    SecurityId, Stat2Row, StatRow, StatsResource, TradingDate, ValueProvenance,
};
use crate::{ErrorKind, Result, TimeoutPhase};

pub(crate) fn project(
    parsed: ParsedStats,
    mut meta: ResultMeta,
    context: &OperationContext,
) -> Result<StatsResource> {
    let mut details = Details {
        rows: Vec::new(),
        omitted: parsed.omitted_diagnostics,
    };
    details
        .rows
        .try_reserve_exact(128)
        .map_err(allocation_error)?;
    for diagnostic in parsed.diagnostics {
        context.check_boundary(TimeoutPhase::Decode)?;
        details.add(format!(
            "{}:{:?}:{:?}",
            diagnostic.member, diagnostic.line, diagnostic.kind
        ));
    }
    let mut stat = Vec::new();
    let mut stat2 = Vec::new();
    stat.try_reserve_exact(parsed.stat.len())
        .map_err(allocation_error)?;
    stat2
        .try_reserve_exact(parsed.stat2.len())
        .map_err(allocation_error)?;
    for row in parsed.stat {
        context.check_boundary(TimeoutPhase::Decode)?;
        let stats_date = date(row.stats_date.as_deref());
        if row.stats_date.is_some() && stats_date.is_none() {
            details.add(format!(
                "tdxstat.cfg:{}:invalid source date",
                row.source_line
            ));
        }
        let free_float_shares =
            row.free_float_shares_10k.and_then(
                |value| match Quantity::shares_from_ten_thousands_f64(
                    value,
                    ValueProvenance::WireRule,
                ) {
                    Ok(value) => Some(value),
                    Err(_) => {
                        details.add(format!(
                            "tdxstat.cfg:{}:shares conversion overflow",
                            row.source_line
                        ));
                        None
                    }
                },
            );
        let source_counts = [
            &row.year_limit_up_days,
            &row.limit_stat_days,
            &row.limit_up_count_in_stat_days,
            &row.limit_up_streak_days,
        ]
        .map(|value| value.as_ref().map(|value| value.decimal().to_owned()));
        let mut count = |value: &Option<SourceInteger>| {
            value.as_ref().and_then(|value| match value.as_u32() {
                Some(value) => Some(value),
                None => {
                    details.add(format!(
                        "tdxstat.cfg:{}:counter outside u32",
                        row.source_line
                    ));
                    None
                }
            })
        };
        stat.push(StatRow {
            id: identity(&row.key),
            source_market: row.key.market_id.decimal().to_owned(),
            source_code: row.key.code,
            stats_date,
            stats_date_raw: row.stats_date,
            beta_60d: row.beta_60d,
            pe_ttm: row.pe_ttm,
            free_float_shares,
            free_float_shares_source_10k: row.free_float_shares_10k,
            year_limit_up_days: count(&row.year_limit_up_days),
            limit_stat_days: count(&row.limit_stat_days),
            limit_up_count: count(&row.limit_up_count_in_stat_days),
            limit_streak: count(&row.limit_up_streak_days),
            source_line: row.source_line,
            source_counts,
        });
    }
    for row in parsed.stat2 {
        context.check_boundary(TimeoutPhase::Decode)?;
        let stats_date = date(row.stats_date.as_deref());
        if row.stats_date.is_some() && stats_date.is_none() {
            details.add(format!(
                "tdxstat2.cfg:{}:invalid source date",
                row.source_line
            ));
        }
        let source_values = [
            row.amount_10k,
            row.seal_amount_10k,
            row.prev_amount_10k,
            row.prev_seal_amount_10k,
            row.prev2_amount_10k,
            row.prev2_seal_amount_10k,
            row.open_volume_hand,
            row.prev_open_volume_hand,
            row.open_amount_10k,
            row.prev_open_amount_10k,
        ];
        let mut money = |value: Option<f64>| {
            value.and_then(|value| {
                match Amount::cny_from_ten_thousands(
                    NumericValue::F64(value),
                    ValueProvenance::WireRule,
                ) {
                    Ok(value) => Some(value),
                    Err(_) => {
                        details.add(format!(
                            "tdxstat2.cfg:{}:money conversion overflow",
                            row.source_line
                        ));
                        None
                    }
                }
            })
        };
        let volume = |value: Option<f64>| {
            value.map(|value| {
                Quantity::with_provenance(
                    NumericValue::F64(value),
                    QuantityUnit::Lots,
                    ValueProvenance::WireRule,
                )
            })
        };
        stat2.push(Stat2Row {
            id: identity(&row.key),
            source_market: row.key.market_id.decimal().to_owned(),
            source_code: row.key.code,
            stats_date,
            stats_date_raw: row.stats_date,
            amount: money(row.amount_10k),
            seal: money(row.seal_amount_10k),
            previous_amount: money(row.prev_amount_10k),
            previous_seal: money(row.prev_seal_amount_10k),
            previous2_amount: money(row.prev2_amount_10k),
            previous2_seal: money(row.prev2_seal_amount_10k),
            open_amount: money(row.open_amount_10k),
            previous_open_amount: money(row.prev_open_amount_10k),
            open_volume: volume(row.open_volume_hand),
            previous_open_volume: volume(row.prev_open_volume_hand),
            source_line: row.source_line,
            source_values,
        });
    }
    for message in details.rows {
        meta = meta.with_diagnostic(Diagnostic::new(
            "stats",
            "source_projection",
            EnrichmentStatus::Unavailable,
            Some(ErrorKind::Protocol),
            Some(message),
        ));
    }
    if details.omitted != 0 {
        meta = meta.with_diagnostic(Diagnostic::new(
            "stats",
            "source_projection",
            EnrichmentStatus::Unavailable,
            Some(ErrorKind::LimitExceeded),
            Some(format!(
                "{} additional diagnostics omitted",
                details.omitted
            )),
        ));
    }
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(StatsResource {
        stat,
        stat2,
        source_path: parsed.source_path,
        meta,
    })
}

struct Details {
    rows: Vec<String>,
    omitted: usize,
}
impl Details {
    fn add(&mut self, message: String) {
        if self.rows.len() < 128 {
            self.rows.push(message);
        } else {
            self.omitted = self.omitted.saturating_add(1);
        }
    }
}
fn identity(key: &StatsKey) -> Option<SecurityId> {
    let market = eltdx_protocol::unit::Market::from_id(i64::from(key.market_id.as_u32()?)).ok()?;
    identity_from_market(market, &key.code).ok()
}
fn date(value: Option<&str>) -> Option<TradingDate> {
    let value = value?;
    if value.len() != 8 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    TradingDate::new(
        value[..4].parse().ok()?,
        value[4..6].parse().ok()?,
        value[6..].parse().ok()?,
    )
    .ok()
}
