use std::sync::Arc;
use std::time::Instant;
use eltdx_protocol::commands::corporate::{CapitalChangesRequest, FinanceBatchRequest};
use eltdx_protocol::unit::NormalizedCode;
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::Engine;
use crate::config::ClientConfig;
use crate::hq::{decode::DecodedReply, executor::HqExecutor};
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, RecoveryLimit, WorkLimits};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use crate::types::{AdjustmentFactors, Amount, BatchItem, BatchItemState, BatchOutcome, CapitalChangeBlock,
    CapitalEvent, CapitalQuery, FactorQuery, FinanceField, FinanceProjection, FinanceQuery, FinanceRecord,
    FinanceValue, MissingPolicy, MissingReason, Quantity, ResultMeta, SecurityId, ValueProvenance};
use super::{aggregate_meta, allocation_error, checked_ids, identity_from_market, result_meta, ServiceSource};
use super::recovery::{ordered_tree, RecoveryStep};

const FINANCE_RECOVERY: RecoveryLimit = RecoveryLimit { name: "finance_missing", maximum: 32 };
const FINANCE_CHUNK: usize = 300;

#[derive(Clone, Debug)]
pub struct CorporateService { executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String }
impl CorporateService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig, namespace: String) -> Self {
        Self { executor, engine, config, namespace }
    }
    pub fn capital_changes(&self, ids: &[SecurityId], query: &CapitalQuery, options: &OperationOptions) -> Result<BatchOutcome<CapitalChangeBlock>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "capital_changes", &self.config, options)?;
        let result = self.capital_in_context(&mut context, ids, query)?;
        context.finish()?; Ok(result)
    }
    pub fn adjustment_factors(&self, ids: &[SecurityId], query: &FactorQuery, options: &OperationOptions) -> Result<BatchOutcome<AdjustmentFactors>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "adjustment_factors", &self.config, options)?;
        query.validate()?;
        let changes = self.capital_in_context(&mut context, ids, query.capital())?;
        let mut items = Vec::new(); items.try_reserve(changes.len()).map_err(allocation_error)?;
        for item in changes.items() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let state = match item.state() {
                BatchItemState::Present(block) => BatchItemState::Present(crate::adjustment::build_with_boundary(block, query,
                    || context.check_boundary(TimeoutPhase::Aggregate))?),
                BatchItemState::ConfirmedMissing(reason) => BatchItemState::ConfirmedMissing(reason.clone()),
            };
            items.push(BatchItem::new(item.input_index(), item.security().clone(), state));
        }
        context.finish()?; Ok(BatchOutcome::with_meta(items, changes.meta().clone()))
    }
    pub fn finance(&self, ids: &[SecurityId], query: &FinanceQuery, options: &OperationOptions) -> Result<BatchOutcome<FinanceRecord>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "finance", &self.config, options)?;
        let result = self.finance_in_context(&mut context, ids, query)?;
        context.finish()?; Ok(result)
    }
    pub fn finance_fields(&self, ids: &[SecurityId], fields: &[FinanceField], query: &FinanceQuery,
        options: &OperationOptions) -> Result<BatchOutcome<FinanceProjection>> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "finance_fields", &self.config, options)?;
        if fields.len() > 65_535 { return Err(Error::new(ErrorKind::LimitExceeded, "too many finance projection fields")); }
        let projected = ids.len().checked_mul(fields.len()).ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "finance projection size overflows"))?;
        if projected > 2_097_152 { return Err(Error::new(ErrorKind::LimitExceeded, "finance projection exceeds 2097152 cells")); }
        let records = self.finance_in_context(&mut context, ids, query)?;
        let mut items = Vec::new(); items.try_reserve(records.len()).map_err(allocation_error)?;
        for item in records.items() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let state = match item.state() {
                BatchItemState::Present(record) => BatchItemState::Present(record.project(fields)?),
                BatchItemState::ConfirmedMissing(reason) => BatchItemState::ConfirmedMissing(reason.clone()),
            };
            items.push(BatchItem::new(item.input_index(), item.security().clone(), state));
        }
        context.finish()?; Ok(BatchOutcome::with_meta(items, records.meta().clone()))
    }

    pub(crate) fn capital_in_context(&self, context: &mut OperationContext, ids: &[SecurityId], query: &CapitalQuery) -> Result<BatchOutcome<CapitalChangeBlock>> {
        query.validate()?; let codes = checked_ids(ids)?;
        if codes.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        context.set_work_limits(WorkLimits { pages: std::num::NonZeroUsize::new(ids.len()), records: Some(ids.len()), bytes: None })?;
        self.capital_collect_in_context(context, ids, query)
    }

    pub(crate) fn capital_collect_in_context(&self, context: &OperationContext, ids: &[SecurityId], query: &CapitalQuery)
        -> Result<BatchOutcome<CapitalChangeBlock>> {
        context.check_boundary(TimeoutPhase::Admission)?;
        query.validate()?; let codes = checked_ids(ids)?;
        if codes.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        // This wire domain needs no security-table precision. Freeze the
        // operation's existing metadata instead of introducing catalog I/O.
        super::capture_existing_metadata(context, &self.namespace, ids)?;
        let mut items = Vec::new(); items.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut metas = Vec::new(); let mut page_index = 0;
        for chunk in codes.chunks(query.batch_size()) {
            let mut consumed = 0;
            while consumed < chunk.len() {
                context.check_boundary(TimeoutPhase::Admission)?;
                let remaining = &chunk[consumed..];
                let request = CommandRequest::CapitalChanges(CapitalChangesRequest::new_batch(remaining.to_vec()).map_err(Error::from_protocol)?);
                let raw = self.executor.execute_raw(&self.engine, context, request,
                    if page_index == 0 { ChildPurpose::Required } else { ChildPurpose::Page }, None)?;
                let decoded = self.executor.decode_received(context, raw)?;
                let CommandResponse::CapitalChanges(page) = &decoded.response else { return Err(wrong("capital changes")); };
                if page.blocks.is_empty() || page.blocks.len() > remaining.len() { return Err(wrong("empty or excess capital prefix")); }
                for (expected, block) in remaining.iter().zip(&page.blocks) {
                    let actual = strict_id(block.market, &block.code)?;
                    if actual != identity_from_market(expected.market(), expected.number())? { return Err(wrong("capital prefix identity")); }
                    for event in &block.records {
                        if event.market_id != block.market_id || event.code != block.code { return Err(wrong("capital event identity")); }
                    }
                }
                let ranges = page.blocks.iter().flat_map(|block| block.records.iter().map(|event| event.record_range)).collect::<Vec<_>>();
                let meta = self.meta(context, &decoded, &ranges, "corporate.capital")?;
                for block in &page.blocks {
                    let index = items.len(); let id = ids[index].clone();
                    let mut events = Vec::new(); events.try_reserve(block.records.len()).map_err(allocation_error)?;
                    for event in &block.records {
                        events.push(CapitalEvent::from_wire(event.date_raw, event.category_raw,
                            [event.c1_raw, event.c2_raw, event.c3_raw, event.c4_raw], event.reserved_7).with_name(event.category_name));
                    }
                    let row_meta = self.meta(context, &decoded, &block.records.iter().map(|event| event.record_range).collect::<Vec<_>>(), "corporate.capital")?;
                    items.push(BatchItem::new(index, id.clone(), BatchItemState::Present(
                        CapitalChangeBlock::new(id, events, page.declared_blocks, row_meta))));
                }
                context.record_work(page.blocks.len(), 0)?;
                consumed += page.blocks.len(); page_index += 1; metas.push(meta);
            }
        }
        Ok(BatchOutcome::with_meta(items, aggregate_meta(context, &metas, context.metadata()?.as_deref().map(|snapshot| snapshot.version()))?))
    }

    pub(crate) fn finance_in_context(&self, context: &mut OperationContext, ids: &[SecurityId], query: &FinanceQuery) -> Result<BatchOutcome<FinanceRecord>> {
        let codes = checked_ids(ids)?;
        if codes.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        context.set_work_limits(WorkLimits { pages: std::num::NonZeroUsize::new(ids.len().div_ceil(FINANCE_CHUNK)), records: Some(ids.len()), bytes: None })?;
        self.finance_collect_in_context(context, ids, query, FINANCE_CHUNK, ChildPurpose::Required)
    }

    /// A composition freezes its aggregate limits before its first child.
    /// This collector never resets those limits or finishes its parent context.
    pub(crate) fn finance_collect_in_context(&self, context: &OperationContext, ids: &[SecurityId],
        query: &FinanceQuery, chunk_size: usize, first_purpose: ChildPurpose) -> Result<BatchOutcome<FinanceRecord>> {
        if !(1..=FINANCE_CHUNK).contains(&chunk_size) {
            return Err(Error::new(ErrorKind::InvalidArgument, "finance chunk size must be between 1 and 300"));
        }
        let codes = checked_ids(ids)?;
        if codes.is_empty() { return Ok(BatchOutcome::new(Vec::new())); }
        super::capture_existing_metadata(context, &self.namespace, ids)?;
        let mut first = true; let mut slots = Vec::new(); let mut metas = Vec::new();
        slots.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut execute = |part: &[NormalizedCode], recovery: bool| {
            let purpose = if recovery { ChildPurpose::MissingRecovery } else if first { first = false; first_purpose } else { ChildPurpose::Page };
            let request = CommandRequest::FinanceBatch(FinanceBatchRequest::new(part.to_vec()).map_err(Error::from_protocol)?);
            let raw = self.executor.execute_raw(&self.engine, context, request, purpose, recovery.then_some(FINANCE_RECOVERY))?;
            self.executor.decode_received(context, raw)
        };
        for chunk in codes.chunks(chunk_size) {
            let fetched = ordered_tree(context, chunk, false,
                &mut |part, recovery| finance_step(context, part, recovery, &mut execute), &merge_finance)?;
            for reply in &fetched.replies {
                let ranges = match &reply.response { CommandResponse::FinanceBatch(page) => page.records.iter().map(|row| row.record_range).collect(), _ => Vec::new() };
                metas.push(self.meta(context, reply, &ranges, "corporate.finance")?);
            }
            slots.extend(fetched.slots);
        }
        let mut items = Vec::new(); items.try_reserve(ids.len()).map_err(allocation_error)?;
        let mut missing = Vec::new();
        for (index, (id, slot)) in ids.iter().zip(slots).enumerate() {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let state = match slot {
                Some((row, reply)) => {
                    let meta = self.meta(context, &reply, &[row.record_range], "corporate.finance")?;
                    let projected = project_finance(&row, id.clone(), meta).map_err(|error|
                        error.with_context("endpoint", reply.source.host.clone()).with_context("security", format!("{id}")))?;
                    BatchItemState::Present(projected)
                }
                None => { missing.push(index); BatchItemState::ConfirmedMissing(MissingReason::NotFound) }
            };
            items.push(BatchItem::new(index, id.clone(), state));
        }
        if query.missing() == MissingPolicy::Error && !missing.is_empty() {
            let mut error = Error::new(ErrorKind::MissingData, "one or more requested finance records are missing").with_context("command", "0x0010");
            for index in missing { error = error.with_context(format!("missing.{index}.security"), format!("{}", ids[index]))
                .with_context(format!("missing.{index}.input_index"), format!("{index}")).with_context(format!("missing.{index}.reason"), "NotFound"); }
            return Err(error);
        }
        Ok(BatchOutcome::with_meta(items, aggregate_meta(context, &metas, context.metadata()?.as_deref().map(|snapshot| snapshot.version()))?))
    }

    fn meta(&self, context: &OperationContext, reply: &DecodedReply, ranges: &[eltdx_protocol::RecordRange], source: &str) -> Result<ResultMeta> {
        Ok(result_meta(context, &self.config, context.metadata()?.as_deref(), &ServiceSource::from_reply(reply),
            Some(reply.payload.clone()), reply.wire.as_ref().map(|wire| wire.raw.clone()), ranges, source))
    }
}

