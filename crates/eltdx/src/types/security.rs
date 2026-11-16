use std::fmt;
use std::str::FromStr;

use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum HqMarket {
    Shenzhen,
    Shanghai,
    Beijing,
}

impl HqMarket {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Shenzhen => "sz",
            Self::Shanghai => "sh",
            Self::Beijing => "bj",
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SecurityId {
    market: HqMarket,
    number: String,
}

impl SecurityId {
    pub(crate) fn canonical_heap(&mut self) { super::memory::canonical_string(&mut self.number); }
    pub fn new(market: HqMarket, number: &str) -> Result<Self> {
        if number.len() != 6 || !number.is_ascii() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid_code(number));
        }
        Ok(Self {
            market,
            number: number.to_owned(),
        })
    }

    pub const fn market(&self) -> HqMarket {
        self.market
    }

    pub fn number(&self) -> &str {
        &self.number
    }

    pub(crate) fn charge_heap(&self, charge: &mut super::memory::MemoryCharge) -> Result<()> {
        charge.string(&self.number)
    }
}

impl FromStr for SecurityId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self> {
        let (explicit_market, number) = if value.len() == 8 && value.is_ascii() {
            let (prefix, number) = value.split_at(2);
            let market = match prefix.to_ascii_lowercase().as_str() {
                "sz" => Some(HqMarket::Shenzhen),
                "sh" => Some(HqMarket::Shanghai),
                "bj" => Some(HqMarket::Beijing),
                _ => return Err(invalid_code(value)),
            };
            (market, number)
        } else {
            (None, value)
        };
        if number.len() != 6 || !number.is_ascii() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid_code(value));
        }
        let market = match explicit_market {
            Some(market) => market,
            None => infer_market(number).ok_or_else(|| invalid_code(value))?,
        };
        Self::new(market, number)
    }
}

impl fmt::Display for SecurityId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}{}", self.market.code(), self.number)
    }
}

fn infer_market(number: &str) -> Option<HqMarket> {
    if ["019", "120", "122", "126", "127"].iter().any(|prefix| number.starts_with(prefix)) {
        return None;
    }
    if ["110", "113", "118"].iter().any(|prefix| number.starts_with(prefix)) {
        return Some(HqMarket::Shanghai);
    }
    if ["123", "127", "128"].iter().any(|prefix| number.starts_with(prefix)) {
        return Some(HqMarket::Shenzhen);
    }
    match number.as_bytes().first().copied()? {
        b'0' | b'1' | b'2' | b'3' => Some(HqMarket::Shenzhen),
        b'4' | b'8' => Some(HqMarket::Beijing),
        b'5' | b'6' => Some(HqMarket::Shanghai),
        b'9' if number.starts_with("92") => Some(HqMarket::Beijing),
        b'9' => Some(HqMarket::Shanghai),
        _ => None,
    }
}

fn invalid_code(value: &str) -> Error {
    Error::new(ErrorKind::InvalidArgument, "invalid HQ security identifier")
        .with_context("value", value)
}
