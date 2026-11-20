use serde_json::{json, Value};

pub(super) struct Tool {
    pub name: &'static str,
    pub hq: bool,
    description: &'static str,
    fields: &'static str,
}

macro_rules! tool {
    ($name:ident, $hq:literal, $description:literal, $fields:literal) => {
        Tool {
            name: stringify!($name),
            hq: $hq,
            description: $description,
            fields: $fields,
        }
    };
}

static TOOLS: &[Tool] = &[
    tool!(quote, true, "Query quote snapshots.", "codes:c!"),
    tool!(quote_depth, true, "Query five-level quote depth.", "codes:c!"),
    tool!(kline, true, "Query one bounded K-line page.", "code:c! period:s count:i start:i adjust:s anchor_date:d include_raw:b batch_size:i"),
    tool!(capital_changes, true, "Query capital-change and corporate-action records.", "codes:c! include_raw:b batch_size:i"),
    tool!(adjustment_factors, true, "Calculate adjustment coefficients from capital-change records.", "codes:c! anchor_date:d start_date:d batch_size:i"),
    tool!(money_flow, true, "Query recent daily money-flow records.", "codes:c! include_raw:b batch_size:i"),
    tool!(daily_price_limits, true, "Calculate daily price limits for a trading date.", "codes:c trade_date:d!"),
    tool!(minute, true, "Query current or historical minute data.", "code:c! trading_date:d include_raw:b batch_size:i"),
    tool!(trades, true, "Query one page of current or historical trades.", "code:c! trading_date:d start:i count:i include_raw:b batch_size:i"),
    tool!(call_auction, true, "Query current or historical call auction data.", "code:s! trading_date:d include_raw:b"),
    tool!(opening_match_today, true, "Query today's opening match.", "code:s!"),
    tool!(opening_match_history, true, "Query the opening match for a trading date.", "code:s! trading_date:d!"),
    tool!(auction_data, true, "Combine auction series, opening match and quote.", "code:s! trading_date:d include_series:b include_snapshot:b include_quote:b pre_close_price:n"),
    tool!(stock_profile, true, "Combine stock quote, security and finance data.", "codes:c! include_security:b include_finance:b"),
    tool!(shortline_indicators, true, "Calculate shortline trading indicators.", "codes:c! stats_path:s refresh_stats:b"),
    tool!(stock_topics, false, "Query a stock's topics.", "code:s!"),
    tool!(topic_stocks, false, "Query stocks in a selected topic.", "seed_code:s! topic_id:d topic_name:s sort_by:s section:s"),
    tool!(limit_up_down_list, false, "Query the F10 limit-up/down list.", "start_date:d end_date:d include_summary:b"),
    tool!(limit_board_ladder, false, "Query the F10 consecutive limit-up ladder.", "start_date:d end_date:d include_summary:b"),
    tool!(company_profile, false, "Query a company profile section.", "code:s! section:s"),
    tool!(f10_digest, false, "Query an F10 company digest.", "code:s! section:s page:i page_size:i"),
    tool!(us_stock_f10, false, "Query US stock F10 data.", "code:s! section:s"),
    tool!(hk_stock_f10, false, "Query Hong Kong stock F10 data.", "code:s! section:s"),
    tool!(bond_f10, false, "Query bond F10 data by kind and section.", "code:s! kind:s section:s"),
    tool!(industry_f10, false, "Query F10 industry data.", "industry_code:s! section:s"),
    tool!(stock_analysis, false, "Query F10 stock analysis.", "code:s! section:s"),
    tool!(macro_indicators, false, "List available macroeconomic indicators.", ""),
    tool!(macro_data, false, "Query latest or historical macroeconomic data.", "codes:s! period:s history:b"),
    tool!(hot_topics, false, "Query hot topics from F10.", "code:s! section:s"),
    tool!(finance_report, false, "Query the selected F10 financial report.", "code:s! report_type:s"),
    tool!(company_news, false, "Query company news and research.", "code:s! section:s keyword:s rating:d page:i page_size:i"),
    tool!(market_news, false, "Query the market news feed.", "tab:i page:i page_size:i"),
    tool!(channel_news, false, "Query news from a numbered channel.", "channel:i! page:i page_size:i"),
    tool!(news_channels, false, "List all supported news channel IDs and names.", ""),
    tool!(docs_index, false, "Locate the bundled eltdx documentation.", ""),
];

impl Tool {
    fn fields(&self) -> impl Iterator<Item = (&str, &str, bool)> {
        let extras = match self.name {
            "docs_index" | "news_channels" => "",
            _ if self.hq => "timeout:n host:s",
            _ => "timeout:n",
        };
        self.fields
            .split_ascii_whitespace()
            .chain(extras.split_ascii_whitespace())
            .filter_map(|field| {
                let (name, kind) = field.split_once(':')?;
                Some((name, kind.trim_end_matches('!'), kind.ends_with('!')))
            })
    }
}

pub(super) fn lookup(name: &str) -> Option<&'static Tool> {
    TOOLS.iter().find(|tool| tool.name == name)
}

pub(super) fn schemas() -> Vec<Value> {
    TOOLS.iter().map(|tool| {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for (name, kind, mandatory) in tool.fields() {
            let schema = match kind {
                "c" => json!({"anyOf": [{"type": "string"}, {"type": "array", "items": {"type": "string"}, "minItems": 1}]}),
                "d" => json!({"type": ["string", "integer"]}),
                "i" => json!({"type": "integer"}),
                "n" => json!({"type": "number"}),
                "b" => json!({"type": "boolean"}),
                _ => json!({"type": "string"}),
            };
            if mandatory {
                required.push(name);
                properties.insert(name.to_owned(), schema);
            } else {
                properties.insert(name.to_owned(), json!({"anyOf": [schema, {"type": "null"}]}));
            }
        }
        json!({
            "name": format!("eltdx_{}", tool.name),
            "description": tool.description,
            "inputSchema": {"type": "object", "properties": properties, "required": required, "additionalProperties": false}
        })
    }).collect()
}

pub(super) fn validate(tool: &Tool, args: &Value) -> Result<(), Box<dyn std::error::Error>> {
    let object = args.as_object().ok_or("arguments must be an object")?;
    for name in object.keys() {
        if !tool.fields().any(|(field, _, _)| field == name) {
            return Err(format!("Unknown argument: {name}").into());
        }
    }
    for (name, kind, mandatory) in tool.fields() {
        let value = &args[name];
        let valid = match kind {
            "c" => {
                value.is_string()
                    || value.as_array().is_some_and(|items| {
                        !items.is_empty() && items.iter().all(Value::is_string)
                    })
            }
            "d" => value.is_string() || value.is_i64() || value.is_u64(),
            "i" => value.is_i64() || value.is_u64(),
            "n" => value.is_number(),
            "b" => value.is_boolean(),
            _ => value.is_string(),
        };
        if !(valid || !mandatory && value.is_null()) {
            return Err(format!("Invalid argument: {name}").into());
        }
    }
    Ok(())
}
