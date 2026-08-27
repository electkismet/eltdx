use eltdx_protocol::commands::{
    auctions::{
        AuctionSeriesRequest, DEFAULT_AUCTION_LIMIT, DEFAULT_AUCTION_SELECTOR,
        DEFAULT_AUCTION_START, TYPE_AUCTION_SERIES,
    },
    corporate::{
        CapitalChangesRequest, FinanceBatchRequest, TYPE_CAPITAL_CHANGES, TYPE_FINANCE_BATCH,
    },
    klines::{KlineKind, KlinesRequest, TYPE_KLINES},
    limits::{SpecialLimitsRequest, TYPE_SPECIAL_LIMITS},
    minutes::{
        HistoricalIntradayRequest, IntradayAuxKind, IntradayAuxRequest, RecentIntradayRequest,
        SparklineRequest, TodayIntradayRequest, DEFAULT_SPARKLINE_FIXED_RAW,
        DEFAULT_SPARKLINE_SELECTOR, DEFAULT_SPARKLINE_WINDOW, DEFAULT_TODAY_RESERVED_TAIL,
        TYPE_HISTORICAL_INTRADAY, TYPE_INTRADAY_AUX, TYPE_RECENT_INTRADAY, TYPE_SPARKLINE,
        TYPE_TODAY_INTRADAY,
    },
    quotes::{
        normalize_category, normalize_sort_type, CategoryQuotesRequest, LegacyQuotesRequest,
        RefreshCursor, RefreshStreamRequest, SnapshotsRequest, TYPE_CATEGORY_QUOTES,
        TYPE_LEGACY_QUOTES, TYPE_REFRESH_STREAM, TYPE_SNAPSHOTS,
    },
    resources::{FileContentRequest, TYPE_FILE_CONTENT},
    security::{
        SecurityCountRequest, SecurityListRequest, TYPE_SECURITY_COUNT, TYPE_SECURITY_LIST,
    },
    session::{HandshakeRequest, HeartbeatRequest, TYPE_HANDSHAKE, TYPE_HEARTBEAT},
    trades::{HistoricalTicksRequest, TodayTicksRequest, TYPE_HISTORICAL_TICKS, TYPE_TODAY_TICKS},
};
use eltdx_protocol::limits::{
    DEFAULT_CODE_PAGE_SIZE, DEFAULT_FILE_CHUNK_SIZE, DEFAULT_TRADE_PAGE_SIZE,
    MAX_CAPITAL_CHANGE_CODES, MAX_CODE_PAGE_SIZE, MAX_COMMAND_ITEMS, MAX_FILE_CHUNK_SIZE,
    MAX_KLINE_PAGE_SIZE, MAX_REFRESH_CODES, MAX_TRADE_PAGE_SIZE,
};
use eltdx_protocol::unit::{AdjustMode, DateParts, KlinePeriod, Market, NormalizedCode};
use eltdx_protocol::{CommandRequest, ProtocolError};
use eltdx_runtime::RuntimeError;
use pyo3::exceptions::PyOverflowError;
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyInt, PyString};

use crate::error;

const MAX_PAYLOAD_FIELDS: usize = 32;
const MAX_ARGUMENT_CHARS: usize = 1_024;

type Payload<'a, 'py> = Option<&'a Bound<'py, PyDict>>;

