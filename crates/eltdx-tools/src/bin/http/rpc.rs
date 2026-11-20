use axum::http::StatusCode;
use eltdx::{ErrorKind, HqMarket, SecurityId, TdxClient, TradingDate};
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

#[path = "f10.rs"]
mod f10;
#[path = "finance.rs"]
mod finance;
#[path = "helpers.rs"]
mod helpers;
#[path = "hq.rs"]
mod hq;
#[path = "quote.rs"]
pub mod quote;

pub use helpers::CollectionCache;

pub type Result<T> = std::result::Result<T, RpcError>;

#[derive(Debug)]
pub struct RpcError {
    pub status: StatusCode,
    kind: &'static str,
    message: String,
}

impl RpcError {
    pub(super) fn new(status: StatusCode, kind: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            kind,
            message: message.into(),
        }
    }

    pub fn request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "GatewayRequestError", message)
    }

    pub fn value(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "ValueError", message)
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, "RuntimeError", message)
    }

    fn unknown(method: &str) -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            "GatewayMethodError",
            format!("unknown method: {method}"),
        )
    }

    pub fn detail(&self) -> Value {
        json!({"type": self.kind, "message": self.message})
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn envelope(&self, id: Value) -> Value {
        json!({"id": id, "ok": false, "error": self.detail()})
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for RpcError {}

impl From<eltdx::Error> for RpcError {
    fn from(error: eltdx::Error) -> Self {
        use ErrorKind::*;
        let (status, kind) = match error.kind() {
            InvalidArgument => (StatusCode::BAD_REQUEST, "ValueError"),
            Timeout | CloseTimeout => (StatusCode::BAD_GATEWAY, "TimeoutError"),
            Connection | PoolBusy | PinLost | Closed => (StatusCode::BAD_GATEWAY, "TransportError"),
            Protocol | ResourceFormat => (StatusCode::BAD_GATEWAY, "ProtocolError"),
            MissingData => (StatusCode::BAD_GATEWAY, "MissingCodesError"),
            PushGap => (StatusCode::BAD_GATEWAY, "PushOverflowError"),
            Internal => (StatusCode::INTERNAL_SERVER_ERROR, "RuntimeError"),
            Io => (StatusCode::BAD_GATEWAY, "OSError"),
            _ => (StatusCode::BAD_GATEWAY, "EltdxError"),
        };
        Self::new(status, kind, error.message())
    }
}

pub fn parse_request(mut body: Value) -> Result<(String, Value)> {
    let body = body
        .as_object_mut()
        .ok_or_else(|| RpcError::request("RPC body must be a JSON object"))?;
    let method = body
        .get("method")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|method| !method.is_empty())
        .ok_or_else(|| RpcError::request("method must be a non-empty string"))?
        .to_owned();
    let params = body.remove("params").unwrap_or_default();
    let params = match params {
        Value::Null => Value::Object(Map::new()),
        Value::Array(_) | Value::Object(_) => params,
        _ => return Err(RpcError::request("params must be an object or an array")),
    };
    Ok((method, params))
}

pub fn methods() -> Vec<String> {
    let mut methods = hq::METHODS
        .iter()
        .chain(helpers::METHODS)
        .map(|method| (*method).to_owned())
        .chain(f10::methods().map(|name| format!("f10.{name}")))
        .collect::<Vec<_>>();
    methods.push("quotes.subscribe".to_owned());
    methods.sort_unstable();
    methods
}

pub fn call(client: &TdxClient, method: &str, params: Value) -> Result<Value> {
    call_with_cache(client, &CollectionCache::default(), method, params)
}

pub fn call_with_cache(
    client: &TdxClient,
    collections: &CollectionCache,
    method: &str,
    params: Value,
) -> Result<Value> {
    if let Some(name) = method.strip_prefix("f10.") {
        return f10::call(client, name, params);
    }
    if matches!(method, "quotes.subscribe" | "quotes.subscribe_hq") {
        let _ = Arguments::bind(
            params,
            &[
                "codes",
                "callback",
                ";",
                "timeout",
                "filter_codes",
                "on_error",
                "reactivate_interval",
            ],
        )?;
        return Err(RpcError::request(format!(
            "{method} requires a Python callback; use WebSocket quotes.subscribe for JSON subscriptions"
        )));
    }
    if hq::METHODS.contains(&method) {
        let result = hq::call(client, method, params)?;
        if method == "clear_cache" {
            collections.clear()?;
        }
        return Ok(result);
    }
    if helpers::METHODS.contains(&method) {
        return helpers::call(client, collections, method, params);
    }
    Err(RpcError::unknown(method))
}

pub(super) struct Arguments(Map<String, Value>);

impl Arguments {
    pub(super) fn bind(params: Value, signature: &[&str]) -> Result<Self> {
        let positional = signature
            .iter()
            .position(|name| *name == ";")
            .unwrap_or(signature.len());
        let values = match params {
            Value::Array(values) => {
                if values.len() > positional {
                    return Err(RpcError::request(format!(
                        "expected at most {positional} positional arguments"
                    )));
                }
                signature
                    .iter()
                    .zip(values)
                    .map(|(name, value)| ((*name).to_owned(), value))
                    .collect()
            }
            Value::Object(values) => {
                if let Some(name) = values
                    .keys()
                    .find(|name| *name == ";" || !signature.contains(&name.as_str()))
                {
                    return Err(RpcError::request(format!(
                        "unexpected keyword argument '{name}'"
                    )));
                }
                values
            }
            _ => return Err(RpcError::request("params must be an object or an array")),
        };
        Ok(Self(values))
    }

    pub(super) fn value(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }

    pub(super) fn required(&self, name: &str) -> Result<&Value> {
        self.value(name)
            .ok_or_else(|| RpcError::request(format!("missing required argument '{name}'")))
    }

    pub(super) fn get<T: DeserializeOwned>(&self, name: &str) -> Result<T> {
        decode(name, self.required(name)?)
    }

    pub(super) fn optional<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        self.value(name)
            .filter(|value| !value.is_null())
            .map(|value| decode(name, value))
            .transpose()
    }

    pub(super) fn default<T: DeserializeOwned>(&self, name: &str, default: T) -> Result<T> {
        self.value(name)
            .map(|value| decode(name, value))
            .unwrap_or(Ok(default))
    }
}

