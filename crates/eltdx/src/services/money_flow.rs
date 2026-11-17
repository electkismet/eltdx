use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use super::money_flow_probe::ProbeOrder;
use super::{
    aggregate_meta, allocation_error, checked_ids, identity_from_market, result_meta, ServiceSource,
};
use crate::config::ClientConfig;
use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::hq::domains::{money_flow_hosts, retire_probe, DomainPools};
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions};
use crate::types::{
    Amount, AmountUnit, BatchItem, BatchItemState, BatchOutcome, DailyMoneyFlow, Diagnostic,
    EnrichmentStatus, MoneyFlowDay, NumericValue, Ratio, RatioUnit, SecurityId, TradingDate,
    ValueProvenance,
};
use eltdx_protocol::commands::money_flow::{MoneyFlowRecord, MoneyFlowRequest};
use eltdx_protocol::unit::NormalizedCode;
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::Engine;

fn fetch_money_flow_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<crate::hq::decode::DecodedReply>,
) -> Result<crate::hq::decode::DecodedReply> {
    if !matches!(&request, CommandRequest::MoneyFlow(_)) {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "money-flow service requires a money-flow request",
        ));
    }
    context.check_boundary(TimeoutPhase::Admission)?;
    let decoded = fetch(request)?;
    if !matches!(&decoded.response, CommandResponse::MoneyFlow(_)) {
        return Err(Error::new(
            ErrorKind::Protocol,
            "money-flow request returned a different response kind",
        ));
    }
    context.check_boundary(TimeoutPhase::Decode)?;
    Ok(decoded)
}

pub(crate) fn legacy_money_flow_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<crate::hq::decode::DecodedReply>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "legacy money flow requires a compatibility operation",
        ));
    }
    Ok(fetch_money_flow_reply(context, request, fetch)?.response)
}

#[derive(Clone, Debug)]
pub struct MoneyFlowService {
    executor: HqExecutor,
    pools: Arc<DomainPools>,
    config: ClientConfig,
}

impl MoneyFlowService {
    pub(crate) fn new(executor: HqExecutor, pools: Arc<DomainPools>, config: ClientConfig) -> Self {
        Self {
            executor,
            pools,
            config,
        }
    }

    pub fn daily(&self, id: &SecurityId, options: &OperationOptions) -> Result<DailyMoneyFlow> {
        let entry = Instant::now();
        let context =
            self.executor
                .begin_context_at(entry, "money_flow_daily", &self.config, options)?;
        checked_ids(std::slice::from_ref(id))?;
        let _admission = self.pools.money_flow_admission(&context)?;
        self.install_snapshot(&context)?;
        let engine = self.pools.money_flow(&self.config)?;
        let mut state = ProbeState::new(3);
        let value = self.daily_with(&context, &engine, id, 0, &mut state)?;
        context.finish()?;
        Ok(value)
    }

