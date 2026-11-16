use std::time::Instant;

use crate::operation::{OperationContext, OperationOptions, WorkLimits};
use crate::services::{aggregate_meta, checked_ids, AuctionService, SessionService, TradeService};
use crate::types::{Amount, AmountUnit, AuctionData, AuctionDataQuery, AuctionQuery, BatchItemState,
    Diagnostic, EnrichmentStatus, HandshakeInfo, MarketDateChoice, MissingPolicy, NumericValue,
    Price, QuantityUnit, Ratio, RatioUnit, ResultMeta, SecurityId, TradeEventKind, TradeQuery,
    TradeTick, TradingDate, ValueProvenance};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use super::service::HelperService;

impl HelperService {
    pub fn auction_data(&self, id: &SecurityId, query: &AuctionDataQuery,
        options: &OperationOptions) -> Result<AuctionData> {
        let entry = Instant::now();
        let mut context = self.executor.begin_context_at(entry, "helpers_auction_data", &self.config, options)?;
        checked_ids(std::slice::from_ref(id))?;
        let historical = matches!(query.date(), MarketDateChoice::Date(_));
        let need_reference = query.include_quote() && query.pre_close().is_none();
        let need_trades = query.include_snapshot() || (historical && need_reference);
        let trade_query = TradeQuery::new().with_date(query.date())?;
        let records = usize::from(query.include_series()) * eltdx_protocol::commands::auctions::DEFAULT_AUCTION_LIMIT as usize
            + usize::from(need_trades) * trade_query.pages().max_records() + usize::from(need_reference && !historical);
        context.set_work_limits(WorkLimits { pages: None, records: Some(records), bytes: None })?;
        let mut metas = Vec::new(); let mut diagnostics = Vec::new();
        let date = match query.date() {
            MarketDateChoice::Date(date) => date,
            MarketDateChoice::Current => {
                let (date, handshake) = self.source_date_in_context(&context)?;
                metas.push(handshake.meta().clone()); date
            }
        };
        // Quote precision must be frozen before auction/trade decoding starts.
        // Historical reference never requests a current quote or directory.
        if need_reference && !historical { self.quotes().prepare_in_context(&context, std::slice::from_ref(id))?; }
        let series = if query.include_series() {
            let service = AuctionService::new(self.executor.clone(), self.pools.clone(), self.config.clone());
            let series = service.series_in_context(&context, id, &AuctionQuery::new().with_date(query.date())?, date)?;
            metas.push(series.meta().clone());
            diagnostics.push(state("auction.series", if series.points().is_empty() {
                EnrichmentStatus::Unavailable } else { EnrichmentStatus::Succeeded }, None));
            Some(series)
        } else { diagnostics.push(state("auction.series", EnrichmentStatus::NotRequested, None)); None };
        let mut opening = None;
        let mut pre_close = query.pre_close();
        if need_trades {
            let service = TradeService::new(self.executor.clone(), self.engine.clone(), self.config.clone(), self.namespace.clone());
            let page = if historical {
                let page = service.collect_at_date_in_context(&context, id, &trade_query, date)?;
                if query.include_snapshot() {
                    opening = page.ticks().items().iter().find(|tick| tick.kind() == TradeEventKind::OpeningMatch).cloned();
                }
                if need_reference { pre_close = page.price_base().filter(|price| price.as_f64() > 0.0); }
                page
            } else {
                let (page, tick) = service.opening_page_in_context(&context, id, &trade_query, date)?;
                opening = tick; page
            };
            metas.push(page.meta().clone());
        }
        diagnostics.push(state("auction.opening_match", if !query.include_snapshot() { EnrichmentStatus::NotRequested }
            else if opening.is_some() { EnrichmentStatus::Succeeded } else { EnrichmentStatus::Unavailable }, None));
        if need_reference && !historical {
            let batch = self.quotes().snapshots_in_context(&context, std::slice::from_ref(id), MissingPolicy::Error, 80)?;
            metas.push(batch.meta().clone());
            let item = batch.into_items().into_iter().next().ok_or_else(|| Error::new(ErrorKind::MissingData,
                "auction reference is missing its required current quote"))?;
            if let BatchItemState::Present(snapshot) = item.into_parts().2 {
                pre_close = Some(snapshot.pre_close());
            } else { return Err(Error::new(ErrorKind::MissingData, "auction current reference quote is missing")); }
        }
        diagnostics.push(state("auction.reference", if pre_close.is_some() { EnrichmentStatus::Succeeded }
            else if !query.include_quote() { EnrichmentStatus::NotRequested } else { EnrichmentStatus::Unavailable },
            query.pre_close().map(|_| "explicit caller reference takes precedence")));
        let version = context.metadata()?.map(|snapshot| snapshot.version());
        let mut meta = aggregate_meta(&context, &metas, version)?;
        for diagnostic in diagnostics { meta = meta.with_diagnostic(diagnostic); }
        context.check_boundary(TimeoutPhase::Aggregate)?;
        let (open, open_volume, open_amount, open_change) = opening_fields(opening.as_ref(), pre_close, &mut meta)?;
        let result = AuctionData { id: id.clone(), date, series, opening_match: opening,
            pre_close, open, open_volume, open_amount, open_change, meta };
        context.finish()?;
        Ok(result)
    }