pub fn from_python(
    py: Python<'_>,
    command: u16,
    payload: Option<&Bound<'_, PyDict>>,
) -> PyResult<CommandRequest> {
    if payload.is_some_and(|value| value.len() > MAX_PAYLOAD_FIELDS) {
        return Err(value_error(
            "payload",
            format!("payload contains more than {MAX_PAYLOAD_FIELDS} fields"),
        ));
    }
    let payload_copy = payload.map(|value| value.copy()).transpose()?;
    let payload = payload_copy.as_ref();
    match command {
        TYPE_HEARTBEAT => Ok(CommandRequest::Heartbeat(HeartbeatRequest)),
        TYPE_HANDSHAKE => Ok(CommandRequest::Handshake(HandshakeRequest)),
        TYPE_CAPITAL_CHANGES => capital_changes(payload),
        TYPE_FINANCE_BATCH => finance_batch(payload),
        TYPE_SECURITY_LIST => security_list(payload),
        TYPE_SECURITY_COUNT => security_count(py, payload),
        TYPE_SPECIAL_LIMITS => special_limits(payload),
        TYPE_INTRADAY_AUX => intraday_aux(payload),
        TYPE_KLINES => klines(payload),
        TYPE_TODAY_INTRADAY => today_intraday(payload),
        TYPE_LEGACY_QUOTES => legacy_quotes(payload),
        TYPE_REFRESH_STREAM => refresh_stream(payload),
        TYPE_CATEGORY_QUOTES => category_quotes(payload),
        TYPE_SNAPSHOTS => snapshots(payload),
        TYPE_AUCTION_SERIES => auction_series(payload),
        TYPE_FILE_CONTENT => file_content(payload),
        TYPE_HISTORICAL_INTRADAY => historical_intraday(payload),
        TYPE_TODAY_TICKS => today_ticks(payload),
        TYPE_HISTORICAL_TICKS => historical_ticks(payload),
        TYPE_SPARKLINE => sparkline(payload),
        TYPE_RECENT_INTRADAY => recent_intraday(py, payload),
        _ => Err(error::from_runtime(RuntimeError::unsupported_command(
            command,
        ))),
    }
}

fn capital_changes(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let codes = if let Some(value) = get(payload, &["codes"])? {
        code_list(&value, "codes", MAX_CAPITAL_CHANGE_CODES, true)?
    } else {
        vec![required_code(payload, "code")?]
    };
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(CapitalChangesRequest::with_include_raw_batch(
        codes,
        include_raw,
    ))
        .map(CommandRequest::CapitalChanges)
}

fn finance_batch(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let value = required(payload, &["codes"], "codes")?;
    let codes = code_list(&value, "codes", MAX_COMMAND_ITEMS, false)?;
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(FinanceBatchRequest::with_include_raw(codes, include_raw))
        .map(CommandRequest::FinanceBatch)
}

fn security_list(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let market = required_market(payload, &["market", "market_id"])?;
    let start = u32_field(payload, &["start"], 0, 0, u32::MAX)?;
    let limit = u16_field(
        payload,
        &["limit"],
        DEFAULT_CODE_PAGE_SIZE,
        0,
        MAX_CODE_PAGE_SIZE,
    )?;
    protocol(SecurityListRequest::new(market, start, limit)).map(CommandRequest::SecurityList)
}

fn security_count(py: Python<'_>, payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let market = required_market(payload, &["market", "market_id"])?;
    let client_date = date_raw_or_today(
        py,
        get(payload, &["client_date_yyyymmdd", "client_date"])?,
        "client_date",
    )?;
    Ok(CommandRequest::SecurityCount(SecurityCountRequest {
        market,
        client_date,
    }))
}

fn special_limits(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let start_index = u16_field(payload, &["start_index"], 0, 0, u16::MAX)?;
    Ok(CommandRequest::SpecialLimits(SpecialLimitsRequest::new(
        start_index,
    )))
}

fn intraday_aux(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let kind = match get(payload, &["selector", "kind"])? {
        Some(value) if value.is_instance_of::<PyString>() => {
            let text = string_value(&value, "kind")?;
            protocol(IntradayAuxKind::normalize(&text))?
        }
        Some(value) => IntradayAuxKind::from_raw(u8_value(&value, "kind", 0, u8::MAX)?),
        None => IntradayAuxKind::BuySellStrength,
    };
    let include_raw = bool_field(payload, "include_raw", false)?;
    Ok(CommandRequest::IntradayAux(
        IntradayAuxRequest::with_include_raw(code, kind, include_raw),
    ))
}

