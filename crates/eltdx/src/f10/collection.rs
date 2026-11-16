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

/// Generic first-match traversal, with success distinct from an absent value.
/// A Python match containing None must stop just like any other matched row.
pub fn first_match<T, V, E>(rows: impl IntoIterator<Item = Result<T, E>>,
    mut select: impl FnMut(T) -> Result<Option<V>, E>) -> Result<Option<V>, E>
{
    for row in rows { if let Some(value) = select(row?)? { return Ok(Some(value)); } }
    Ok(None)
}

pub fn resolve_dates<T: Clone, E>(start: Option<T>, end: Option<T>,
    today: impl FnOnce() -> Result<T, E>) -> Result<(T, T), E>
{
    match (start, end) {
        (Some(start), Some(end)) => Ok((start, end)),
        (Some(day), None) | (None, Some(day)) => Ok((day.clone(), day)),
        (None, None) => { let day = today()?; Ok((day.clone(), day)) }
    }
}

pub const LADDER_ROW_FIELDS: &[(&str, &str, Option<&str>)] = &[
    ("trading_date", "rq", None), ("trading_date_value", "rqex", None),
    ("code", "ZQDM", Some("_as_text")), ("market_id", "SC", Some("_as_int")),
    ("limit_reason", "ztyy", None), ("seal_amount", "fde", Some("_as_number")),
    ("name", "ZQJC", None), ("limit_reason_extra", "ztyy2", None),
    ("limit_time", "ztsj", None), ("broken_count", "kbcs", Some("_as_int")),
    ("industry", "sshy", None), ("limit_type", "ztlb", None),
    ("success_rate", "cgl", None), ("highest_board_level", "zglb", Some("_as_int")),
];

pub(crate) fn ladder_column(attribute: &str) -> &'static str {
    LADDER_ROW_FIELDS.iter().find(|&&(name, _, _)| name == attribute)
        .expect("built-in ladder field").1
}

pub fn keep_ladder_level<E>(up: impl FnOnce() -> Result<bool, E>,
    present: impl FnOnce() -> Result<bool, E>, positive: impl FnOnce() -> Result<bool, E>) -> Result<bool, E>
{
    Ok(up()? && present()? && positive()?)
}

pub const ETF_PREFIXES: &[&str] = &["51", "56", "58", "15", "159", "16"];
pub const BOARD_RECORD_ATTRIBUTES: &[&str] = &["code", "dividend_plan", "announce_date", "record_date", "ex_date", "report_period"];
pub const OVERVIEW_FIELDS: &[(&str, &str)] = &[
    ("advancing_count", "N002"), ("declining_count", "N003"), ("flat_count", "N004"),
    ("limit_up_count", "N005"), ("limit_down_count", "N006"),
    ("today_amount", "N007"), ("previous_amount", "N008"),
];
pub const OVERVIEW_TOPICS: &[(&str, &str)] = &[("N009", "N010"), ("N011", "N012"), ("N013", "N014")];

pub fn visit_classified<T, K, E>(rows: impl IntoIterator<Item = Result<T, E>>,
    mut classify: impl FnMut(T) -> Result<Option<K>, E>,
    mut add: impl FnMut(K) -> Result<(), E>) -> Result<(), E>
{
    for row in rows { if let Some(key) = classify(row?)? { add(key)?; } }
    Ok(())
}

pub fn visit_promotion<L, V, E>(levels: impl IntoIterator<Item = Result<L, E>>,
    mut denominator: impl FnMut(&L) -> Result<Option<V>, E>,
    mut add: impl FnMut(L, V) -> Result<(), E>) -> Result<(), E>
{
    for level in levels {
        let level = level?;
        if let Some(value) = denominator(&level)? { add(level, value)?; }
    }
    Ok(())
}

#[cfg(test)]
mod value_tests {
    use super::*;

    #[test]
    fn first_match_retains_nested_none_and_dates_only_read_clock_when_absent() {
        let values = [None, Some(None), Some(Some(2))];
        assert_eq!(first_match(values.into_iter().map(Ok::<_, ()>), Ok).unwrap(), Some(None));
        assert_eq!(resolve_dates(Some(2), None, || Err::<i32, _>(())).unwrap(), (2, 2));
        assert_eq!(resolve_dates(None, None, || Ok::<_, ()>(3)).unwrap(), (3, 3));
        assert!(!keep_ladder_level(|| Ok::<_, ()>(false), || panic!("short circuit"), || panic!("short circuit")).unwrap());
    }

    #[test]
    fn classification_and_promotion_preserve_source_order() {
        let mut selected = Vec::new();
        visit_classified([2, 1, 3].into_iter().map(Ok::<_, ()>),
            |value| Ok((value != 1).then_some(value)),
            |value| { selected.push(value); Ok(()) }).unwrap();
        assert_eq!(selected, [2, 3]);
        let mut rates = Vec::new();
        visit_promotion([3, 2, 1].into_iter().map(Ok::<_, ()>),
            |&level| Ok((level > 1).then_some(level - 1)),
            |level, lower| { rates.push((level, lower)); Ok(()) }).unwrap();
        assert_eq!(rates, [(3, 2), (2, 1)]);
    }
}
