//! Shared pagination loop and typed row field definitions.

pub fn collect_pages<P, T, E>(mut page: P,
    mut visit: impl FnMut(&P) -> Result<Option<T>, E>,
    mut advance: impl FnMut(P) -> Result<P, E>) -> Result<T, E>
{
    loop {
        if let Some(result) = visit(&page)? { return Ok(result); }
        page = advance(page)?;
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FieldConversion { Raw, Text, Integer, Number, MarketFlag }

#[derive(Clone, Copy, Debug)]
pub struct RowField {
    pub attribute: &'static str,
    pub column: &'static str,
    pub conversion: FieldConversion,
}

macro_rules! fields {
    ($($attribute:ident : $column:literal => $conversion:ident),* $(,)?) => {
        &[$(RowField { attribute: stringify!($attribute), column: $column, conversion: FieldConversion::$conversion }),*]
    };
}

pub fn row_fields(method: &str) -> &'static [RowField] {
    match method {
        "index_members" => fields![code: "N001" => Text, name: "N002" => Text,
            market_id: "N004" => MarketFlag, join_date: "N003" => Raw],
        "index_tracking_products" => fields![code: "N001" => Text, name: "N002" => Text,
            market_id: "sc" => Integer, scale_yuan: "N003" => Number, change_pct: "N005" => Number,
            amount_yuan: "N006" => Number, inception_date: "N008" => Raw, manager: "N009" => Text,
            company: "N010" => Text, nav: "N011" => Number, fee_rate_pct: "N012" => Number],
        "board_members" => fields![code: "N001" => Text, name: "N002" => Text, market_id: "sc" => MarketFlag,
            dividend_plan: "N003" => Text, progress: "N004" => Text, announce_date: "N005" => Raw,
            record_date: "N006" => Raw, ex_date: "N007" => Raw, report_period: "N008" => Raw],
        "ipo_calendar" => fields![apply_date: "N001" => Raw, code: "N002" => Text, name: "N003" => Text,
            market_id: "sc" => Integer, issue_pe: "N004" => Number, extra_pct: "N005" => Number,
            issue_price: "N006" => Number, apply_limit: "N007" => Number, raise_amount: "N008" => Number],
        _ => &[],
    }
}

pub(crate) fn column(method: &str, attribute: &str) -> &'static str {
    row_fields(method).iter().find(|field| field.attribute == attribute)
        .expect("built-in F10 field").column
}

pub fn collection_route(method: &str) -> Option<(&'static str, &'static str, &'static str)> {
    match method {
        "index_members" => Some(("index_members", "IndexMember", "IndexMemberTable")),
        "index_tracking_products" => Some(("index_etfs", "IndexTrackingProduct", "IndexTrackingTable")),
        "board_members" => Some(("board_members", "BoardMember", "BoardMemberTable")),
        "ipo_calendar" => Some(("ipo_calendar", "IpoEntry", "IpoCalendar")),
        _ => None,
    }
}

pub fn ladder_parameters<T>(selector: T, start: T, end: T) -> [T; 3] { [selector, start, end] }
