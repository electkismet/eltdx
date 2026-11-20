use eltdx::types::{BatchItemState, BatchOutcome, FinanceField, FinanceRecord, FinanceValue};
use serde_json::{json, Map, Value};

use super::hq::date_value;
use super::quote::{hex, identity, raw, record};
use super::{Result, RpcError};

pub(crate) const KEYS: [&str; 31] = [
    "liu_tong_gu_ben_raw_float",
    "zong_gu_ben_raw_float",
    "guo_jia_gu_raw_float",
    "fa_qi_ren_fa_ren_gu_raw_float",
    "fa_ren_gu_raw_float",
    "b_gu_raw_float",
    "h_gu_raw_float",
    "eps_raw",
    "zong_zi_chan_raw_float",
    "liu_dong_zi_chan_raw_float",
    "gu_ding_zi_chan_raw_float",
    "wu_xing_zi_chan_raw_float",
    "gu_dong_ren_shu_raw_float",
    "liu_dong_fu_zhai_raw_float",
    "chang_qi_fu_zhai_raw_float",
    "zi_ben_gong_ji_jin_raw_float",
    "jing_zi_chan_raw_float",
    "zhu_ying_shou_ru_raw_float",
    "zhu_ying_li_run_raw_float",
    "ying_shou_zhang_kuan_raw_float",
    "ying_ye_li_run_raw_float",
    "tou_zi_shou_yu_raw_float",
    "jing_ying_xian_jin_liu_raw_float",
    "zong_xian_jin_liu_raw_float",
    "cun_huo_raw_float",
    "li_run_zong_he_raw_float",
    "shui_hou_li_run_raw_float",
    "jing_li_run_raw_float",
    "wei_fen_li_run_raw_float",
    "mei_gu_jing_zi_chan_raw_float",
    "bao_liu_2_raw_float",
];

fn wire(value: FinanceValue) -> Option<f32> {
    match value {
        FinanceValue::Shares(value) => Some(value.source_value().as_f64() as f32),
        FinanceValue::Money(value) => Some(value.source_value().as_f64() as f32),
        FinanceValue::PerShare(value) => Some(value.as_f64() as f32),
        FinanceValue::Count(value) => Some(value as f32),
        FinanceValue::Unclassified { value, .. } => Some(value),
        _ => None,
    }
}

fn row(record_value: &FinanceRecord) -> Value {
    let values = FinanceField::ALL.map(|field| record_value.value(field).and_then(wire));
    let mut info = Vec::with_capacity(136);
    info.extend(values[0].unwrap_or_default().to_le_bytes());
    info.extend(record_value.province().to_le_bytes());
    info.extend(record_value.industry().to_le_bytes());
    info.extend(record_value.updated_raw().to_le_bytes());
    info.extend(record_value.ipo_raw().to_le_bytes());
    for value in &values[1..] {
        info.extend(value.unwrap_or_default().to_le_bytes());
    }
    let mut result = identity(
        record_value.id(),
        json!({
            "finance_info_raw": hex(&info), "province_raw": record_value.province(),
            "industry_raw": record_value.industry(), "updated_date_raw": record_value.updated_raw(),
            "updated_date": date_value(record_value.updated()), "ipo_date_raw": record_value.ipo_raw(),
            "ipo_date": date_value(record_value.ipo()), "record_hex": record(record_value.meta(), 0),
        }),
    );
    for (key, value) in KEYS.into_iter().zip(values) {
        result[key] = json!(value.map(f64::from));
    }
    result
}

pub(super) fn batch(
    batch: &BatchOutcome<FinanceRecord>,
    fields: Option<&[String]>,
) -> Result<Value> {
    let mut records = Vec::new();
    for item in batch.items() {
        let BatchItemState::Present(record_value) = item.state() else {
            continue;
        };
        let row = row(record_value);
        let Some(fields) = fields else {
            records.push(row);
            continue;
        };
        let mut projection = Map::new();
        let full_code = json!(format!("{}", item.security()));
        projection.insert("full_code".to_owned(), full_code.clone());
        for field in fields {
            let value = match field.as_str() {
                "full_code" => full_code.clone(),
                "circulating_shares" | "流通股本" => scaled(&row, KEYS[0], 10_000.0),
                "total_shares" | "总股本" => scaled(&row, KEYS[1], 10_000.0),
                "total_assets_yuan" | "总资产" => scaled(&row, KEYS[8], 1000.0),
                "net_profit_yuan" | "净利润" => scaled(&row, KEYS[27], 1000.0),
                name => row
                    .get(name)
                    .cloned()
                    .ok_or_else(|| RpcError::value(format!("unknown finance field: {name}")))?,
            };
            projection.insert(field.clone(), value);
        }
        records.push(Value::Object(projection));
    }
    Ok(if fields.is_some() {
        json!(records)
    } else {
        json!({"records": records, "raw_payload": raw(batch.meta())})
    })
}

fn scaled(row: &Value, field: &str, factor: f64) -> Value {
    json!(row[field].as_f64().map(|value| value * factor))
}