fn klines(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let period = match get(payload, &["period"])? {
        Some(value) => period_value(&value)?,
        None => protocol(KlinePeriod::normalize("day"))?,
    };
    let start = u16_field(payload, &["start"], 0, 0, u16::MAX)?;
    let count = u16_field(
        payload,
        &["count"],
        MAX_KLINE_PAGE_SIZE,
        1,
        MAX_KLINE_PAGE_SIZE,
    )?;
    let adjust = adjust_value(get(payload, &["adjust"])?)?;
    let anchor_date_raw = anchor_date_raw(get(payload, &["anchor_date", "anchor_date_raw"])?)?;
    let kind = kline_kind(get(payload, &["kind"])?)?;
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(KlinesRequest::with_include_raw(
        code,
        period,
        start,
        count,
        adjust,
        anchor_date_raw,
        kind,
        include_raw,
    ))
    .map(CommandRequest::Klines)
}

fn today_intraday(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let reserved_tail_raw = match get(payload, &["reserved_tail_raw"])? {
        Some(value) => four_bytes(&value, "reserved_tail_raw")?,
        None => DEFAULT_TODAY_RESERVED_TAIL,
    };
    let include_raw = bool_field(payload, "include_raw", false)?;
    Ok(CommandRequest::TodayIntraday(
        TodayIntradayRequest::with_include_raw(code, reserved_tail_raw, include_raw),
    ))
}

fn legacy_quotes(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let value = required(payload, &["codes"], "codes")?;
    let codes = code_list(&value, "codes", MAX_COMMAND_ITEMS, true)?;
    protocol(LegacyQuotesRequest::new(codes)).map(CommandRequest::LegacyQuotes)
}

fn refresh_stream(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let codes = match get(payload, &["codes"])? {
        Some(value) if !value.is_none() => code_list(&value, "codes", MAX_REFRESH_CODES, false)?,
        _ => Vec::new(),
    };
    let cursors = cursor_inputs(get(payload, &["cursors"])?)?;
    let mut items = Vec::with_capacity(codes.len().saturating_add(cursors.len()));
    for code in codes {
        let full_code = code.full_code();
        let number = code.number();
        let cursor = cursors
            .iter()
            .find(|item| item.raw_key == full_code)
            .or_else(|| cursors.iter().find(|item| item.raw_key == number))
            .map_or(0, |item| item.cursor);
        items.push(RefreshCursor { code, cursor });
    }
    for cursor in cursors {
        if items.iter().any(|item| item.code == cursor.code) {
            continue;
        }
        if items.len() >= MAX_REFRESH_CODES {
            return Err(value_error(
                "codes",
                format!("refresh_stream accepts at most {MAX_REFRESH_CODES} codes per request"),
            ));
        }
        items.push(RefreshCursor {
            code: cursor.code,
            cursor: cursor.cursor,
        });
    }
    protocol(RefreshStreamRequest::new(items)).map(CommandRequest::RefreshStream)
}

fn category_quotes(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let category_value = required(payload, &["category"], "category")?;
    let category = category_value_to_u16(&category_value)?;
    let sort_type = match get(payload, &["sort_type", "sort_by"])? {
        Some(value) if value.is_none() => 0,
        Some(value) if value.is_instance_of::<PyString>() => {
            let text = string_value(&value, "sort_by")?;
            protocol(normalize_sort_type(Some(&text)))?
        }
        Some(value) => u16_value(&value, "sort_by", 0, u16::MAX)?,
        None => 0,
    };
    let start = u16_field(payload, &["start"], 0, 0, u16::MAX)?;
    let count = u16_field(payload, &["count"], 80, 0, u16::MAX)?;
    let ascending = bool_field(payload, "ascending", false)?;
    let sort_reverse = match get(payload, &["sort_reverse"])? {
        Some(value) => Some(u16_value(&value, "sort_reverse", 0, u16::MAX)?),
        None => None,
    };
    let filter_raw = u16_field(payload, &["filter_raw"], 0, 0, u16::MAX)?;
    Ok(CommandRequest::CategoryQuotes(CategoryQuotesRequest::new(
        category,
        sort_type,
        start,
        count,
        ascending,
        sort_reverse,
        filter_raw,
    )))
}