    pub fn daily_many(
        &self,
        ids: &[SecurityId],
        options: &OperationOptions,
    ) -> Result<BatchOutcome<DailyMoneyFlow>> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(
            entry,
            "money_flow_daily_many",
            &self.config,
            options,
        )?;
        if ids.is_empty() {
            context.finish()?;
            return Ok(BatchOutcome::new(Vec::new()));
        }
        checked_ids(ids)?;
        let _admission = self.pools.money_flow_admission(&context)?;
        self.install_snapshot(&context)?;
        let engine = self.pools.money_flow(&self.config)?;
        let mut state = ProbeState::new(ids.len().min(8) * 3);
        let mut items = Vec::new();
        let mut metas = Vec::new();
        items.try_reserve(ids.len()).map_err(allocation_error)?;
        metas.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut scopes = HashMap::<SecurityId, usize>::new();
        scopes.try_reserve(ids.len()).map_err(|error| {
            Error::new(
                ErrorKind::LimitExceeded,
                "unable to reserve money-flow probe identity scopes",
            )
            .with_source(error)
        })?;
        // Sequential scheduling respects every positive pool/owner budget.
        // Blocks never exceed the domain scheduling maximum of 75 identities.
        for (chunk_index, chunk) in ids.chunks(75).enumerate() {
            context.check_boundary(TimeoutPhase::Admission)?;
            for (within, id) in chunk.iter().enumerate() {
                let index = chunk_index
                    .checked_mul(75)
                    .and_then(|base| base.checked_add(within))
                    .ok_or_else(|| {
                        Error::new(ErrorKind::LimitExceeded, "money-flow batch index overflows")
                    })?;
                let scope = *scopes.entry(id.clone()).or_insert(index);
                let value = self.daily_with(&context, &engine, id, scope, &mut state)?;
                metas.push(value.meta().clone());
                items.push(BatchItem::new(
                    index,
                    id.clone(),
                    BatchItemState::Present(value),
                ));
            }
        }
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let value = BatchOutcome::with_meta(items, aggregate_meta(&context, &metas, version)?);
        context.finish()?;
        Ok(value)
    }

    fn install_snapshot(&self, context: &OperationContext) -> Result<()> {
        let namespace = format!("hq:money-flow:{}", money_flow_hosts(&self.config).join(","));
        context.install_metadata(context.owner().metadata().capture(&namespace, &[])?)
    }

    fn daily_with(
        &self,
        context: &OperationContext,
        engine: &Engine,
        id: &SecurityId,
        scope: usize,
        state: &mut ProbeState,
    ) -> Result<DailyMoneyFlow> {
        let initial = self.request(context, engine, id, None)?;
        if !initial.records().is_empty() {
            return Ok(initial);
        }
        let mut metas = vec![initial.meta().clone()];
        let mut candidates = money_flow_hosts(&self.config);
        // A host's empty answer applies to this security only.
        let primary = initial
            .meta()
            .source()
            .endpoint()
            .map(|host| host.to_string());
        let mut unique = HashSet::new();
        candidates.retain(|host| {
            Some(host.as_str()) != primary.as_deref() && unique.insert(host.clone())
        });
        let start = if !candidates.is_empty() {
            let start = state.cursor % candidates.len();
            state.cursor = state
                .cursor
                .checked_add(1)
                .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "probe cursor overflows"))?;
            start
        } else {
            0
        };
        let mut source_error: Option<Error> = None;
        for index in ProbeOrder::new(candidates.len(), start, candidates.len()) {
            let endpoint = &candidates[index];
            context.check_boundary(TimeoutPhase::Admission)?;
            // The physical attempt reservation atomically debits code, batch and
            // global counters. These reads only avoid knowingly exhausted work.
            if context.budget.probe_started(scope)? >= 3
                || context.budget.recovery_started("money_flow_probes")? >= state.maximum
            {
                break;
            }
            let probe = self
                .pools
                .money_probe(&self.executor, engine, &self.config, endpoint)?;
            let requested = self.request(context, &probe, id, Some((state.maximum, scope)));
            match retire_probe(&self.executor, context, &self.config, &probe, requested) {
                Ok(value) => {
                    metas.push(value.meta().clone());
                    if !value.records().is_empty() {
                        let mut meta = aggregate_meta(
                            context,
                            &metas,
                            value.meta().source().metadata_version(),
                        )?;
                        if let Some(error) = source_error {
                            meta = meta.with_diagnostic(source_diagnostic(&error));
                        }
                        return Ok(value.with_meta(meta));
                    }
                }
                Err(error) => {
                    // Connection failure can try another bounded candidate. A
                    // corrupt block or operation stop cannot become empty success.
                    if error.kind() != ErrorKind::Connection {
                        return Err(error);
                    }
                    context.remember_recoverable(error.clone());
                    source_error = Some(error);
                }
            }
        }
        if let Some(error) = source_error {
            return Err(error);
        }
        let meta = aggregate_meta(context, &metas, initial.meta().source().metadata_version())?
            .with_diagnostic(Diagnostic::new("money_flow", "empty_probe", EnrichmentStatus::Unavailable,
                None, Some("validated source responses contain no records; bounded alternate-host policy completed".to_owned())));
        Ok(initial.with_meta(meta))
    }

    fn request(
        &self,
        context: &OperationContext,
        engine: &Engine,
        id: &SecurityId,
        probe: Option<(usize, usize)>,
    ) -> Result<DailyMoneyFlow> {
        let code = NormalizedCode::parse(&id.to_string()).map_err(Error::from_protocol)?;
        let request = CommandRequest::MoneyFlow(MoneyFlowRequest::new(code));
        let decoded = fetch_money_flow_reply(context, request, |request| {
            let raw = match probe {
                Some((maximum, scope)) => self
                    .executor
                    .execute_probe_raw(engine, context, request, maximum, scope)?,
                None => self.executor.execute_raw(
                    engine,
                    context,
                    request,
                    ChildPurpose::Required,
                    None,
                )?,
            };
            self.executor.decode_received(context, raw)
        })?;
        let CommandResponse::MoneyFlow(response) = &decoded.response else {
            return Err(Error::new(
                ErrorKind::Protocol,
                "money-flow request returned a different response kind",
            ));
        };
        if response.blocks.len() != 1 {
            return Err(Error::new(
                ErrorKind::Protocol,
                "single-identity money-flow response must contain exactly one block",
            ));
        }
        let block = &response.blocks[0];
        let market = block
            .market
            .ok_or_else(|| Error::new(ErrorKind::Protocol, "unknown money-flow response market"))?;
        if identity_from_market(market, &block.code)? != *id {
            return Err(Error::new(
                ErrorKind::Protocol,
                "money-flow response identity differs from request",
            ));
        }
        context.record_work(block.records.len(), 0)?;
        let mut records = Vec::new();
        records
            .try_reserve(block.records.len())
            .map_err(allocation_error)?;
        let mut missing_date = false;
        for row in &block.records {
            let day = project_day(row)?;
            missing_date |= day.date().is_none();
            records.push(day);
        }
        let ranges = block
            .records
            .iter()
            .map(|row| row.record_range)
            .collect::<Vec<_>>();
        let snapshot = context.metadata()?;
        let mut meta = result_meta(
            context,
            &self.config,
            snapshot.as_deref(),
            &ServiceSource::from_reply(&decoded),
            Some(decoded.payload.clone()),
            decoded.wire.as_ref().map(|wire| wire.raw.clone()),
            &ranges,
            "money_flow.daily",
        );
        meta = meta.with_diagnostic(Diagnostic::new("money_flow", "amount_units", EnrichmentStatus::Unavailable,
            None, Some("source amount unit is unestablished; wire values and percentage-point ratios retained".to_owned())));
        if missing_date {
            meta = meta.with_diagnostic(Diagnostic::new(
                "money_flow",
                "record_date",
                EnrichmentStatus::Unavailable,
                None,
                Some(
                    "one or more date scalars are not valid Gregorian dates; date_raw retained"
                        .to_owned(),
                ),
            ));
        }
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(DailyMoneyFlow::new(id.clone(), records, meta))
    }
}

