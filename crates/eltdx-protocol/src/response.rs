use crate::commands::{
    auctions::{parse_auction_series_payload, AuctionSeries},
    corporate::{
        parse_capital_changes_payload, parse_finance_batch_payload, CapitalChangeBatch,
        FinanceBatch,
    },
    klines::{parse_klines_payload, KlineSeries},
    limits::{parse_special_limits_payload, SpecialLimitPage},
    minutes::{
        parse_historical_intraday_payload, parse_intraday_aux_payload,
        parse_recent_intraday_payload, parse_sparkline_payload, parse_today_intraday_payload,
        HistoricalIntradayRequest, MinuteAuxSeries, MinuteSeries, RecentIntradayRequest,
        SparklineSeries, TodayIntradayRequest,
    },
    quotes::{
        parse_category_quotes_payload, parse_legacy_quotes_payload, parse_refresh_stream_payload,
        parse_snapshots_payload, CategoryQuotePage, LegacyQuote, QuoteRefreshPage, QuoteSnapshot,
    },
    resources::{parse_file_content_payload, FileContentChunk},
    security::{parse_security_count_payload, parse_security_list_payload, SecurityCode},
    session::{parse_handshake_payload, parse_heartbeat_payload, HandshakeInfo, HeartbeatAck},
    trades::{
        parse_historical_ticks_payload, parse_today_ticks_payload, HistoricalTicksRequest,
        TodayTicksRequest, TradePage,
    },
};
use crate::error::ProtocolError;
use crate::request::CommandRequest;

#[derive(Clone, Debug, PartialEq)]
pub enum CommandResponse {
    Heartbeat(HeartbeatAck),
    Handshake(HandshakeInfo),
    CapitalChanges(CapitalChangeBatch),
    FinanceBatch(FinanceBatch),
    SecurityList(Vec<SecurityCode>),
    SecurityCount(u16),
    SpecialLimits(SpecialLimitPage),
    IntradayAux(MinuteAuxSeries),
    Klines(KlineSeries),
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
            CommandRequest::Klines(request) => {
                parse_klines_payload(payload, request).map(Self::Klines)
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
