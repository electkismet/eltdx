//! Full-ladder topic ranking. Optional F10 failures stay visible per security.
use super::{HelperService, StockTopics, StockTopicsQuery};
use crate::services::{aggregate_meta, allocation_error};
use crate::types::{
    Amount, AmountUnit, Diagnostic, EnrichmentStatus, LadderQuery, NumericValue, ResultMeta,
    SecurityId, SecuritySelection, TradingDate,
};
use crate::{Error, ErrorKind, OperationOptions, Result, TimeoutPhase};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Instant;

/// Evaluation sequence shared with the Python object adapter. Each callback
/// evaluates its operands when reached, preserving live conversion hooks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeUpdate {
    Count,
    Level,
    Highest,
    Streak,
    Seal,
    Leader,
}
pub fn update_group<E>(
    mut apply: impl FnMut(ThemeUpdate) -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    for step in [
        ThemeUpdate::Count,
        ThemeUpdate::Level,
        ThemeUpdate::Highest,
        ThemeUpdate::Streak,
        ThemeUpdate::Seal,
        ThemeUpdate::Leader,
    ] {
        apply(step)?;
    }
    Ok(())
}
pub const RANK_FIELDS: &[&str] = &[
    "limit_up_count",
    "highest_ladder_level",
    "lianban_count",
    "total_seal_amount",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThemeStrengthQuery {
    ladder: LadderQuery,
    count: Option<usize>,
    topic_records: NonZeroUsize,
    groups: NonZeroUsize,
}
impl Default for ThemeStrengthQuery {
    fn default() -> Self {
        Self {
            ladder: LadderQuery::new(),
            count: None,
            topic_records: NonZeroUsize::new(20_000).unwrap_or(NonZeroUsize::MIN),
            groups: NonZeroUsize::new(20_000).unwrap_or(NonZeroUsize::MIN),
        }
    }
}
impl ThemeStrengthQuery {
    pub fn new() -> Self {
        Self::default()
    }
    /// A ladder's count is deliberately discarded: top-N applies after every
    /// contributing stock and topic has been observed.
    pub fn with_ladder(mut self, value: LadderQuery) -> Result<Self> {
        value.validate()?;
        self.ladder = value.with_count(None)?;
        Ok(self)
    }
    pub fn with_count(mut self, value: Option<usize>) -> Result<Self> {
        if value == Some(0) {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "theme count must be positive or None",
            ));
        }
        self.count = value;
        Ok(self)
    }
    pub fn with_max_topic_records(mut self, value: NonZeroUsize) -> Self {
        self.topic_records = value;
        self
    }
    pub fn with_max_groups(mut self, value: NonZeroUsize) -> Self {
        self.groups = value;
        self
    }
    pub fn ladder(&self) -> &LadderQuery {
        &self.ladder
    }
    pub fn count(&self) -> Option<usize> {
        self.count
    }
    pub fn max_topic_records(&self) -> NonZeroUsize {
        self.topic_records
    }
    pub fn max_groups(&self) -> NonZeroUsize {
        self.groups
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeStrengthRow {
    rank: usize,
    topic_id: Option<String>,
    topic_name: String,
    limit_up_count: usize,
    highest_ladder_level: u32,
    lianban_count: usize,
    total_seal_amount: Amount,
    leader: SecurityId,
    leader_ladder_level: Option<u32>,
}
impl ThemeStrengthRow {
    pub fn rank(&self) -> usize {
        self.rank
    }
    pub fn topic_id(&self) -> Option<&str> {
        self.topic_id.as_deref()
    }
    pub fn topic_name(&self) -> &str {
        &self.topic_name
    }
    pub fn limit_up_count(&self) -> usize {
        self.limit_up_count
    }
    pub fn highest_ladder_level(&self) -> u32 {
        self.highest_ladder_level
    }
    pub fn lianban_count(&self) -> usize {
        self.lianban_count
    }
    pub fn total_seal_amount(&self) -> Amount {
        self.total_seal_amount
    }
    pub fn leader(&self) -> &SecurityId {
        &self.leader
    }
    pub fn leader_ladder_level(&self) -> Option<u32> {
        self.leader_ladder_level
    }
}
#[derive(Clone, Debug)]
pub struct ThemeTopicSource {
    id: SecurityId,
    result: Option<StockTopics>,
    failure: Option<Error>,
}
impl ThemeTopicSource {
    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub fn result(&self) -> Option<&StockTopics> {
        self.result.as_ref()
    }
    pub fn failure(&self) -> Option<&Error> {
        self.failure.as_ref()
    }
}
#[derive(Clone, Debug)]
pub struct ThemeStrengthTable {
    trade_date: Option<TradingDate>,
    rows: Vec<ThemeStrengthRow>,
    sources: Vec<ThemeTopicSource>,
    meta: ResultMeta,
}
impl ThemeStrengthTable {
    pub fn trade_date(&self) -> Option<TradingDate> {
        self.trade_date
    }
    pub fn rows(&self) -> &[ThemeStrengthRow] {
        &self.rows
    }
    pub fn count(&self) -> usize {
        self.rows.len()
    }
    pub fn topic_sources(&self) -> &[ThemeTopicSource] {
        &self.sources
    }
    pub fn missing_ids(&self) -> impl Iterator<Item = &SecurityId> {
        self.sources
            .iter()
            .filter(|source| source.failure.is_some())
            .map(|source| &source.id)
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

struct Group {
    id: Option<String>,
    name: String,
    count: usize,
    highest: u32,
    streak: usize,
    seal: f64,
    leader: SecurityId,
    leader_level: Option<u32>,
}
impl Group {
    fn observe(
        &mut self,
        id: &SecurityId,
        ladder_level: Option<u32>,
        seal_amount: Option<Amount>,
    ) -> Result<()> {
        let mut level = 0;
        update_group(|step| {
            match step {
                ThemeUpdate::Count => {
                    self.count = self.count.checked_add(1).ok_or_else(overflow)?
                }
                ThemeUpdate::Level => level = ladder_level.unwrap_or(0),
                ThemeUpdate::Highest => self.highest = self.highest.max(level),
                ThemeUpdate::Streak => {
                    self.streak = self
                        .streak
                        .checked_add(usize::from(level >= 2))
                        .ok_or_else(overflow)?
                }
                ThemeUpdate::Seal => {
                    if let Some(amount) = seal_amount {
                        if amount.unit() != AmountUnit::Cny
                            || !amount.value().as_f64().is_finite()
                            || amount.value().as_f64() < 0.0
                        {
                            return Err(Error::new(
                                ErrorKind::MissingData,
                                "theme seal amount must have finite nonnegative CNY units",
                            ));
                        }
                        self.seal += amount.value().as_f64();
                        if !self.seal.is_finite() {
                            return Err(overflow());
                        }
                    }
                }
                ThemeUpdate::Leader => {
                    if self.count == 1 || level > self.leader_level.unwrap_or(0) {
                        self.leader = id.clone();
                        self.leader_level = ladder_level;
                    }
                }
            }
            Ok(())
        })
    }
}
fn overflow() -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "theme aggregate exceeds its finite representation",
    )
}
fn optional_topic_failure(
    context: &crate::operation::OperationContext,
    error: &Error,
) -> Result<()> {
    // Work exhaustion belongs to the complete composition, including HTTP
    // bytes already consumed by earlier optional securities.
    if error.kind() == ErrorKind::LimitExceeded {
        return Err(error.clone());
    }
    super::service::optional_failure(context, error)
}

impl HelperService {
    pub fn theme_strength_rank(
        &self,
        query: &ThemeStrengthQuery,
        options: &OperationOptions,
    ) -> Result<ThemeStrengthTable> {
        let mut context = self.executor.begin_context_at(
            Instant::now(),
            "helpers.theme_strength_rank",
            &self.config,
            options,
        )?;
        query.ladder.validate()?;
        let count = match query.ladder.selection() {
            SecuritySelection::Explicit(ids) => ids.len(),
            SecuritySelection::AllAShares => 65_535,
        };
        let mut limits = super::shortline::shortline_work_limits(
            count,
            matches!(query.ladder.selection(), SecuritySelection::AllAShares),
        )?;
        let topic_records = count
            .checked_mul(query.topic_records.get())
            .ok_or_else(overflow)?;
        limits.records = Some(
            limits
                .records
                .unwrap_or(0)
                .checked_add(topic_records)
                .ok_or_else(overflow)?,
        );
        limits.bytes = Some(self.f10.config().max_operation_bytes().get() as usize);
        context.set_work_limits(limits)?;
        let (trade_date, ladder) = self.ladder_in_context(&query.ladder, &context)?;
        let mut metas = vec![ladder.meta().clone()];
        let mut sources = Vec::new();
        let mut groups: Vec<Group> = Vec::new();
        let mut positions = HashMap::new();
        sources
            .try_reserve(ladder.rows().len())
            .map_err(allocation_error)?;
        let mut failures = Vec::new();
        for row in ladder.rows() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let query_topics = StockTopicsQuery::new(row.id().clone().into())
                .with_max_records(query.topic_records);
            let (result, failure) = match self.stock_topics_in_context(&query_topics, &context) {
                Ok(result) => {
                    metas.try_reserve(1).map_err(allocation_error)?;
                    metas.push(result.meta().clone());
                    let failure = (!result.ok()).then(|| {
                        Error::new(
                            ErrorKind::MissingData,
                            "optional F10 topic source reported a business error",
                        )
                        .with_context("security", format!("{}", row.id()))
                        .with_context("completion", "business_error")
                    });
                    (Some(result), failure)
                }
                Err(error) => {
                    optional_topic_failure(&context, &error)?;
                    (
                        None,
                        Some(error.with_context("security", format!("{}", row.id()))),
                    )
                }
            };
            if let Some(error) = &failure {
                failures.try_reserve(1).map_err(allocation_error)?;
                failures.push(Diagnostic::new(
                    "f10.topics",
                    "theme_strength_rank",
                    EnrichmentStatus::Failed,
                    Some(error.kind()),
                    Some(format!("{}: {}", row.id(), error.message())),
                ));
            } else if let Some(result) = &result {
                for topic in result.topics() {
                    context.check_boundary(TimeoutPhase::Aggregate)?;
                    let Some(name) = topic.topic_name() else {
                        continue;
                    };
                    let key = topic.topic_id().unwrap_or(name);
                    let index = if let Some(&index) = positions.get(key) {
                        index
                    } else {
                        if groups.len() >= query.groups.get() {
                            return Err(Error::new(
                                ErrorKind::LimitExceeded,
                                "theme group limit exceeded",
                            ));
                        }
                        groups.try_reserve(1).map_err(allocation_error)?;
                        positions.try_reserve(1).map_err(allocation_error)?;
                        let index = groups.len();
                        positions.insert(key.to_owned(), index);
                        groups.push(Group {
                            id: topic.topic_id().map(str::to_owned),
                            name: name.to_owned(),
                            count: 0,
                            highest: 0,
                            streak: 0,
                            seal: 0.0,
                            leader: row.id().clone(),
                            leader_level: None,
                        });
                        index
                    };
                    groups[index].observe(row.id(), row.ladder_level(), row.seal_amount())?;
                }
            }
            sources.push(ThemeTopicSource {
                id: row.id().clone(),
                result,
                failure,
            });
        }
        let rows = ranked_rows(groups, query.count, || {
            context.check_boundary(TimeoutPhase::Aggregate)
        })?;
        let mut meta = aggregate_meta(
            &context,
            &metas,
            context.metadata()?.map(|snapshot| snapshot.version()),
        )?;
        for failure in failures {
            meta = meta.with_diagnostic(failure);
        }
        context.finish()?;
        Ok(ThemeStrengthTable {
            trade_date,
            rows,
            sources,
            meta,
        })
    }
}