fn snapshots(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let value = required(payload, &["codes"], "codes")?;
    let codes = code_list(&value, "codes", MAX_COMMAND_ITEMS, false)?;
    protocol(SnapshotsRequest::new(codes)).map(CommandRequest::Snapshots)
}

fn auction_series(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let trading_date = optional_date(payload, &["trading_date", "date"], "trading_date")?;
    let selector = u32_field(
        payload,
        &["mode_or_selector_raw", "selector"],
        DEFAULT_AUCTION_SELECTOR,
        0,
        u32::MAX,
    )?;
    let start = u32_field(
        payload,
        &["start_raw", "start"],
        DEFAULT_AUCTION_START,
        0,
        u32::MAX,
    )?;
    let limit = u32_field(
        payload,
        &["limit_or_count_raw", "limit"],
        DEFAULT_AUCTION_LIMIT,
        0,
        u32::MAX,
    )?;
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(AuctionSeriesRequest::with_trading_date_and_include_raw(
        code,
        trading_date,
        selector,
        start,
        limit,
        include_raw,
    ))
    .map(CommandRequest::AuctionSeries)
}

fn file_content(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let value = required(payload, &["path"], "path")?;
    let path = string_value(&value, "path")?;
    let offset = u32_field(payload, &["offset"], 0, 0, u32::MAX)?;
    let size = u32_field(
        payload,
        &["size"],
        DEFAULT_FILE_CHUNK_SIZE,
        1,
        MAX_FILE_CHUNK_SIZE,
    )?;
    protocol(FileContentRequest::new(&path, offset, size)).map(CommandRequest::FileContent)
}

fn historical_intraday(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let trading_date = required_date(payload, "trading_date")?;
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(HistoricalIntradayRequest::with_include_raw(
        code,
        trading_date,
        include_raw,
    ))
    .map(CommandRequest::HistoricalIntraday)
}

fn today_ticks(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let start = u16_field(payload, &["start"], 0, 0, u16::MAX)?;
    let count = u16_field(
        payload,
        &["count"],
        DEFAULT_TRADE_PAGE_SIZE,
        1,
        MAX_TRADE_PAGE_SIZE,
    )?;
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(TodayTicksRequest::with_include_raw(
        code,
        start,
        count,
        include_raw,
    ))
    .map(CommandRequest::TodayTicks)
}

fn historical_ticks(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let trading_date = required_date(payload, "trading_date")?;
    let start = u16_field(payload, &["start"], 0, 0, u16::MAX)?;
    let count = u16_field(
        payload,
        &["count"],
        DEFAULT_TRADE_PAGE_SIZE,
        1,
        MAX_TRADE_PAGE_SIZE,
    )?;
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(HistoricalTicksRequest::with_include_raw(
        code,
        trading_date,
        start,
        count,
        include_raw,
    ))
    .map(CommandRequest::HistoricalTicks)
}

fn sparkline(payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let selector = u8_field(
        payload,
        &["selector"],
        DEFAULT_SPARKLINE_SELECTOR,
        0,
        u8::MAX,
    )?;
    let window = u16_field(
        payload,
        &["window", "window_or_count_raw"],
        DEFAULT_SPARKLINE_WINDOW,
        0,
        u16::MAX,
    )?;
    let fixed_raw = u32_field(
        payload,
        &["fixed_raw"],
        DEFAULT_SPARKLINE_FIXED_RAW,
        0,
        u32::MAX,
    )?;
    let include_raw = bool_field(payload, "include_raw", false)?;
    Ok(CommandRequest::Sparkline(
        SparklineRequest::with_include_raw(code, selector, window, fixed_raw, include_raw),
    ))
}

fn recent_intraday(py: Python<'_>, payload: Payload<'_, '_>) -> PyResult<CommandRequest> {
    let code = required_code(payload, "code")?;
    let trading_date = match get(payload, &["trading_date"])? {
        Some(value) if !value.is_none() => date_parts(&value, "trading_date")?,
        _ => current_date(py)?,
    };
    let include_raw = bool_field(payload, "include_raw", false)?;
    protocol(RecentIntradayRequest::with_include_raw(
        code,
        trading_date,
        include_raw,
    ))
    .map(CommandRequest::RecentIntraday)
}

