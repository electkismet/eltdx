use crate::commands::{
    auctions::parse_auction_series_payload_with_context,
    corporate::{
        parse_capital_changes_payload_with_context, parse_finance_batch_payload_with_context,
    },
    delisted_bars::parse_delisted_klines_payload_with_context,
    history_fund_flow::parse_history_fund_flow_payload_with_context,
    klines::parse_klines_payload_with_context,
    limits::parse_special_limits_payload_with_context,
    minutes::{
        parse_historical_intraday_payload_with_context, parse_intraday_aux_payload_with_context,
        parse_recent_intraday_payload_with_context, parse_sparkline_payload_with_context,
        parse_today_intraday_payload_with_context,
    },
    money_flow::parse_money_flow_payload_with_context,
    quotes::{
        parse_category_quotes_payload_with_context, parse_legacy_quotes_payload_with_context,
        parse_refresh_stream_payload_with_context, parse_snapshots_payload_with_context,
    },
    resources::parse_file_content_payload_with_context,
    security::parse_security_list_payload_with_context,
    session::{parse_handshake_payload_with_context, parse_heartbeat_payload_with_context},
    trades::{parse_historical_ticks_payload_with_context, parse_today_ticks_payload_with_context},
};
use crate::commands::{
    auctions::{parse_auction_series_payload, AuctionSeries},
    corporate::{
        parse_capital_changes_payload, parse_company_info_category_payload,
        parse_company_info_content_payload, parse_finance_batch_payload, CapitalChangeBatch,
        CompanyInfoCategory, FinanceBatch,
    },
    delisted_bars::{parse_delisted_klines_payload, DelistedKlineSeries},
    history_fund_flow::{
        parse_history_fund_flow_payload, HistoryFundFlowSeries, HISTORY_FUND_FLOW_CATEGORY,
    },
    klines::{parse_klines_payload, KlineSeries},
    limits::{parse_special_limits_payload, SpecialLimitPage},
    minutes::{
        parse_historical_intraday_payload, parse_intraday_aux_payload,
        parse_recent_intraday_payload, parse_sparkline_payload, parse_today_intraday_payload,
        HistoricalIntradayRequest, MinuteAuxSeries, MinuteSeries, RecentIntradayRequest,
        SparklineSeries, TodayIntradayRequest,
    },
    money_flow::{parse_money_flow_payload, MoneyFlowBatch},
    quotes::{
        parse_category_quotes_payload, parse_legacy_quotes_payload, parse_refresh_stream_payload,
        parse_snapshots_payload, CategoryQuotePage, LegacyQuote, QuoteRefreshPage, QuoteSnapshot,
    },
    resources::{
        parse_block_meta_payload, parse_file_content_payload, BlockMeta, FileContentChunk,
    },
    security::{parse_security_count_payload, parse_security_list_payload, SecurityCode},
    session::{parse_handshake_payload, parse_heartbeat_payload, HandshakeInfo, HeartbeatAck},
    trades::{
        parse_historical_ticks_payload, parse_today_ticks_payload, HistoricalTicksRequest,
        TodayTicksRequest, TradePage,
    },
};
use crate::context::ParseOptions;
use crate::error::ProtocolError;
use crate::request::CommandRequest;
use bytes::Bytes;

#[derive(Clone, Debug, PartialEq)]
pub enum CommandResponse {
    Heartbeat(HeartbeatAck),
    Handshake(HandshakeInfo),
    CapitalChanges(CapitalChangeBatch),
    FinanceBatch(FinanceBatch),
    CompanyInfoCategory(Vec<CompanyInfoCategory>),
    CompanyInfoContent(String),
    BlockMeta(BlockMeta),
    SecurityList(Vec<SecurityCode>),
    SecurityCount(u16),
    SpecialLimits(SpecialLimitPage),
    IntradayAux(MinuteAuxSeries),
    DelistedKlines(DelistedKlineSeries),
    Klines(KlineSeries),
    HistoryFundFlow(HistoryFundFlowSeries),
    TodayIntraday(MinuteSeries<TodayIntradayRequest>),
    LegacyQuotes(Vec<LegacyQuote>),
    RefreshStream(QuoteRefreshPage),
    CategoryQuotes(CategoryQuotePage),
    Snapshots(Vec<QuoteSnapshot>),
    AuctionSeries(AuctionSeries),
    FileContent(FileContentChunk),
    HistoricalIntraday(MinuteSeries<HistoricalIntradayRequest>),
    TodayTicks(TradePage<TodayTicksRequest>),
    HistoricalTicks(TradePage<HistoricalTicksRequest>),
    Sparkline(SparklineSeries),
    RecentIntraday(MinuteSeries<RecentIntradayRequest>),
    MoneyFlow(MoneyFlowBatch),
}

