//! Board compositions on one existing owner and operation. Candidate order,
//! missing observations, native units and every auxiliary source are retained.
use std::cmp::Ordering;
use std::collections::HashSet;
use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;
use crate::{Error, ErrorKind, PageOptions, Result, TimeoutPhase, TradingDate};
use crate::operation::{ChildPurpose, OperationContext};
use crate::types::{Diagnostic, EnrichmentStatus, ResultMeta};
use super::{Bar, BoardInfo, BoardRankKey, BoardSymbol, FieldId, FieldSelection, MacAdjust,
    MacCode, MacInstrumentId, MacPeriod, NativeNumber, PresetField, Quote, WireFloat};
use super::collect::{self, Collection, ListPlan};
use super::models::{FieldContext, FieldMeaning, SlotValue};
use super::pagination::{EndPolicy, MergeOrder, PageRows, Pagination, Target};
use super::query::{BarsQuery, BoardsQuery, BoardSummaryQuery, BoardRankingQuery, BoardChangeRankingQuery};
use super::session::MacOwner;

/// A dynamic source-native field. Float bits and unknown bytes are never
/// rounded, coerced to zero or assigned an inferred currency/volume unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NativeValue { I64(i64), U64(u64), F32(WireFloat), Text(String), Bytes(Vec<u8>) }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedNativeField { name: String, value: NativeValue, meaning: FieldMeaning }
impl NamedNativeField {
    pub fn name(&self) -> &str { &self.name }
    pub fn value(&self) -> &NativeValue { &self.value }
    pub fn meaning(&self) -> &FieldMeaning { &self.meaning }
}