fn get<'py>(payload: Payload<'_, 'py>, names: &[&str]) -> PyResult<Option<Bound<'py, PyAny>>> {
    let Some(payload) = payload else {
        return Ok(None);
    };
    for name in names {
        if let Some(value) = payload.get_item(*name)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

fn required<'py>(
    payload: Payload<'_, 'py>,
    names: &[&str],
    public_name: &'static str,
) -> PyResult<Bound<'py, PyAny>> {
    match get(payload, names)? {
        Some(value) if !value.is_none() => Ok(value),
        _ => Err(type_error(
            public_name,
            format!("missing required argument: {public_name:?}"),
        )),
    }
}

fn required_code(payload: Payload<'_, '_>, name: &'static str) -> PyResult<NormalizedCode> {
    let value = required(payload, &[name], name)?;
    code_value(&value, name)
}

fn code_value(value: &Bound<'_, PyAny>, name: &'static str) -> PyResult<NormalizedCode> {
    let text = string_value(value, name)?;
    protocol(NormalizedCode::parse(&text))
}

fn code_list(
    value: &Bound<'_, PyAny>,
    name: &'static str,
    maximum: usize,
    require_nonempty: bool,
) -> PyResult<Vec<NormalizedCode>> {
    if value.is_instance_of::<PyString>() {
        return code_value(value, name).map(|code| vec![code]);
    }
    let mut result = Vec::new();
    let iterator = value
        .try_iter()
        .map_err(|_| type_error(name, format!("{name} must be an iterable of strings")))?;
    for item in iterator {
        if result.len() >= maximum {
            return Err(value_error(name, format!("{name} contains too many items")));
        }
        result.push(code_value(&item?, name)?);
    }
    if require_nonempty && result.is_empty() {
        return Err(value_error(name, format!("{name} must not be empty")));
    }
    Ok(result)
}

fn required_market(payload: Payload<'_, '_>, names: &[&str]) -> PyResult<Market> {
    let value = required(payload, names, "market")?;
    if value.is_instance_of::<PyString>() {
        let text = string_value(&value, "market")?;
        protocol(Market::normalize(&text))
    } else {
        let raw = integer_value(&value, "market")?;
        let raw = i64::try_from(raw)
            .map_err(|_| value_error("market", "market id is outside the supported range"))?;
        protocol(Market::from_id(raw))
    }
}

fn required_date(payload: Payload<'_, '_>, name: &'static str) -> PyResult<DateParts> {
    let value = required(payload, &[name], name)?;
    date_parts(&value, name)
}

fn optional_date(
    payload: Payload<'_, '_>,
    names: &[&str],
    name: &'static str,
) -> PyResult<Option<DateParts>> {
    match get(payload, names)? {
        Some(value) if !value.is_none() => date_parts(&value, name).map(Some),
        _ => Ok(None),
    }
}

fn date_parts(value: &Bound<'_, PyAny>, name: &'static str) -> PyResult<DateParts> {
    if value.is_instance_of::<PyString>() {
        let text = string_value(value, name)?;
        return protocol(parse_date_text(&text, name));
    }
    if value.is_instance_of::<PyInt>() {
        let raw = u32_value(value, name, 0, u32::MAX)?;
        return DateParts::from_yyyymmdd(raw).ok_or_else(|| {
            protocol_error(ProtocolError::invalid_data(
                "date",
                format!("invalid date: {raw}"),
            ))
        });
    }
    let year_raw = date_attribute(value, "year", name)?;
    let month_raw = date_attribute(value, "month", name)?;
    let day_raw = date_attribute(value, "day", name)?;
    let year = i32::try_from(year_raw)
        .map_err(|_| value_error(name, format!("{name} year is outside the supported range")))?;
    let month = u8::try_from(month_raw)
        .map_err(|_| value_error(name, format!("{name} month is outside the supported range")))?;
    let day = u8::try_from(day_raw)
        .map_err(|_| value_error(name, format!("{name} day is outside the supported range")))?;
    protocol(DateParts::new(year, month, day))
}