type WireFinance = eltdx_protocol::commands::corporate::FinanceRecord;
struct FinanceRows { slots: Vec<Option<(WireFinance, Arc<DecodedReply>)>>, replies: Vec<Arc<DecodedReply>> }
fn finance_step(context: &OperationContext, codes: &[NormalizedCode], _recovery: bool,
    execute: &mut impl FnMut(&[NormalizedCode], bool) -> Result<DecodedReply>) -> Result<RecoveryStep<FinanceRows>> {
    let decoded = match execute(codes, _recovery) {
        Ok(reply) => reply,
        Err(error) => {
            if codes.len() <= 1 || !recoverable_finance(&error) { return Err(error); }
            context.remember_recoverable(error); return Ok(RecoveryStep::Split);
        }
    };
    let CommandResponse::FinanceBatch(page) = &decoded.response else { return Err(wrong("finance")); };
    if page.records.is_empty() && codes.len() == 1 {
        return Ok(RecoveryStep::Complete(FinanceRows { slots: vec![None], replies: vec![Arc::new(decoded)] }));
    }
    if page.records.len() != codes.len() {
        if codes.len() <= 1 || page.records.len() > codes.len() { return Err(wrong("finance count")); }
        context.remember_recoverable(wrong("incomplete finance batch")); return Ok(RecoveryStep::Split);
    }
    let ids = codes.iter().map(|code| identity_from_market(code.market(), code.number())).collect::<Result<Vec<_>>>()?;
    let order = super::quotes::occurrence_order(&ids, page.records.iter().map(|row| strict_id(row.market, &row.code)))?;
    let reply = Arc::new(decoded);
    let CommandResponse::FinanceBatch(page) = &reply.response else { unreachable!() };
    let mut slots = Vec::new(); slots.try_reserve(order.len()).map_err(allocation_error)?;
    for index in order { slots.push(Some((page.records[index].clone(), reply.clone()))); }
    context.record_work(slots.len(), 0)?;
    Ok(RecoveryStep::Complete(FinanceRows { slots, replies: vec![reply] }))
}
fn merge_finance(mut first: FinanceRows, second: FinanceRows) -> Result<FinanceRows> {
    first.slots.try_reserve(second.slots.len()).map_err(allocation_error)?;
    first.replies.try_reserve(second.replies.len()).map_err(allocation_error)?;
    first.slots.extend(second.slots); first.replies.extend(second.replies); Ok(first)
}
fn recoverable_finance(error: &Error) -> bool {
    std::error::Error::source(error).and_then(|source| source.downcast_ref::<eltdx_protocol::ProtocolError>())
        .is_some_and(|source| matches!(source, eltdx_protocol::ProtocolError::InvalidData { context: "finance batch", message }
            if message.starts_with("finance batch missing record:") || message.starts_with("invalid finance batch length:")))
}
fn strict_id(market: Option<eltdx_protocol::unit::Market>, code: &str) -> Result<SecurityId> {
    identity_from_market(market.ok_or_else(|| wrong("unknown corporate market"))?, code)
}