fn ranked_rows(
    mut groups: Vec<Group>,
    count: Option<usize>,
    mut boundary: impl FnMut() -> Result<()>,
) -> Result<Vec<ThemeStrengthRow>> {
    boundary()?;
    // Stable sort retains first topic discovery order for an exact tie.
    groups.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| right.highest.cmp(&left.highest))
            .then_with(|| right.streak.cmp(&left.streak))
            .then_with(|| right.seal.total_cmp(&left.seal))
    });
    boundary()?;
    if let Some(count) = count {
        groups.truncate(count);
    }
    let mut rows = Vec::new();
    rows.try_reserve(groups.len()).map_err(allocation_error)?;
    for (index, group) in groups.into_iter().enumerate() {
        boundary()?;
        rows.push(ThemeStrengthRow {
            rank: index + 1,
            topic_id: group.id,
            topic_name: group.name,
            limit_up_count: group.count,
            highest_ladder_level: group.highest,
            lianban_count: group.streak,
            total_seal_amount: Amount::new(NumericValue::F64(group.seal), AmountUnit::Cny),
            leader: group.leader,
            leader_ladder_level: group.leader_level,
        });
    }
    Ok(rows)
}

#[cfg(test)]
mod source_cases {
    use super::*;
    fn group(name: &str) -> Group {
        Group {
            id: Some(name.to_owned()),
            name: name.to_owned(),
            count: 0,
            highest: 0,
            streak: 0,
            seal: 0.0,
            leader: "sh600000".parse().unwrap(),
            leader_level: None,
        }
    }
    fn cny(value: f64) -> Amount {
        Amount::new(NumericValue::F64(value), AmountUnit::Cny)
    }
    #[test]
    fn aggregation_observes_every_occurrence_and_retains_first_maximum_leader() {
        let mut value = group("topic");
        value
            .observe(&"sh601001".parse().unwrap(), Some(2), Some(cny(1.0)))
            .unwrap();
        value
            .observe(&"sh600002".parse().unwrap(), Some(3), Some(cny(2.0)))
            .unwrap();
        value
            .observe(&"sh600003".parse().unwrap(), Some(3), None)
            .unwrap();
        value
            .observe(&"sh600004".parse().unwrap(), None, Some(cny(0.5)))
            .unwrap();
        assert_eq!(
            (value.count, value.highest, value.streak, value.seal),
            (4, 3, 3, 3.5)
        );
        assert_eq!(value.leader, "sh600002".parse().unwrap());
        assert_eq!(value.leader_level, Some(3));
    }
    #[test]
    fn ranking_uses_all_four_metrics_stable_ties_and_only_then_top_n() {
        let mut low = group("low");
        low.count = 1;
        low.highest = 99;
        let mut seal = group("seal");
        seal.count = 2;
        seal.highest = 2;
        seal.streak = 1;
        seal.seal = 100.0;
        let mut streak = group("streak");
        streak.count = 2;
        streak.highest = 2;
        streak.streak = 2;
        let mut highest = group("highest");
        highest.count = 2;
        highest.highest = 3;
        let mut tied = group("tied");
        tied.count = 2;
        tied.highest = 3;
        let rows = ranked_rows(vec![low, seal, streak, highest, tied], Some(4), || Ok(())).unwrap();
        assert_eq!(
            rows.iter().map(|row| row.topic_name()).collect::<Vec<_>>(),
            vec!["highest", "tied", "streak", "seal"]
        );
        assert_eq!(
            rows.iter().map(|row| row.rank()).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(rows[3].total_seal_amount().unit(), AmountUnit::Cny);
    }
    #[test]
    fn invalid_amounts_overflows_and_cancelled_ranking_never_produce_success() {
        for amount in [
            Amount::new(NumericValue::F64(1.0), AmountUnit::Unknown),
            cny(-1.0),
            cny(f64::INFINITY),
        ] {
            assert_eq!(
                group("x")
                    .observe(&"sh601001".parse().unwrap(), None, Some(amount))
                    .unwrap_err()
                    .kind(),
                ErrorKind::MissingData
            );
        }
        let mut value = group("overflow");
        value.count = usize::MAX;
        assert_eq!(
            value
                .observe(&"sh601001".parse().unwrap(), None, None)
                .unwrap_err()
                .kind(),
            ErrorKind::LimitExceeded
        );
        assert_eq!(
            ranked_rows(vec![group("x")], None, || Err(Error::new(
                ErrorKind::Cancelled,
                "stop"
            )))
            .unwrap_err()
            .kind(),
            ErrorKind::Cancelled
        );
    }
    #[test]
    fn optional_source_outage_may_degrade_but_terminal_errors_and_live_cancellation_may_not() {
        let client = crate::TdxClient::builder().build().unwrap();
        let helper = client.helpers();
        let context = helper
            .executor
            .begin_context_at(
                Instant::now(),
                "theme.source_case",
                &helper.config,
                &OperationOptions::default(),
            )
            .unwrap();
        assert!(optional_topic_failure(
            &context,
            &Error::new(ErrorKind::Connection, "optional outage")
        )
        .is_ok());
        for kind in [
            ErrorKind::LimitExceeded,
            ErrorKind::Cancelled,
            ErrorKind::CancelUnconfirmed,
            ErrorKind::Closed,
            ErrorKind::Internal,
        ] {
            assert_eq!(
                optional_topic_failure(&context, &Error::new(kind, "terminal"))
                    .unwrap_err()
                    .kind(),
                kind
            );
        }
        drop(context);
        let token = crate::CancellationToken::default();
        let options = OperationOptions::default().with_cancellation(token.clone());
        let context = helper
            .executor
            .begin_context_at(
                Instant::now(),
                "theme.source_case",
                &helper.config,
                &options,
            )
            .unwrap();
        token.cancel();
        assert_eq!(
            optional_topic_failure(
                &context,
                &Error::new(ErrorKind::Connection, "outage after cancellation")
            )
            .unwrap_err()
            .kind(),
            ErrorKind::Cancelled
        );
        // Closing waits for admitted operations. Release this admission first;
        // shared close itself is covered by the public-entry source cases.
        drop(context);
        client.close().unwrap();
    }
}