fn date_attribute(
    value: &Bound<'_, PyAny>,
    attribute: &'static str,
    name: &'static str,
) -> PyResult<i128> {
    let item = value
        .getattr(attribute)
        .map_err(|_| type_error(name, format!("{name} must be a date, integer, or string")))?;
    integer_value(&item, name)
}

fn current_date(py: Python<'_>) -> PyResult<DateParts> {
    let today = py
        .import("datetime")?
        .getattr("date")?
        .call_method0("today")?;
    date_parts(&today, "date")
}

fn date_raw_or_today(
    py: Python<'_>,
    value: Option<Bound<'_, PyAny>>,
    name: &'static str,
) -> PyResult<u32> {
    let date = match value {
        Some(value) if !value.is_none() => {
            if value.is_instance_of::<PyInt>() {
                return u32_value(&value, name, 0, u32::MAX);
            }
            date_parts(&value, name)?
        }
        _ => current_date(py)?,
    };
    protocol(date.yyyymmdd())
}

fn parse_date_text(value: &str, name: &'static str) -> Result<DateParts, ProtocolError> {
    let compact = value.trim().replace('-', "");
    if compact.len() != 8 || !compact.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ProtocolError::invalid_data(
            "date",
            format!("invalid {name}: {value:?}"),
        ));
    }
    let raw = compact
        .parse::<u32>()
        .map_err(|_| ProtocolError::invalid_data("date", format!("invalid {name}: {value:?}")))?;
    DateParts::from_yyyymmdd(raw)
        .ok_or_else(|| ProtocolError::invalid_data("date", format!("invalid {name}: {value:?}")))
}

fn anchor_date_raw(value: Option<Bound<'_, PyAny>>) -> PyResult<u32> {
    let Some(value) = value else {
        return Ok(0);
    };
    if value.is_none() {
        return Ok(0);
    }
    if value.is_instance_of::<PyInt>() {
        return u32_value(&value, "anchor_date", 0, u32::MAX);
    }
    if value.is_instance_of::<PyString>() && string_value(&value, "anchor_date")?.is_empty() {
        return Ok(0);
    }
    protocol(date_parts(&value, "anchor_date")?.yyyymmdd())
}

fn period_value(value: &Bound<'_, PyAny>) -> PyResult<KlinePeriod> {
    if value.is_instance_of::<PyString>() {
        let text = string_value(value, "period")?;
        return protocol(KlinePeriod::normalize(&text));
    }
    let (raw, parameter) = value
        .extract::<(i128, i128)>()
        .map_err(|source| conversion_error(value, "period", source))?;
    let raw = bounded_u16(raw, "period", 0, u16::MAX)?;
    let parameter = bounded_u16(parameter, "period", 0, u16::MAX)?;
    Ok(KlinePeriod::new(raw, parameter))
}

fn adjust_value(value: Option<Bound<'_, PyAny>>) -> PyResult<AdjustMode> {
    let Some(value) = value else {
        return Ok(AdjustMode::None);
    };
    if value.is_none() {
        return Ok(AdjustMode::None);
    }
    if value.is_instance_of::<PyString>() {
        let text = string_value(&value, "adjust")?;
        return protocol(AdjustMode::normalize(Some(&text)));
    }
    let raw = integer_value(&value, "adjust")?;
    let raw = i64::try_from(raw)
        .map_err(|_| value_error("adjust", "adjust mode is outside the supported range"))?;
    protocol(AdjustMode::from_raw(raw))
}

fn kline_kind(value: Option<Bound<'_, PyAny>>) -> PyResult<KlineKind> {
    let text = match value {
        Some(value) => string_value(&value, "kind")?.trim().to_lowercase(),
        None => "stock".to_owned(),
    };
    match text.as_str() {
        "stock" => Ok(KlineKind::Stock),
        "index" => Ok(KlineKind::Index),
        _ => Err(value_error(
            "kind",
            format!("kind must be 'stock' or 'index', got {text:?}"),
        )),
    }
}