    /// Server dates provide the current source day. A local clock cannot fill
    /// a missing handshake date or prove that a response belongs to today.
    pub(super) fn source_date_in_context(&self, context: &OperationContext) -> Result<(TradingDate, HandshakeInfo)> {
        let info = SessionService::new(self.executor.clone(), self.engine.clone(), self.config.clone()).handshake_in_context(context)?;
        let date = info.server_date1().into_iter().chain(info.server_date2()).max()
            .or_else(|| info.server_datetime().and_then(|time| time.date()))
            .ok_or_else(|| Error::new(ErrorKind::MetadataUnavailable, "handshake provides no valid source trading date"))?;
        Ok((date, info))
    }
}

type OpeningFields = (Option<Price>, Option<crate::types::Quantity>, Option<Amount>, Option<Ratio>);
fn opening_fields(tick: Option<&TradeTick>, reference: Option<Price>, meta: &mut ResultMeta) -> Result<OpeningFields> {
    let Some(tick) = tick else { return Ok((None, None, None, None)); };
    if tick.kind() != TradeEventKind::OpeningMatch {
        return Err(Error::new(ErrorKind::Internal, "auction composition received a non-opening trade"));
    }
    let price = tick.price(); let volume = tick.volume();
    if !price.as_f64().is_finite() || price.as_f64() < 0.0 || !volume.value().is_finite() || volume.value().as_f64() < 0.0 {
        return Err(Error::new(ErrorKind::Protocol, "opening trade has an invalid price or volume"));
    }
    let amount = price.as_f64() * volume.value().as_f64() * 100.0;
    let amount = if volume.unit() == QuantityUnit::Lots && amount.is_finite() {
        Some(Amount::with_provenance(NumericValue::F64(amount), AmountUnit::Cny, ValueProvenance::WireRule))
    } else {
        *meta = meta.clone().with_diagnostic(state("auction.open_amount", EnrichmentStatus::Unavailable,
            Some("opening amount requires known equity hand units and finite CNY arithmetic")));
        None
    };
    let change = reference.and_then(|reference| {
        let value = (price.as_f64() - reference.as_f64()) / reference.as_f64() * 100.0;
        if reference.as_f64() > 0.0 && value.is_finite() {
            Some(Ratio::with_provenance(NumericValue::F64(value), RatioUnit::PercentagePoints, ValueProvenance::WireRule))
        } else {
            *meta = meta.clone().with_diagnostic(state("auction.open_change", EnrichmentStatus::Unavailable,
                Some("opening change requires a positive finite reference price"))); None
        }
    });
    Ok((Some(price), Some(volume), amount, change))
}
fn state(stage: &str, status: EnrichmentStatus, message: Option<&str>) -> Diagnostic {
    Diagnostic::new("hq.auction_data", stage, status, None, message.map(str::to_owned))
}
