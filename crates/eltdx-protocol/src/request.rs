use crate::commands::{
    auctions::{AuctionSeriesRequest, TYPE_AUCTION_SERIES},
    corporate::{
        CapitalChangesRequest, FinanceBatchRequest, TYPE_CAPITAL_CHANGES, TYPE_FINANCE_BATCH,
    },
    klines::{KlinesRequest, TYPE_KLINES},
    limits::{SpecialLimitsRequest, TYPE_SPECIAL_LIMITS},
    minutes::{
        HistoricalIntradayRequest, IntradayAuxRequest, RecentIntradayRequest, SparklineRequest,
        TodayIntradayRequest, TYPE_HISTORICAL_INTRADAY, TYPE_INTRADAY_AUX, TYPE_RECENT_INTRADAY,
        TYPE_SPARKLINE, TYPE_TODAY_INTRADAY,
    },
    money_flow::{MoneyFlowRequest, TYPE_MONEY_FLOW},
    quotes::{
        CategoryQuotesRequest, LegacyQuotesRequest, RefreshStreamRequest, SnapshotsRequest,
        TYPE_CATEGORY_QUOTES, TYPE_LEGACY_QUOTES, TYPE_REFRESH_STREAM, TYPE_SNAPSHOTS,
    },
    resources::{FileContentRequest, TYPE_FILE_CONTENT},
    security::{
        SecurityCountRequest, SecurityListRequest, TYPE_SECURITY_COUNT, TYPE_SECURITY_LIST,
    },
    session::{HandshakeRequest, HeartbeatRequest, TYPE_HANDSHAKE, TYPE_HEARTBEAT},
    trades::{HistoricalTicksRequest, TodayTicksRequest, TYPE_HISTORICAL_TICKS, TYPE_TODAY_TICKS},
};
use crate::error::ProtocolError;
use crate::frame::RequestFrame;