impl CommandResponse {
    pub fn parse(request: CommandRequest, payload: &[u8]) -> Result<Self, ProtocolError> {
        match request {
            CommandRequest::Heartbeat(_) => parse_heartbeat_payload(payload).map(Self::Heartbeat),
            CommandRequest::Handshake(_) => parse_handshake_payload(payload).map(Self::Handshake),
            CommandRequest::CapitalChanges(request) => {
                parse_capital_changes_payload(payload, request).map(Self::CapitalChanges)
            }
            CommandRequest::FinanceBatch(request) => {
                parse_finance_batch_payload(payload, request).map(Self::FinanceBatch)
            }
            CommandRequest::CompanyInfoCategory(_) => {
                parse_company_info_category_payload(payload).map(Self::CompanyInfoCategory)
            }
            CommandRequest::CompanyInfoContent(_) => {
                parse_company_info_content_payload(payload).map(Self::CompanyInfoContent)
            }
            CommandRequest::BlockMeta(_) => parse_block_meta_payload(payload).map(Self::BlockMeta),
            CommandRequest::SecurityList(request) => {
                parse_security_list_payload(payload, request.market).map(Self::SecurityList)
            }
            CommandRequest::SecurityCount(_) => {
                parse_security_count_payload(payload).map(Self::SecurityCount)
            }
            CommandRequest::SpecialLimits(request) => {
                parse_special_limits_payload(payload, request).map(Self::SpecialLimits)
            }
            CommandRequest::IntradayAux(request) => {
                parse_intraday_aux_payload(payload, request).map(Self::IntradayAux)
            }
            CommandRequest::DelistedKlines(request) => {
                parse_delisted_klines_payload(payload, request).map(Self::DelistedKlines)
            }
            CommandRequest::Klines(request) => {
                // Category 22 shares the 0x052d frame but returns the historical
                // money-flow series rather than OHLC bars.
                if request.period.raw == HISTORY_FUND_FLOW_CATEGORY {
                    parse_history_fund_flow_payload(payload, request).map(Self::HistoryFundFlow)
                } else {
                    parse_klines_payload(payload, request).map(Self::Klines)
                }
            }
            CommandRequest::TodayIntraday(request) => {
                parse_today_intraday_payload(payload, request).map(Self::TodayIntraday)
            }
            CommandRequest::LegacyQuotes(request) => {
                parse_legacy_quotes_payload(payload, &request.codes).map(Self::LegacyQuotes)
            }
            CommandRequest::RefreshStream(request) => {
                let requested_codes = request.requested_codes();
                parse_refresh_stream_payload(payload, &requested_codes).map(Self::RefreshStream)
            }
            CommandRequest::CategoryQuotes(request) => {
                parse_category_quotes_payload(payload, request).map(Self::CategoryQuotes)
            }
            CommandRequest::Snapshots(request) => {
                parse_snapshots_payload(payload, &request.codes).map(Self::Snapshots)
            }
            CommandRequest::AuctionSeries(request) => {
                parse_auction_series_payload(payload, request).map(Self::AuctionSeries)
            }
            CommandRequest::FileContent(request) => {
                parse_file_content_payload(payload, request).map(Self::FileContent)
            }
            CommandRequest::HistoricalIntraday(request) => {
                parse_historical_intraday_payload(payload, request).map(Self::HistoricalIntraday)
            }
            CommandRequest::TodayTicks(request) => {
                parse_today_ticks_payload(payload, request).map(Self::TodayTicks)
            }
            CommandRequest::HistoricalTicks(request) => {
                parse_historical_ticks_payload(payload, request).map(Self::HistoricalTicks)
            }
            CommandRequest::Sparkline(request) => {
                parse_sparkline_payload(payload, request).map(Self::Sparkline)
            }
            CommandRequest::RecentIntraday(request) => {
                parse_recent_intraday_payload(payload, request).map(Self::RecentIntraday)
            }
            CommandRequest::MoneyFlow(request) => {
                parse_money_flow_payload(payload, request).map(Self::MoneyFlow)
            }
        }
    }
    /// Decodes using only the caller's immutable precision source.
    ///
    /// Retained page payloads share `payload`. For vector response variants the
    /// caller (the SDK adapter) retains that one payload alongside record ranges.
    pub fn parse_with_context(
        request: CommandRequest,
        payload: Bytes,
        options: &ParseOptions<'_>,
    ) -> Result<Self, ProtocolError> {
        match request {
            CommandRequest::Heartbeat(_) => {
                parse_heartbeat_payload_with_context(payload.clone(), options).map(Self::Heartbeat)
            }
            CommandRequest::Handshake(_) => {
                parse_handshake_payload_with_context(payload.clone(), options).map(Self::Handshake)
            }
            CommandRequest::CapitalChanges(request) => {
                parse_capital_changes_payload_with_context(payload.clone(), request, options)
                    .map(Self::CapitalChanges)
            }
            CommandRequest::FinanceBatch(request) => {
                parse_finance_batch_payload_with_context(payload.clone(), request, options)
                    .map(Self::FinanceBatch)
            }
            CommandRequest::CompanyInfoCategory(_) => {
                parse_company_info_category_payload(&payload).map(Self::CompanyInfoCategory)
            }
            CommandRequest::CompanyInfoContent(_) => {
                parse_company_info_content_payload(&payload).map(Self::CompanyInfoContent)
            }
            CommandRequest::BlockMeta(_) => parse_block_meta_payload(&payload).map(Self::BlockMeta),
            CommandRequest::SecurityList(request) => {
                parse_security_list_payload_with_context(payload.clone(), request.market, options)
                    .map(Self::SecurityList)
            }
            CommandRequest::SecurityCount(_) => {
                parse_security_count_payload(&payload).map(Self::SecurityCount)
            }
            CommandRequest::SpecialLimits(request) => {
                parse_special_limits_payload_with_context(payload.clone(), request, options)
                    .map(Self::SpecialLimits)
            }
            CommandRequest::IntradayAux(request) => {
                parse_intraday_aux_payload_with_context(payload.clone(), request, options)
                    .map(Self::IntradayAux)
            }
            CommandRequest::DelistedKlines(request) => {
                parse_delisted_klines_payload_with_context(payload.clone(), request, options)
                    .map(Self::DelistedKlines)
            }
            CommandRequest::Klines(request) => {
                // Category 22 shares the 0x052d frame but returns the historical
                // money-flow series rather than OHLC bars.
                if request.period.raw == HISTORY_FUND_FLOW_CATEGORY {
                    parse_history_fund_flow_payload_with_context(payload.clone(), request, options)
                        .map(Self::HistoryFundFlow)
                } else {
                    parse_klines_payload_with_context(payload.clone(), request, options)
                        .map(Self::Klines)
                }
            }
            CommandRequest::TodayIntraday(request) => {
                parse_today_intraday_payload_with_context(payload.clone(), request, options)
                    .map(Self::TodayIntraday)
            }
            CommandRequest::LegacyQuotes(request) => {
                parse_legacy_quotes_payload_with_context(payload.clone(), &request.codes, options)
                    .map(Self::LegacyQuotes)
            }
            CommandRequest::RefreshStream(request) => {
                let requested_codes = request.requested_codes();
                parse_refresh_stream_payload_with_context(
                    payload.clone(),
                    &requested_codes,
                    options,
                )
                .map(Self::RefreshStream)
            }
            CommandRequest::CategoryQuotes(request) => {
                parse_category_quotes_payload_with_context(payload.clone(), request, options)
                    .map(Self::CategoryQuotes)
            }
            CommandRequest::Snapshots(request) => {
                parse_snapshots_payload_with_context(payload.clone(), &request.codes, options)
                    .map(Self::Snapshots)
            }
            CommandRequest::AuctionSeries(request) => {
                parse_auction_series_payload_with_context(payload.clone(), request, options)
                    .map(Self::AuctionSeries)
            }
            CommandRequest::FileContent(request) => {
                parse_file_content_payload_with_context(payload.clone(), request, options)
                    .map(Self::FileContent)
            }
            CommandRequest::HistoricalIntraday(request) => {
                parse_historical_intraday_payload_with_context(payload.clone(), request, options)
                    .map(Self::HistoricalIntraday)
            }
            CommandRequest::TodayTicks(request) => {
                parse_today_ticks_payload_with_context(payload.clone(), request, options)
                    .map(Self::TodayTicks)
            }
            CommandRequest::HistoricalTicks(request) => {
                parse_historical_ticks_payload_with_context(payload.clone(), request, options)
                    .map(Self::HistoricalTicks)
            }
            CommandRequest::Sparkline(request) => {
                parse_sparkline_payload_with_context(payload.clone(), request, options)
                    .map(Self::Sparkline)
            }
            CommandRequest::RecentIntraday(request) => {
                parse_recent_intraday_payload_with_context(payload.clone(), request, options)
                    .map(Self::RecentIntraday)
            }
            CommandRequest::MoneyFlow(request) => {
                parse_money_flow_payload_with_context(payload.clone(), request, options)
                    .map(Self::MoneyFlow)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CommandResponse;
    use crate::commands::session::HeartbeatRequest;
    use crate::request::CommandRequest;
    use crate::ProtocolError;

    #[test]
    fn aggregate_parser_dispatches_without_dynamic_payloads() -> Result<(), ProtocolError> {
        let payload = [0, 0, 0, 0, 0, 0, 0xa8, 0x26, 0x35, 0x01];
        let response =
            CommandResponse::parse(CommandRequest::Heartbeat(HeartbeatRequest), &payload)?;
        assert!(matches!(
            response,
            CommandResponse::Heartbeat(value) if value.server_date_raw == 20_260_520
        ));
        Ok(())
    }
}