/// Aggregates all member occurrences, including duplicates. Missing fields
/// carry diagnostics; a subtotal is not evidence of a complete field sum.
#[derive(Clone, Debug)]
pub struct BoardSummary {
    vol: NativeNumber<u64>, amount: NativeNumber<f64>, main_net_amount: NativeNumber<f64>,
    main_net_3d: NativeNumber<f64>, main_net_5d: NativeNumber<f64>,
    up_count: u32, down_count: u32, member_count: u32,
    members: Vec<Quote>, extra: Vec<NamedNativeField>, missing: [usize;6], meta: Arc<ResultMeta>,
}
impl BoardSummary {
    pub fn vol(&self) -> &NativeNumber<u64> { &self.vol }
    pub fn amount(&self) -> &NativeNumber<f64> { &self.amount }
    pub fn main_net_amount(&self) -> &NativeNumber<f64> { &self.main_net_amount }
    pub fn main_net_3d(&self) -> &NativeNumber<f64> { &self.main_net_3d }
    pub fn main_net_5d(&self) -> &NativeNumber<f64> { &self.main_net_5d }
    pub const fn up_count(&self) -> u32 { self.up_count }
    pub const fn down_count(&self) -> u32 { self.down_count }
    pub const fn member_count(&self) -> u32 { self.member_count }
    pub fn members(&self) -> &[Quote] { &self.members }
    pub fn extra(&self) -> &[NamedNativeField] { &self.extra }
    /// Missing/invalid member occurrences for one aggregate input bit. Bits0/4
    /// report pairs unusable for up/down; unrelated fields return None.
    pub fn missing_member_count(&self, field: FieldId) -> Option<usize> {
        match field.get() { 7 => Some(self.missing[0]),56 => Some(self.missing[1]),111 => Some(self.missing[2]),
            112 => Some(self.missing[3]),5 => Some(self.missing[4]),0|4 => Some(self.missing[5]),_ => None }
    }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
#[derive(Clone, Debug)]
pub struct BoardRank {
    source: BoardInfo, change_pct: Option<f64>, summary: BoardSummary, meta: Arc<ResultMeta>,
}
impl BoardRank {
    pub fn code(&self) -> &str { self.source.board().id().code().as_str() }
    pub fn name(&self) -> &str { self.source.board().name().as_str() }
    pub const fn change_pct(&self) -> Option<f64> { self.change_pct }
    pub fn summary(&self) -> &BoardSummary { &self.summary }
    /// Includes both board and independently identified leader source fields.
    pub fn source(&self) -> &BoardInfo { &self.source }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
#[derive(Clone, Debug)]
pub struct BoardChangeRank {
    source: BoardInfo, close_start: WireFloat, close_end: WireFloat, change_pct: f64,
    start_date: TradingDate, end_date: TradingDate, meta: Arc<ResultMeta>,
}
impl BoardChangeRank {
    pub fn code(&self) -> &str { self.source.board().id().code().as_str() }
    pub fn name(&self) -> &str { self.source.board().name().as_str() }
    pub const fn close_start(&self) -> WireFloat { self.close_start }
    pub const fn close_end(&self) -> WireFloat { self.close_end }
    pub const fn change_pct(&self) -> f64 { self.change_pct }
    pub const fn start_date(&self) -> TradingDate { self.start_date }
    pub const fn end_date(&self) -> TradingDate { self.end_date }
    pub fn source(&self) -> &BoardInfo { &self.source }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

pub(crate) fn summary(owner: &MacOwner, context: &OperationContext, query: &BoardSummaryQuery) -> Result<BoardSummary> {
    let mut fields = FieldSelection::preset(PresetField::Basic);
    for (bit,_,_) in super::python_legacy::BOARD_MONEY_AGGREGATES { fields.insert(FieldId::new(bit)?); }
    let members = collect::quote_list(owner,context,ListPlan {
        code: query.board().wire_code(),start: 0,target: Target::All,fields: &fields,
        sort: query.sort(),order: query.order(),exclude: super::FilterMask::empty(),
        pages: query.pages().clone(),field_context: FieldContext::Instrument,
    })?;
    let result = summarize(context,members)?;
    owner.session.record_meta(result.meta().clone())?;
    context.check_boundary(TimeoutPhase::Aggregate)?; Ok(result)
}

fn summarize(context: &OperationContext, members: Collection<Quote>) -> Result<BoardSummary> {
    let mut vol = 0_u64; let mut sums = [0.0_f64;4]; let mut missing = [0_usize;6];
    let mut nonfinite = [0_usize;4]; let mut up = 0_u32; let mut down = 0_u32;
    let mut extra = Vec::new(); let bits = super::python_legacy::BOARD_MONEY_AGGREGATES.map(|(bit,_,_)| bit);
    let member_count = u32::try_from(members.rows.len()).map_err(|_| limit("board member count exceeds u32"))?;
    for (index,quote) in members.rows.iter().enumerate() {
        if index % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        match quote.field(FieldId::new(5)?).map(|field| field.value) {
            Some(SlotValue::U32(value)) => vol = vol.checked_add(u64::from(value)).ok_or_else(|| limit("board volume sum exceeds u64"))?,
            _ => missing[4] += 1,
        }
        for (slot,bit) in bits.into_iter().enumerate() {
            match float(quote,bit)? {
                Some(value) => { sums[slot] += value; if !value.is_finite() { nonfinite[slot] += 1; } },
                None => missing[slot] += 1,
            }
        }
        match (float(quote,4)?,float(quote,0)?) {
            (Some(close),Some(pre)) if close.is_finite() && pre.is_finite() => {
                if close > pre { up = up.checked_add(1).ok_or_else(|| limit("board up count exceeds u32"))?; }
                if close < pre { down = down.checked_add(1).ok_or_else(|| limit("board down count exceeds u32"))?; }
            },
            _ => missing[5] += 1,
        }
        // Keep additional known fields and every unknown legal slot in a
        // source-occurrence-qualified view as well as the complete member.
        for field in quote.fields() {
            if matches!(field.bit.get(),0|4|5|7|56|111|112) { continue; }
            let name = match field.name {
                Some(name) => format!("members[{index}].{}",name.to_ascii_lowercase()),
                None => format!("members[{index}].bit_{}",field.bit.get()),
            };
            let bytes = std::mem::size_of::<NamedNativeField>().checked_add(name.len())
                .and_then(|value| value.checked_add(4)).ok_or_else(|| limit("board extra field accounting overflows"))?;
            context.record_work(0,bytes)?;
            extra.try_reserve(1).map_err(crate::services::allocation_error)?;
            let value = match field.value {
                SlotValue::F32(value) => NativeValue::F32(value),SlotValue::U32(value) => NativeValue::U64(u64::from(value)),
                SlotValue::I32(value) => NativeValue::I64(i64::from(value)),SlotValue::Unknown(value) => NativeValue::Bytes(value.to_vec()),
            };
            extra.push(NamedNativeField { name,value,meaning: field.meaning.clone() });
        }
    }
    let mut meta = members.meta;
    for (index,name) in ["amount","main_net_amount","main_net_3d","main_net_5d","vol","up/down"].into_iter().enumerate() {
        if missing[index] != 0 {
            meta = meta.with_diagnostic(unavailable("MissingAggregateField",format!(
                "field={name}; missing_or_invalid_members={}; members={member_count}; numeric subtotal is not a complete observation",missing[index])));
        }
    }
    for (slot,bit) in bits.into_iter().enumerate() {
        if nonfinite[slot] != 0 || !sums[slot].is_finite() {
            meta = meta.with_diagnostic(unavailable("NonfiniteAggregateField",format!(
                "bit={bit}; nonfinite_inputs={}; source values and arithmetic result are retained",nonfinite[slot])));
        }
    }
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(BoardSummary {
        vol: NativeNumber::with_meaning(vol,super::bitmap::meaning(FieldId::new(5)?)),
        amount: NativeNumber::with_meaning(sums[0],super::bitmap::meaning(FieldId::new(7)?)),
        main_net_amount: NativeNumber::with_meaning(sums[1],super::bitmap::meaning(FieldId::new(56)?)),
        main_net_3d: NativeNumber::with_meaning(sums[2],super::bitmap::meaning(FieldId::new(111)?)),
        main_net_5d: NativeNumber::with_meaning(sums[3],super::bitmap::meaning(FieldId::new(112)?)),
        up_count: up,down_count: down,member_count,members: members.rows,extra,missing,meta: Arc::new(meta),
    })
}
fn float(quote: &Quote, bit: u8) -> Result<Option<f64>> {
    Ok(match quote.field(FieldId::new(bit)?).map(|field| field.value) { Some(SlotValue::F32(value)) => Some(value.as_f64()),_ => None })
}

pub(crate) fn ranking(owner: &MacOwner, context: &OperationContext, query: &BoardRankingQuery) -> Result<Vec<BoardRank>> {
    let boards_query = BoardsQuery::new().with_board_type(query.board_type()).with_pages(query.pages().clone())?;
    let boards = collect::boards(owner,context,&boards_query,Target::All)?;
    let mut observations = vec![boards.meta]; let mut changes = storage(context,boards.rows.len())?;
    for board in &boards.rows {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let price = board.board().price().value().as_f64(); let pre = board.board().pre_close().value().as_f64();
        let change = if pre != 0.0 && pre.is_finite() && price.is_finite() {
            let value = percentage(price,pre); value.is_finite().then_some(value)
        } else { None };
        if change.is_none() { observations[0] = std::mem::take(&mut observations[0]).with_diagnostic(unavailable(
            "InvalidBoardReference",format!("code={}; zero or nonfinite reference/price; change_pct is absent",board.board().id().code().as_str()))); }
        changes.push(change);
    }
    let mut order = if query.sort() == BoardRankKey::ChangePct {
        stable_indices(context,boards.rows.len(),|left,right| optional_cmp(changes[left],changes[right],query.ascending()))?
    } else { indices(context,boards.rows.len())? };
    if query.sort() == BoardRankKey::ChangePct { truncate(&mut order,query.top_n())?; }
    let mut rows = storage(context,order.len())?;
    for &index in &order {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let board = &boards.rows[index];
        if board.board().id().code().replacement_used() { return Err(protocol("board member identity contains invalid GBK")); }
        let symbol = BoardSymbol::new(board.board().id().code().as_str())
            .map_err(|error| protocol("source board code cannot address its member list").with_source(error))?;
        let member_query = BoardSummaryQuery::new(symbol)
            .with_pages(query.member_pages().clone())?;
        let summary = summary(owner,context,&member_query)?;
        observations.try_reserve(1).map_err(crate::services::allocation_error)?; observations.push(summary.meta().clone());
        rows.push(BoardRank { source: board.clone(),change_pct: changes[index],summary,meta: Arc::new(ResultMeta::default()) });
    }
    if query.sort() != BoardRankKey::ChangePct {
        let sorted = stable_indices(context,rows.len(),|left,right| {
            let left = &rows[left].summary; let right = &rows[right].summary;
            if query.sort() == BoardRankKey::Volume {
                optional_ord(volume_key(left),volume_key(right),query.ascending())
            } else { optional_cmp(money_key(left,query.sort()),money_key(right,query.sort()),query.ascending()) }
        })?;
        apply_order(context,&mut rows,&sorted)?;
        // Non-change metrics started in exact candidate/source order.
        order = sorted; truncate(&mut order,query.top_n())?; truncate(&mut rows,query.top_n())?;
    }
    let meta = aggregate(owner,context,&observations,&order)?;
    for row in &mut rows { context.check_boundary(TimeoutPhase::Aggregate)?; row.meta = Arc::clone(&meta); }
    context.check_boundary(TimeoutPhase::Aggregate)?; Ok(rows)
}

fn volume_key(summary: &BoardSummary) -> Option<u64> {
    (summary.missing[4] == 0).then_some(*summary.vol.value())
}
fn money_key(summary: &BoardSummary, key: BoardRankKey) -> Option<f64> {
    let (value,index) = match key {
        BoardRankKey::Amount => (*summary.amount.value(),0),
        BoardRankKey::MainNetAmount => (*summary.main_net_amount.value(),1),
        BoardRankKey::MainNet3d => (*summary.main_net_3d.value(),2),
        BoardRankKey::MainNet5d => (*summary.main_net_5d.value(),3),
        _ => return None,
    };
    (value.is_finite() && summary.missing[index] == 0).then_some(value)
}
fn optional_ord<T: Ord>(left: Option<T>, right: Option<T>, ascending: bool) -> Ordering {
    match (left,right) { (Some(left),Some(right)) => if ascending { left.cmp(&right) } else { right.cmp(&left) },
        (None,None) => Ordering::Equal,(None,_) => Ordering::Greater,(_,None) => Ordering::Less }
}
fn optional_cmp(left: Option<f64>, right: Option<f64>, ascending: bool) -> Ordering {
    match (left,right) { (Some(left),Some(right)) => {
        // Inputs are finite; equality includes the two source zero signs.
        let order = left.partial_cmp(&right).unwrap_or(Ordering::Equal); if ascending { order } else { order.reverse() }
    },(None,None) => Ordering::Equal,(None,_) => Ordering::Greater,(_,None) => Ordering::Less }
}

pub(crate) fn change_ranking(owner: &MacOwner, context: &OperationContext, query: &BoardChangeRankingQuery) -> Result<Vec<BoardChangeRank>> {
    let boards_query = BoardsQuery::new().with_board_type(query.board_type()).with_pages(query.pages().clone())?;
    let boards = collect::boards(owner,context,&boards_query,Target::All)?;
    let mut observations = vec![boards.meta]; let mut rows = storage(context,boards.rows.len())?;
    let mut source_indices = storage(context,boards.rows.len())?;
    for (index,board) in boards.rows.iter().enumerate() {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let code = board.board().id().code();
        if code.replacement_used() { return Err(protocol("board history identity contains invalid GBK")); }
        let id = MacInstrumentId::new(board.board().id().market(),MacCode::new(code.as_str())
            .map_err(|error| protocol("source board code cannot address its history").with_source(error))?);
        let history = history(owner,context,id,query)?;
        observations.try_reserve(1).map_err(crate::services::allocation_error)?; observations.push(history.meta);
        let Some((start,end)) = history.window else {
            let last = observations.last_mut().ok_or_else(|| internal("board history observation disappeared"))?;
            *last = std::mem::take(last).with_diagnostic(unavailable("InsufficientBoardHistory",format!(
                "code={}; requested_intervals={}; target={:?}; source history contains no full requested window",code.as_str(),query.days(),query.target_date())));
            continue;
        };
        let first = &history.rows[start]; let last = &history.rows[end];
        let close_start = first.close().value().wire(); let close_end = last.close().value().wire();
        let pre = close_start.as_f64(); let current = close_end.as_f64();
        let change = percentage(current,pre);
        if pre <= 0.0 || current <= 0.0 || !pre.is_finite() || !current.is_finite() || !change.is_finite() {
            let last = observations.last_mut().ok_or_else(|| internal("board history observation disappeared"))?;
            *last = std::mem::take(last).with_diagnostic(unavailable("InvalidBoardHistoryPrice",format!(
                "code={}; source window endpoint is nonpositive/nonfinite; no fabricated return is ranked",code.as_str())));
            continue;
        }
        rows.try_reserve(1).map_err(crate::services::allocation_error)?;
        source_indices.try_reserve(1).map_err(crate::services::allocation_error)?; source_indices.push(index);
        rows.push(BoardChangeRank { source: board.clone(),close_start,close_end,change_pct: change,
            start_date: first.source_time().date().ok_or_else(|| protocol("board history lacks source date"))?,
            end_date: last.source_time().date().ok_or_else(|| protocol("board history lacks source date"))?,
            meta: Arc::new(ResultMeta::default()) });
    }
    let order = stable_indices(context,rows.len(),|left,right| optional_cmp(Some(rows[left].change_pct),Some(rows[right].change_pct),query.ascending()))?;
    apply_order(context,&mut rows,&order)?;
    let mut order = order; truncate(&mut order,query.top_n())?; truncate(&mut rows,query.top_n())?;
    let mut selection = storage(context,order.len())?;
    for &index in &order { context.check_boundary(TimeoutPhase::Aggregate)?; selection.push(source_indices[index]); }
    let meta = aggregate(owner,context,&observations,&selection)?;
    for row in &mut rows {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        row.meta = Arc::clone(&meta);
    }
    context.check_boundary(TimeoutPhase::Aggregate)?; Ok(rows)
}

struct History { rows: Vec<Bar>, meta: ResultMeta, window: Option<(usize,usize)> }
/// Fetch towards the past until the exact requested trading intervals before
/// target are available, or this command proves EOF. A full safety cap errors.
fn history(owner: &MacOwner, context: &OperationContext, id: MacInstrumentId, query: &BoardChangeRankingQuery) -> Result<History> {
    let requested = query.days().get().checked_add(10).ok_or_else(|| super::query::invalid("board history buffer overflows"))?;
    let pages = query.history_pages().clone();
    let per_page = pages.page_size().min(usize::try_from(requested).map_err(|_| limit("board history count exceeds platform"))?);
    let pages = PageOptions::new(per_page,pages.max_pages(),pages.max_records())?;
    let mut progress = Pagination::new(0,Target::All,pages,NonZeroU16::new(700).expect("positive"),u32::MAX,EndPolicy::EmptyOrShort)?;
    let mut records = PageRows::new(MergeOrder::PrependPages); let mut metas = Vec::new(); let mut order = PageRows::new(MergeOrder::PrependPages);
    let mut eligible_dates = HashSet::new();
    while let Some(request) = progress.next_request()? {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let bars_query = BarsQuery::new(id.clone()).with_period(MacPeriod::Daily)?.with_adjust(MacAdjust::None)
            .with_count(u32::from(request.count()))?.with_start(request.offset())?;
        let reply = collect::bars_page(owner,context,&bars_query,request.offset(),request.count(),ChildPurpose::Page)?;
        let count = reply.wire_count.ok_or_else(|| internal("board history lacks wire count"))?;
        let previous = progress.rows(); progress.accept(usize::from(count))?;
        let meta = Arc::new(reply.meta); let mut rows = reply.value;
        for row in &mut rows {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            row.set_meta(Arc::clone(&meta));
            let date = row.source_time().date().ok_or_else(|| protocol("daily board history lacks source date"))?;
            if query.target_date().is_none_or(|target| date <= target) && !eligible_dates.contains(&date) {
                context.record_work(0,std::mem::size_of::<TradingDate>()+2*std::mem::size_of::<usize>())?;
                eligible_dates.try_reserve(1).map_err(crate::services::allocation_error)?; eligible_dates.insert(date);
            }
        }
        records.push(rows); order.push((previous..progress.rows()).collect()); metas.push(meta.as_ref().clone());
        // Whole pages stay intact. Readiness counts only observed source dates
        // at/before target; final stable ordering is applied once below.
        if eligible_dates.len() > usize::try_from(query.days().get()).map_err(|_| limit("board history interval count exceeds platform"))? { break; }
        if progress.completion().is_some() { break; }
    }
    let mut rows = records.into_rows(); let original_order = order.into_rows();
    let sorted = stable_indices(context,rows.len(),|left,right| rows[left].source_time().date().cmp(&rows[right].source_time().date()))?;
    let mut raw_order = storage(context,sorted.len())?;
    for &index in &sorted { context.check_boundary(TimeoutPhase::Aggregate)?; raw_order.push(original_order[index]); }
    apply_order(context,&mut rows,&sorted)?;
    let mut meta = crate::services::aggregate_meta(context,&metas,None)?.with_raw_row_order(&raw_order)?;
    let mut points: Vec<(TradingDate,usize)> = storage(context,rows.len())?; let mut duplicate_dates = 0_usize;
    for (index,row) in rows.iter().enumerate() {
        if index % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        let date = row.source_time().date().ok_or_else(|| protocol("board history lacks source date"))?;
        if query.target_date().is_some_and(|target| date > target) { continue; }
        if points.last().is_some_and(|&(previous,_)| previous == date) {
            duplicate_dates += 1;
            points.last_mut().ok_or_else(|| internal("board history date disappeared"))?.1 = index;
        } else { points.push((date,index)); }
    }
    if duplicate_dates > 0 { meta = meta.with_diagnostic(Diagnostic::new("mac-boards","DuplicateBoardHistoryDate",EnrichmentStatus::Succeeded,None,
        Some(format!("duplicate_occurrences={duplicate_dates}; all source rows retained; last stable observation supplies each daily endpoint")))); }
    let days = usize::try_from(query.days().get()).map_err(|_| limit("board history interval count exceeds platform"))?;
    let window = points.len().checked_sub(1).and_then(|end| end.checked_sub(days).map(|start| (points[start].1,points[end].1)));
    context.check_boundary(TimeoutPhase::Aggregate)?;
    Ok(History { rows,meta,window })
}

fn aggregate(owner: &MacOwner, context: &OperationContext, observations: &[ResultMeta], selection: &[usize]) -> Result<Arc<ResultMeta>> {
    context.check_boundary(TimeoutPhase::Aggregate)?;
    let meta = crate::services::aggregate_meta(context,observations,None)?.with_raw_row_selection(selection)?;
    owner.session.record_meta(meta.clone())?;
    context.check_boundary(TimeoutPhase::Aggregate)?; Ok(Arc::new(meta))
}
fn percentage(price: f64, reference: f64) -> f64 {
    let result = crate::helpers::calculation::percentage_change_with::<_,std::convert::Infallible>(price,&reference,
        |price,base| Ok(price-base),|change,base| Ok(change/base),|change| Ok(change*100.0));
    match result { Ok(value) => value,Err(never) => match never {} }
}
fn truncate<T>(rows: &mut Vec<T>, top: Option<NonZeroU32>) -> Result<()> {
    if let Some(top) = top { rows.truncate(usize::try_from(top.get()).map_err(|_| limit("board top count exceeds platform"))?); }
    Ok(())
}
fn indices(context: &OperationContext, length: usize) -> Result<Vec<usize>> {
    let mut values = storage(context,length)?;
    for index in 0..length { if index % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; } values.push(index); }
    Ok(values)
}
fn storage<T>(context: &OperationContext, length: usize) -> Result<Vec<T>> {
    let bytes = length.checked_mul(std::mem::size_of::<T>()).ok_or_else(|| limit("board composition storage overflows"))?;
    context.record_work(0,bytes)?;
    let mut values = Vec::new(); values.try_reserve_exact(length).map_err(crate::services::allocation_error)?; Ok(values)
}
/// Apply a full occurrence permutation with one bounded inverse map, avoiding
/// a second allocation containing all member vectors or parsed bar records.
fn apply_order<T>(context: &OperationContext, rows: &mut [T], order: &[usize]) -> Result<()> {
    if rows.len() != order.len() { return Err(internal("board sort permutation length differs")); }
    let mut inverse = indices(context,order.len())?;
    for (position,&source) in order.iter().enumerate() {
        if position % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
        *inverse.get_mut(source).ok_or_else(|| internal("board sort source index exceeds rows"))? = position;
    }
    for position in 0..rows.len() {
        context.check_boundary(TimeoutPhase::Aggregate)?;
        while inverse[position] != position {
            context.check_boundary(TimeoutPhase::Aggregate)?;
            let next = inverse[position]; rows.swap(position,next); inverse.swap(position,next);
        }
    }
    Ok(())
}
/// Fallible stable merge sort polls the parent during comparisons and copying.
/// Equal keys take the earlier input occurrence; None always sorts last.
fn stable_indices(context: &OperationContext, length: usize, mut compare: impl FnMut(usize,usize) -> Ordering) -> Result<Vec<usize>> {
    let mut values = indices(context,length)?; let mut scratch = indices(context,length)?;
    let mut width = 1;
    while width < length {
        let step = width.checked_mul(2).unwrap_or(length); let mut base = 0;
        while base < length {
            let middle = base.saturating_add(width).min(length); let end = base.saturating_add(step).min(length);
            let mut left = base; let mut right = middle;
            for target in base..end {
                if target % 256 == 0 { context.check_boundary(TimeoutPhase::Aggregate)?; }
                if left < middle && (right == end || compare(values[left],values[right]) != Ordering::Greater) {
                    scratch[target] = values[left]; left += 1;
                } else { scratch[target] = values[right]; right += 1; }
            }
            base = end;
        }
        std::mem::swap(&mut values,&mut scratch); width = step;
    }
    context.check_boundary(TimeoutPhase::Aggregate)?; Ok(values)
}
fn unavailable(stage: &'static str, message: String) -> Diagnostic {
    Diagnostic::new("mac-boards",stage,EnrichmentStatus::Unavailable,Some(ErrorKind::MissingData),Some(message))
}
fn limit(message: &'static str) -> Error { Error::new(ErrorKind::LimitExceeded,message).with_context("domain","mac-boards") }
fn protocol(message: &'static str) -> Error { Error::new(ErrorKind::Protocol,message).with_context("domain","mac-boards") }
fn internal(message: &'static str) -> Error { Error::new(ErrorKind::Internal,message).with_context("domain","mac-boards") }