fn decode<T: DeserializeOwned>(name: &str, value: &Value) -> Result<T> {
    serde_json::from_value(value.clone())
        .map_err(|error| RpcError::request(format!("invalid '{name}': {error}")))
}

pub(super) fn security(value: &str) -> Result<SecurityId> {
    let code = value.trim();
    if code.len() == 6 && code.bytes().all(|byte| byte.is_ascii_digit()) {
        if matches!(
            &code[..3],
            "010" | "019" | "120" | "122" | "124" | "126" | "130"
        ) {
            return Ok(SecurityId::new(HqMarket::Shanghai, code)?);
        }
        if code.starts_with("127") {
            return Ok(SecurityId::new(HqMarket::Shenzhen, code)?);
        }
    }
    Ok(code.parse()?)
}

pub fn securities(value: &Value) -> Result<Vec<SecurityId>> {
    if let Some(value) = value.as_str() {
        return Ok(vec![security(value)?]);
    }
    let values = value
        .as_array()
        .ok_or_else(|| RpcError::request("expected a security code or an array of codes"))?;
    values
        .iter()
        .map(|value| {
            let code = value.as_str().ok_or_else(|| {
                RpcError::request("each security code must be a non-empty string")
            })?;
            security(code)
        })
        .collect()
}

pub(super) fn market(value: &Value) -> Result<HqMarket> {
    let name = match value {
        Value::Number(number) => format!("{number}"),
        Value::String(text) => text.trim().to_lowercase(),
        _ => return Err(RpcError::value("invalid market")),
    };
    match name.as_str() {
        "sz" | "0" | "sza" | "深市" => Ok(HqMarket::Shenzhen),
        "sh" | "1" | "sha" | "沪市" => Ok(HqMarket::Shanghai),
        "bj" | "2" | "bse" | "北交所" => Ok(HqMarket::Beijing),
        _ => Err(RpcError::new(
            StatusCode::BAD_GATEWAY,
            "ProtocolError",
            format!("invalid market: {value}"),
        )),
    }
}

pub(super) fn date(value: &Value) -> Result<TradingDate> {
    let text = match value {
        Value::Number(number) if number.is_u64() => format!("{number}"),
        Value::String(text) => text.trim().replace('-', ""),
        _ => return Err(RpcError::value("invalid trading date")),
    };
    if text.len() != 8 || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(RpcError::value(format!("invalid trading date: {value}")));
    }
    let digits = |part: &[u8]| {
        part.iter()
            .fold(0_u16, |acc, digit| acc * 10 + u16::from(digit - b'0'))
    };
    let text = text.as_bytes();
    Ok(TradingDate::new(
        digits(&text[..4]),
        digits(&text[4..6]) as u8,
        digits(&text[6..]) as u8,
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rpc_envelope_trims_method_and_accepts_null_params() -> Result<()> {
        let (method, params) = parse_request(json!({"method": " ping ", "params": null}))?;
        assert_eq!(method, "ping");
        assert_eq!(params, json!({}));
        Ok(())
    }

    #[test]
    fn positional_input_cannot_bind_keyword_only_parameters() {
        assert!(Arguments::bind(json!(["sh", 1]), &["market", ";", "start"]).is_err());
        assert!(Arguments::bind(
            json!({"market": "sh", "start": 1}),
            &["market", ";", "start"]
        )
        .is_ok());
    }
}