fn project_finance(row: &WireFinance, id: SecurityId, meta: ResultMeta) -> Result<FinanceRecord> {
    let raw = [row.liu_tong_gu_ben_raw_float, row.zong_gu_ben_raw_float, row.guo_jia_gu_raw_float,
        row.fa_qi_ren_fa_ren_gu_raw_float, row.fa_ren_gu_raw_float, row.b_gu_raw_float, row.h_gu_raw_float, row.eps_raw,
        row.zong_zi_chan_raw_float, row.liu_dong_zi_chan_raw_float, row.gu_ding_zi_chan_raw_float, row.wu_xing_zi_chan_raw_float,
        row.gu_dong_ren_shu_raw_float, row.liu_dong_fu_zhai_raw_float, row.chang_qi_fu_zhai_raw_float,
        row.zi_ben_gong_ji_jin_raw_float, row.jing_zi_chan_raw_float, row.zhu_ying_shou_ru_raw_float,
        row.zhu_ying_li_run_raw_float, row.ying_shou_zhang_kuan_raw_float, row.ying_ye_li_run_raw_float,
        row.tou_zi_shou_yu_raw_float, row.jing_ying_xian_jin_liu_raw_float, row.zong_xian_jin_liu_raw_float,
        row.cun_huo_raw_float, row.li_run_zong_he_raw_float, row.shui_hou_li_run_raw_float, row.jing_li_run_raw_float,
        row.wei_fen_li_run_raw_float, row.mei_gu_jing_zi_chan_raw_float, row.bao_liu_2_raw_float];
    let mut values = Vec::new(); values.try_reserve(FinanceField::ALL.len()).map_err(allocation_error)?;
    for (field, value) in FinanceField::ALL.into_iter().zip(raw) {
        let projected = match field {
            FinanceField::CirculatingShares | FinanceField::TotalShares => {
                require_finite(value, field)?;
                FinanceValue::Shares(Quantity::shares_from_ten_thousands(value, ValueProvenance::WireRule)?)
            }
            FinanceField::TotalAssets | FinanceField::NetProfit => {
                require_finite(value, field)?;
                FinanceValue::Money(Amount::cny_from_thousands(value, ValueProvenance::WireRule)?)
            }
            _ => FinanceValue::Unclassified { value, bits: value.to_le_bytes() },
        };
        values.push((field, projected));
    }
    let date = |parts: eltdx_protocol::unit::DateParts| crate::TradingDate::new(
        u16::try_from(parts.year).map_err(|_| wrong("finance date year"))?, parts.month, parts.day);
    Ok(FinanceRecord::new(id, row.province_raw, row.industry_raw, row.updated_date_raw, row.ipo_date_raw,
        row.updated_date.map(date).transpose()?, row.ipo_date.map(date).transpose()?, values, meta))
}
fn require_finite(value: f32, field: FinanceField) -> Result<()> {
    if value.is_finite() { Ok(()) } else { Err(Error::new(ErrorKind::Protocol, "non-finite required finance field")
        .with_context("command", "0x0010").with_context("field", format!("{field:?}")).with_context("bits", format!("{:08x}", value.to_bits()))) }
}
fn wrong(name: &str) -> Error { Error::new(ErrorKind::Protocol, format!("invalid {name} response")) }