fn category_value_to_u16(value: &Bound<'_, PyAny>) -> PyResult<u16> {
    if value.is_instance_of::<PyString>() {
        let text = string_value(value, "category")?;
        protocol(normalize_category(&text))
    } else {
        u16_value(value, "category", 0, u16::MAX)
    }
}

fn four_bytes(value: &Bound<'_, PyAny>, name: &'static str) -> PyResult<[u8; 4]> {
    if value.is_instance_of::<PyString>() {
        let text = string_value(value, name)?.replace(' ', "");
        if text.len() != 8 || !text.is_ascii() {
            return Err(value_error(name, format!("{name} must be 4 bytes")));
        }
        let mut output = [0_u8; 4];
        for (index, item) in output.iter_mut().enumerate() {
            let start = index.saturating_mul(2);
            *item = u8::from_str_radix(&text[start..start + 2], 16)
                .map_err(|_| value_error(name, format!("{name} must be hexadecimal")))?;
        }
        return Ok(output);
    }
    if value
        .len()
        .map_err(|_| type_error(name, format!("{name} must be bytes")))?
        != 4
    {
        return Err(value_error(name, format!("{name} must be 4 bytes")));
    }
    if let Ok(bytes) = value.cast::<PyBytes>() {
        let slice = bytes.as_bytes();
        return slice
            .try_into()
            .map_err(|_| value_error(name, format!("{name} must be 4 bytes")));
    }
    let bytes = value
        .extract::<Vec<u8>>()
        .map_err(|source| conversion_error(value, name, source))?;
    bytes
        .as_slice()
        .try_into()
        .map_err(|_| value_error(name, format!("{name} must be 4 bytes")))
}

struct CursorInput {
    raw_key: String,
    code: NormalizedCode,
    cursor: u32,
}

fn cursor_inputs(value: Option<Bound<'_, PyAny>>) -> PyResult<Vec<CursorInput>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_none() || !value.is_truthy()? {
        return Ok(Vec::new());
    }
    let dictionary = value
        .cast::<PyDict>()
        .map_err(|_| type_error("cursors", "cursors must be a dict"))?;
    if dictionary.len() > MAX_REFRESH_CODES {
        return Err(value_error(
            "cursors",
            format!("cursors contains more than {MAX_REFRESH_CODES} items"),
        ));
    }
    let dictionary = dictionary.copy()?;
    let mut output = Vec::with_capacity(dictionary.len());
    for (key, value) in dictionary.iter() {
        let raw_key = string_value(&key, "cursors")?;
        let code = protocol(NormalizedCode::parse(&raw_key))?;
        let cursor = u32_value(&value, "cursor", 0, u32::MAX)?;
        output.push(CursorInput {
            raw_key,
            code,
            cursor,
        });
    }
    Ok(output)
}

fn string_value(value: &Bound<'_, PyAny>, name: &'static str) -> PyResult<String> {
    if value.is_instance_of::<PyString>() && value.len()? > MAX_ARGUMENT_CHARS {
        return Err(value_error(
            name,
            format!("{name} exceeds {MAX_ARGUMENT_CHARS} characters"),
        ));
    }
    value
        .extract::<String>()
        .map_err(|source| conversion_error(value, name, source))
}

fn bool_field(payload: Payload<'_, '_>, name: &'static str, default: bool) -> PyResult<bool> {
    match get(payload, &[name])? {
        Some(value) => value.is_truthy(),
        None => Ok(default),
    }
}

