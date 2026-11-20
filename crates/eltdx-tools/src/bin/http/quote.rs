use eltdx::types::{QuoteLevel, QuoteUpdates, ResultMeta};
use eltdx::{HqMarket, QuoteUpdate, SecurityId, Snapshot};
use serde_json::{json, Value};

pub fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut text = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

pub(super) fn raw(meta: &ResultMeta) -> String {
    meta.raw()
        .and_then(|raw| raw.payloads().first())
        .map_or_else(String::new, |payload| hex(payload))
}

pub(super) fn record(meta: &ResultMeta, index: usize) -> String {
    meta.raw()
        .and_then(|raw| {
            let &(page, start, end) = raw.row_ranges().get(index)?;
            let bytes = raw.payloads().get(page)?.get(start..end)?;
            Some(if raw.row_requires_xor93(index) == Some(true) {
                hex(&bytes.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>())
            } else {
                hex(bytes)
            })
        })
        .unwrap_or_default()
}

pub(super) fn market_id(market: HqMarket) -> u8 {
    match market {
        HqMarket::Shenzhen => 0,
        HqMarket::Shanghai => 1,
        HqMarket::Beijing => 2,
        _ => u8::MAX,
    }
}

pub(super) fn identity(id: &SecurityId, mut value: Value) -> Value {
    value["exchange"] = json!(id.market().code());
    value["market_id"] = json!(market_id(id.market()));
    value["code"] = json!(id.number());
    value
}

fn order_book(levels: &[QuoteLevel], last: f64) -> Vec<Value> {
    levels.iter().map(|level| {
        let price = level.price();
        let scale = price.value().precision().unwrap_or(2);
        json!({"price": price.as_f64(), "volume": level.volume().source_value().as_f64() as i64,
            "price_delta_raw": ((price.as_f64() - last) * 10_f64.powi(scale as i32)).round() as i64})
    }).collect()
}

fn common(row: &Snapshot) -> Value {
    identity(
        row.id(),
        json!({
            "last_price": row.last().as_f64(), "open_price": row.open().as_f64(), "high_price": row.high().as_f64(), "low_price": row.low().as_f64(),
            "total_hand": row.total().source_value().as_f64() as i64,
            "current_hand": row.current().source_value().as_f64() as i64,
            "inside_dish": row.inside().source_value().as_f64() as i64,
            "outer_disc": row.outside().source_value().as_f64() as i64,
            "amount": row.amount().value().as_f64(), "amount_raw": row.amount_wire_word(),
            "unknown_after_outer_raw": row.unknown_after_outer_raw(),
            "open_amount_raw": row.open_amount_raw(), "open_amount_yuan": row.open_amount().value().as_f64(),
            "buy_levels": order_book(row.bids(), row.last().as_f64()),
            "sell_levels": order_book(row.asks(), row.last().as_f64()),
        }),
    )
}

pub(super) fn snapshot(row: &Snapshot) -> Value {
    let mut value = common(row);
    value["active1"] = json!(row.active());
    value["pre_close_price"] = json!(row.pre_close().as_f64());
    value["time_raw"] = json!(row.server_time_raw());
    value["unknown_after_time_raw"] = json!(row.unknown_after_time_raw());
    value["tail_raw"] = json!("");
    value
}

pub fn update(row: &QuoteUpdate) -> Value {
    let snapshot = row.snapshot();
    let mut value = common(snapshot);
    value["active"] = json!(row.active());
    value["update_time_raw"] = json!(row.cursor());
    value["last_close_price"] = json!(snapshot.pre_close().as_f64());
    value["status_or_reserved_raw"] = json!(row.status_raw());
    value["tail_raw"] = json!("");
    value["record_hex"] = json!(record(snapshot.meta(), 0));
    value
}

pub fn refresh_page(page: &QuoteUpdates) -> Value {
    let payload = raw(page.meta());
    let decoded = page
        .meta()
        .raw()
        .and_then(|raw| raw.payloads().first())
        .map(|bytes| hex(&bytes.iter().map(|byte| byte ^ 0x93).collect::<Vec<_>>()))
        .unwrap_or_default();
    json!({
        "requested_codes": page.requested().iter().map(|id| format!("{id}")).collect::<Vec<_>>(),
        "records": page.updates().iter().map(update).collect::<Vec<_>>(),
        "decoded_payload": decoded,
        "raw_payload": payload,
    })
}