pub const SUPPORTED_COMMAND_CODES: [u16; 22] = [
    TYPE_HEARTBEAT,
    TYPE_HANDSHAKE,
    TYPE_CAPITAL_CHANGES,
    TYPE_FINANCE_BATCH,
    TYPE_SECURITY_LIST,
    TYPE_SECURITY_COUNT,
    TYPE_SPECIAL_LIMITS,
    TYPE_INTRADAY_AUX,
    TYPE_KLINES,
    TYPE_TODAY_INTRADAY,
    TYPE_LEGACY_QUOTES,
    TYPE_REFRESH_STREAM,
    TYPE_CATEGORY_QUOTES,
    TYPE_SNAPSHOTS,
    TYPE_AUCTION_SERIES,
    TYPE_FILE_CONTENT,
    TYPE_HISTORICAL_INTRADAY,
    TYPE_TODAY_TICKS,
    TYPE_HISTORICAL_TICKS,
    TYPE_SPARKLINE,
    TYPE_RECENT_INTRADAY,
    TYPE_MONEY_FLOW,
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandRequest {
    Heartbeat(HeartbeatRequest),
    Handshake(HandshakeRequest),
    CapitalChanges(CapitalChangesRequest),
    FinanceBatch(FinanceBatchRequest),
    SecurityList(SecurityListRequest),
    SecurityCount(SecurityCountRequest),
    SpecialLimits(SpecialLimitsRequest),
    IntradayAux(IntradayAuxRequest),
    Klines(KlinesRequest),
    TodayIntraday(TodayIntradayRequest),
    LegacyQuotes(LegacyQuotesRequest),
    RefreshStream(RefreshStreamRequest),
    CategoryQuotes(CategoryQuotesRequest),
    Snapshots(SnapshotsRequest),
    AuctionSeries(AuctionSeriesRequest),
    FileContent(FileContentRequest),
    HistoricalIntraday(HistoricalIntradayRequest),
    TodayTicks(TodayTicksRequest),
    HistoricalTicks(HistoricalTicksRequest),
    Sparkline(SparklineRequest),
    RecentIntraday(RecentIntradayRequest),
    MoneyFlow(MoneyFlowRequest),
}

impl CommandRequest {
    pub const fn command_code(&self) -> u16 {
        match self {
            Self::Heartbeat(_) => TYPE_HEARTBEAT,
            Self::Handshake(_) => TYPE_HANDSHAKE,
            Self::CapitalChanges(_) => TYPE_CAPITAL_CHANGES,
            Self::FinanceBatch(_) => TYPE_FINANCE_BATCH,
            Self::SecurityList(_) => TYPE_SECURITY_LIST,
            Self::SecurityCount(_) => TYPE_SECURITY_COUNT,
            Self::SpecialLimits(_) => TYPE_SPECIAL_LIMITS,
            Self::IntradayAux(_) => TYPE_INTRADAY_AUX,
            Self::Klines(_) => TYPE_KLINES,
            Self::TodayIntraday(_) => TYPE_TODAY_INTRADAY,
            Self::LegacyQuotes(_) => TYPE_LEGACY_QUOTES,
            Self::RefreshStream(_) => TYPE_REFRESH_STREAM,
            Self::CategoryQuotes(_) => TYPE_CATEGORY_QUOTES,
            Self::Snapshots(_) => TYPE_SNAPSHOTS,
            Self::AuctionSeries(_) => TYPE_AUCTION_SERIES,
            Self::FileContent(_) => TYPE_FILE_CONTENT,
            Self::HistoricalIntraday(_) => TYPE_HISTORICAL_INTRADAY,
            Self::TodayTicks(_) => TYPE_TODAY_TICKS,
            Self::HistoricalTicks(_) => TYPE_HISTORICAL_TICKS,
            Self::Sparkline(_) => TYPE_SPARKLINE,
            Self::RecentIntraday(_) => TYPE_RECENT_INTRADAY,
            Self::MoneyFlow(_) => TYPE_MONEY_FLOW,
        }
    }

    pub const fn retry_safe(&self) -> bool {
        true
    }

    pub fn frame(&self, msg_id: u32) -> Result<RequestFrame, ProtocolError> {
        let frame = match self {
            Self::Heartbeat(request) => request.frame(msg_id),
            Self::Handshake(request) => request.frame(msg_id),
            Self::CapitalChanges(request) => request.frame(msg_id),
            Self::FinanceBatch(request) => request.frame(msg_id),
            Self::SecurityList(request) => request.frame(msg_id),
            Self::SecurityCount(request) => request.frame(msg_id),
            Self::SpecialLimits(request) => request.frame(msg_id),
            Self::IntradayAux(request) => request.frame(msg_id),
            Self::Klines(request) => request.frame(msg_id),
            Self::TodayIntraday(request) => request.frame(msg_id),
            Self::LegacyQuotes(request) => return request.frame(msg_id),
            Self::RefreshStream(request) => return request.frame(msg_id),
            Self::CategoryQuotes(request) => request.frame(msg_id),
            Self::Snapshots(request) => return request.frame(msg_id),
            Self::AuctionSeries(request) => request.frame(msg_id),
            Self::FileContent(request) => request.frame(msg_id),
            Self::HistoricalIntraday(request) => request.frame(msg_id),
            Self::TodayTicks(request) => request.frame(msg_id),
            Self::HistoricalTicks(request) => request.frame(msg_id),
            Self::Sparkline(request) => request.frame(msg_id),
            Self::RecentIntraday(request) => request.frame(msg_id),
            Self::MoneyFlow(request) => request.frame(msg_id),
        };
        Ok(frame)
    }
}

pub const fn is_supported_command(command: u16) -> bool {
    matches!(
        command,
        TYPE_HEARTBEAT
            | TYPE_HANDSHAKE
            | TYPE_CAPITAL_CHANGES
            | TYPE_FINANCE_BATCH
            | TYPE_SECURITY_LIST
            | TYPE_SECURITY_COUNT
            | TYPE_SPECIAL_LIMITS
            | TYPE_INTRADAY_AUX
            | TYPE_KLINES
            | TYPE_TODAY_INTRADAY
            | TYPE_LEGACY_QUOTES
            | TYPE_REFRESH_STREAM
            | TYPE_CATEGORY_QUOTES
            | TYPE_SNAPSHOTS
            | TYPE_AUCTION_SERIES
            | TYPE_FILE_CONTENT
            | TYPE_HISTORICAL_INTRADAY
            | TYPE_TODAY_TICKS
            | TYPE_HISTORICAL_TICKS
            | TYPE_SPARKLINE
            | TYPE_RECENT_INTRADAY
            | TYPE_MONEY_FLOW
    )
}

#[cfg(test)]
mod tests {
    use super::{is_supported_command, CommandRequest, SUPPORTED_COMMAND_CODES};
    use crate::commands::{
        auctions::AuctionSeriesRequest,
        corporate::{CapitalChangesRequest, FinanceBatchRequest},
        klines::{KlineKind, KlinesRequest},
        limits::SpecialLimitsRequest,
        minutes::{
            HistoricalIntradayRequest, IntradayAuxKind, IntradayAuxRequest, RecentIntradayRequest,
            SparklineRequest, TodayIntradayRequest, DEFAULT_SPARKLINE_FIXED_RAW,
            DEFAULT_TODAY_RESERVED_TAIL,
        },
        money_flow::MoneyFlowRequest,
        quotes::{
            CategoryQuotesRequest, LegacyQuotesRequest, RefreshStreamRequest, SnapshotsRequest,
        },
        resources::FileContentRequest,
        security::{SecurityCountRequest, SecurityListRequest},
        session::{HandshakeRequest, HeartbeatRequest},
        trades::{HistoricalTicksRequest, TodayTicksRequest},
    };
    use crate::unit::{AdjustMode, DateParts, KlinePeriod, Market, NormalizedCode};
    use crate::ProtocolError;

    #[test]
    fn exact_twenty_one_command_codes_are_registered() {
        assert_eq!(SUPPORTED_COMMAND_CODES.len(), 22);
        assert!(SUPPORTED_COMMAND_CODES
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
        assert!(SUPPORTED_COMMAND_CODES
            .iter()
            .all(|code| is_supported_command(*code)));
        assert!(!is_supported_command(0x9999));
    }

    #[test]
    fn every_request_variant_builds_its_registered_type() -> Result<(), ProtocolError> {
        for request in all_requests()? {
            let frame = request.frame(0x1234_5678)?;
            assert_eq!(frame.msg_type, request.command_code());
            assert!(request.retry_safe());
        }
        Ok(())
    }

    #[test]
    fn raw_capable_requests_retain_default_and_explicit_context() -> Result<(), ProtocolError> {
        let defaults = all_requests()?;
        for request in defaults {
            match request {
                CommandRequest::CapitalChanges(value) => assert!(!value.include_raw),
                CommandRequest::FinanceBatch(value) => assert!(!value.include_raw()),
                CommandRequest::IntradayAux(value) => assert!(!value.include_raw),
                CommandRequest::Klines(value) => assert!(!value.include_raw),
                CommandRequest::TodayIntraday(value) => assert!(!value.include_raw),
                CommandRequest::AuctionSeries(value) => assert!(!value.include_raw),
                CommandRequest::HistoricalIntraday(value) => assert!(!value.include_raw),
                CommandRequest::TodayTicks(value) => assert!(!value.include_raw),
                CommandRequest::HistoricalTicks(value) => assert!(!value.include_raw),
                CommandRequest::Sparkline(value) => assert!(!value.include_raw),
                CommandRequest::RecentIntraday(value) => assert!(!value.include_raw),
                _ => {}
            }
        }

        for request in raw_requests()? {
            let retained = match request {
                CommandRequest::CapitalChanges(value) => value.include_raw,
                CommandRequest::FinanceBatch(value) => value.include_raw(),
                CommandRequest::IntradayAux(value) => value.include_raw,
                CommandRequest::Klines(value) => value.include_raw,
                CommandRequest::TodayIntraday(value) => value.include_raw,
                CommandRequest::AuctionSeries(value) => value.include_raw,
                CommandRequest::HistoricalIntraday(value) => value.include_raw,
                CommandRequest::TodayTicks(value) => value.include_raw,
                CommandRequest::HistoricalTicks(value) => value.include_raw,
                CommandRequest::Sparkline(value) => value.include_raw,
                CommandRequest::RecentIntraday(value) => value.include_raw,
                _ => false,
            };
            assert!(retained);
        }
        Ok(())
    }

    fn all_requests() -> Result<Vec<CommandRequest>, ProtocolError> {
        let code = NormalizedCode::parse("sz000001")?;
        let date = DateParts::new(2026, 8, 15)?;
        Ok(vec![
            CommandRequest::Heartbeat(HeartbeatRequest),
            CommandRequest::Handshake(HandshakeRequest),
            CommandRequest::CapitalChanges(CapitalChangesRequest::new(code.clone())),
            CommandRequest::FinanceBatch(FinanceBatchRequest::new(vec![code.clone()])?),
            CommandRequest::SecurityList(SecurityListRequest::new(Market::Shenzhen, 0, 1)?),
            CommandRequest::SecurityCount(SecurityCountRequest {
                market: Market::Shenzhen,
                client_date: 0,
            }),
            CommandRequest::SpecialLimits(SpecialLimitsRequest::new(0)),
            CommandRequest::IntradayAux(IntradayAuxRequest::new(
                code.clone(),
                IntradayAuxKind::BuySellStrength,
            )),
            CommandRequest::Klines(KlinesRequest::new(
                code.clone(),
                KlinePeriod::normalize("day")?,
                0,
                1,
                AdjustMode::None,
                0,
                KlineKind::Stock,
            )?),
            CommandRequest::TodayIntraday(TodayIntradayRequest::new(
                code.clone(),
                DEFAULT_TODAY_RESERVED_TAIL,
            )),
            CommandRequest::LegacyQuotes(LegacyQuotesRequest::new(vec![code.clone()])?),
            CommandRequest::RefreshStream(RefreshStreamRequest::new(Vec::new())?),
            CommandRequest::CategoryQuotes(CategoryQuotesRequest::new(6, 0, 0, 1, false, None, 0)),
            CommandRequest::Snapshots(SnapshotsRequest::new(vec![code.clone()])?),
            CommandRequest::AuctionSeries(AuctionSeriesRequest::with_defaults(code.clone())),
            CommandRequest::FileContent(FileContentRequest::with_defaults("zhb.zip")?),
            CommandRequest::HistoricalIntraday(HistoricalIntradayRequest::new(code.clone(), date)?),
            CommandRequest::TodayTicks(TodayTicksRequest::new(code.clone(), 0, 1)?),
            CommandRequest::HistoricalTicks(HistoricalTicksRequest::new(code.clone(), date, 0, 1)?),
            CommandRequest::Sparkline(SparklineRequest::new(
                code.clone(),
                1,
                20,
                DEFAULT_SPARKLINE_FIXED_RAW,
            )),
            CommandRequest::RecentIntraday(RecentIntradayRequest::new(code, date)?),
            CommandRequest::MoneyFlow(MoneyFlowRequest::new(NormalizedCode::parse("sz000001")?)),
        ])
    }

    fn raw_requests() -> Result<Vec<CommandRequest>, ProtocolError> {
        let code = NormalizedCode::parse("sz000001")?;
        let date = DateParts::new(2026, 8, 15)?;
        Ok(vec![
            CommandRequest::CapitalChanges(CapitalChangesRequest::with_include_raw(
                code.clone(),
                true,
            )),
            CommandRequest::FinanceBatch(FinanceBatchRequest::with_include_raw(
                vec![code.clone()],
                true,
            )?),
            CommandRequest::IntradayAux(IntradayAuxRequest::with_include_raw(
                code.clone(),
                IntradayAuxKind::BuySellStrength,
                true,
            )),
            CommandRequest::Klines(KlinesRequest::with_include_raw(
                code.clone(),
                KlinePeriod::normalize("day")?,
                0,
                1,
                AdjustMode::None,
                0,
                KlineKind::Stock,
                true,
            )?),
            CommandRequest::TodayIntraday(TodayIntradayRequest::with_include_raw(
                code.clone(),
                DEFAULT_TODAY_RESERVED_TAIL,
                true,
            )),
            CommandRequest::AuctionSeries(AuctionSeriesRequest::with_include_raw(
                code.clone(),
                3,
                0,
                500,
                true,
            )),
            CommandRequest::HistoricalIntraday(HistoricalIntradayRequest::with_include_raw(
                code.clone(),
                date,
                true,
            )?),
            CommandRequest::TodayTicks(TodayTicksRequest::with_include_raw(
                code.clone(),
                0,
                1,
                true,
            )?),
            CommandRequest::HistoricalTicks(HistoricalTicksRequest::with_include_raw(
                code.clone(),
                date,
                0,
                1,
                true,
            )?),
            CommandRequest::Sparkline(SparklineRequest::with_include_raw(
                code.clone(),
                1,
                20,
                DEFAULT_SPARKLINE_FIXED_RAW,
                true,
            )),
            CommandRequest::RecentIntraday(RecentIntradayRequest::with_include_raw(
                code, date, true,
            )?),
        ])
    }
}