fn u8_field(
    payload: Payload<'_, '_>,
    names: &[&'static str],
    default: u8,
    minimum: u8,
    maximum: u8,
) -> PyResult<u8> {
    match get(payload, names)? {
        Some(value) => u8_value(&value, names[0], minimum, maximum),
        None => Ok(default),
    }
}

fn u16_field(
    payload: Payload<'_, '_>,
    names: &[&'static str],
    default: u16,
    minimum: u16,
    maximum: u16,
) -> PyResult<u16> {
    match get(payload, names)? {
        Some(value) => u16_value(&value, names[0], minimum, maximum),
        None => Ok(default),
    }
}

fn u32_field(
    payload: Payload<'_, '_>,
    names: &[&'static str],
    default: u32,
    minimum: u32,
    maximum: u32,
) -> PyResult<u32> {
    match get(payload, names)? {
        Some(value) => u32_value(&value, names[0], minimum, maximum),
        None => Ok(default),
    }
}

fn u8_value(
    value: &Bound<'_, PyAny>,
    name: &'static str,
    minimum: u8,
    maximum: u8,
) -> PyResult<u8> {
    let parsed = integer_value(value, name)?;
    if parsed < i128::from(minimum) || parsed > i128::from(maximum) {
        return Err(range_error(name, minimum, maximum));
    }
    u8::try_from(parsed).map_err(|_| range_error(name, minimum, maximum))
}

fn u16_value(
    value: &Bound<'_, PyAny>,
    name: &'static str,
    minimum: u16,
    maximum: u16,
) -> PyResult<u16> {
    bounded_u16(integer_value(value, name)?, name, minimum, maximum)
}

fn bounded_u16(parsed: i128, name: &'static str, minimum: u16, maximum: u16) -> PyResult<u16> {
    if parsed < i128::from(minimum) || parsed > i128::from(maximum) {
        return Err(range_error(name, minimum, maximum));
    }
    u16::try_from(parsed).map_err(|_| range_error(name, minimum, maximum))
}

fn u32_value(
    value: &Bound<'_, PyAny>,
    name: &'static str,
    minimum: u32,
    maximum: u32,
) -> PyResult<u32> {
    let parsed = integer_value(value, name)?;
    if parsed < i128::from(minimum) || parsed > i128::from(maximum) {
        return Err(range_error(name, minimum, maximum));
    }
    u32::try_from(parsed).map_err(|_| range_error(name, minimum, maximum))
}

fn integer_value(value: &Bound<'_, PyAny>, name: &'static str) -> PyResult<i128> {
    value
        .extract::<i128>()
        .map_err(|source| conversion_error(value, name, source))
}

fn conversion_error(value: &Bound<'_, PyAny>, name: &'static str, source: PyErr) -> PyErr {
    if source.is_instance_of::<PyOverflowError>(value.py()) {
        invalid_argument("OverflowError", name, format!("{name} is too large"))
    } else {
        type_error(name, format!("{name} has an invalid type"))
    }
}

fn range_error(
    name: &'static str,
    minimum: impl std::fmt::Display,
    maximum: impl std::fmt::Display,
) -> PyErr {
    value_error(
        name,
        format!("{name} must be between {minimum} and {maximum}"),
    )
}

fn type_error(name: &'static str, message: impl Into<String>) -> PyErr {
    invalid_argument("TypeError", name, message)
}

fn value_error(name: &'static str, message: impl Into<String>) -> PyErr {
    invalid_argument("ValueError", name, message)
}

fn invalid_argument(
    python_kind: &'static str,
    name: &'static str,
    message: impl Into<String>,
) -> PyErr {
    error::from_runtime(
        RuntimeError::invalid_argument(python_kind, message).with_context("name", name),
    )
}

fn protocol<T>(result: Result<T, ProtocolError>) -> PyResult<T> {
    result.map_err(protocol_error)
}

fn protocol_error(error_value: ProtocolError) -> PyErr {
    error::from_runtime(error_value.into())
}

#[cfg(test)]
mod tests {
    use super::{parse_date_text, DateParts, ProtocolError};

    #[test]
    fn textual_dates_accept_compact_and_hyphenated_forms() -> Result<(), ProtocolError> {
        let expected = DateParts::new(2026, 8, 15)?;
        assert_eq!(parse_date_text("20260815", "date")?, expected);
        assert_eq!(parse_date_text("2026-08-15", "date")?, expected);
        Ok(())
    }
}