struct ProbeState {
    maximum: usize,
    cursor: usize,
}
impl ProbeState {
    fn new(maximum: usize) -> Self {
        Self { maximum, cursor: 0 }
    }
}

fn source_diagnostic(error: &Error) -> Diagnostic {
    Diagnostic::new(
        "money_flow",
        "alternate_source",
        EnrichmentStatus::Failed,
        Some(error.kind()),
        Some(error.to_string()),
    )
}

fn project_day(row: &MoneyFlowRecord) -> Result<MoneyFlowDay> {
    let main = [
        row.main_super_large_net,
        row.main_large_net,
        row.main_medium_net,
        row.main_small_net,
    ];
    let main_buy = [
        row.main_buy_super_large_net,
        row.main_buy_large_net,
        row.main_buy_medium_net,
        row.main_buy_small_net,
    ];
    if !row.total_amount.is_finite()
        || !row.main_net.is_finite()
        || !row.main_buy_net.is_finite()
        || !row.main_ratio.is_finite()
        || !row.main_buy_ratio.is_finite()
        || main
            .iter()
            .chain(main_buy.iter())
            .any(|value| !value.is_finite())
    {
        return Err(Error::new(
            ErrorKind::Protocol,
            "money-flow response has a non-finite required amount or ratio",
        ));
    }
    let date = row.date.and_then(|date| {
        u16::try_from(date.year)
            .ok()
            .and_then(|year| TradingDate::new(year, date.month, date.day).ok())
    });
    let amount = |value| {
        Amount::with_provenance(
            NumericValue::F64(value),
            AmountUnit::Unknown,
            ValueProvenance::WireRule,
        )
    };
    let ratio = |value| {
        Ratio::with_provenance(
            NumericValue::F64(value),
            RatioUnit::PercentagePoints,
            ValueProvenance::WireRule,
        )
    };
    Ok(MoneyFlowDay {
        date,
        date_raw: row.date_raw,
        total_amount: Amount::with_provenance(
            NumericValue::F32(row.total_amount),
            AmountUnit::Unknown,
            ValueProvenance::WireRule,
        ),
        buckets: row.buckets,
        raw_values: row.raw,
        main_net: amount(row.main_net),
        main_buy_net: amount(row.main_buy_net),
        main_ratio: ratio(row.main_ratio),
        main_buy_ratio: ratio(row.main_buy_ratio),
        main_size_net: main.map(amount),
        main_buy_size_net: main_buy.map(amount),
    })
}
