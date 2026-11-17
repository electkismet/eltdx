//! Closed source registries. Market numbers, categories, sorting codes and
//! quote bitmap fields are separate types even where their numbers coincide.
use crate::{Error, ErrorKind, Result};

macro_rules! registry {
    ($name:ident { $($variant:ident = $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub const fn wire(self) -> u16 { match self { $(Self::$variant => $wire),+ } }
            pub fn from_wire(value: u16) -> Result<Self> {
                match value { $($wire => Ok(Self::$variant)),+,
                    _ => Err(Error::new(ErrorKind::InvalidArgument, "unknown MAC registry value")
                        .with_context("domain", "mac").with_context("registry", stringify!($name))
                        .with_context("value", format!("{value}"))),
                }
            }
        }
    };
}
registry!(BoardType {
    Hy = 0, Hy2 = 1, Gn = 3, Fg = 4, Dq = 5, Other = 6,
    YjLevel1 = 7, YjLevel2 = 8, YjLevel3 = 9, All = 255,
});
registry!(ExBoardType { HkAll = 0, HkGn = 1, HkHy = 2, UsAll = 3, UsGn = 4, UsHy = 5 });
registry!(Category {
    Sh = 0, Sz = 2, A = 6, B = 7, Kcb = 8, Bj = 12, Cyb = 14,
    BoardAll = 10000, BoardHy = 10001, BoardHy2 = 10002, BoardGn = 10004,
    BoardFg = 10005, BoardDq = 10006, BoardOther = 10007,
    BoardYjLevel1 = 10008, BoardYjLevel2 = 10009, BoardYjLevel3 = 10010,
    Hgt = 0x2af9, Sgt = 0x2b01, Fxjs = 0x2aff, Etf = 0x2afd, Lof = 0x2b04, Zs = 0x2b2c,
});
registry!(SortType {
    Code = 0x00, Name = 0x01, PreClose = 0x02, Open = 0x03, High = 0x04, Low = 0x05,
    Price = 0x06, Bid = 0x07, Ask = 0x08, Volume = 0x09, TotalAmount = 0x0a,
    LastVolume = 0x0b, Change = 0x0c, ChangePct = 0x0e, AmplitudePct = 0x0f,
    Avg = 0x10, PeDynamic = 0x11, EntrustRatio = 0x12, InsideVolume = 0x13,
    OutsideVolume = 0x14, InOutRatio = 0x15, BidVolume = 0x17, AskVolume = 0x18,
    LockedRatio = 0x1b, LockedAmount = 0x1c, OpenAmount = 0x1d, OpenTurnoverPct = 0x1e,
    VolRatio = 0x23, TurnoverRate = 0x24, FloatShares = 0x25, FloatMarketCap = 0x26,
    TotalMarketCapAb = 0x27, UnmatchedVolume = 0x2a, StrengthPct = 0x2d,
    SpeedPct = 0x2e, Activity = 0x2f, ShortTurnoverPct = 0xcc, VolSpeedPct = 0xd0,
    MainNetAmount = 0xd4, MainNetRatio = 0xd7, AuctionLimitBuy = 0x102,
    Amount2m = 0x10c, OpenSnatchPct = 0x10a, OpenPct = 0x119, HighPct = 0x11a,
    LowPct = 0x11b, AvgChangePct = 0x11c, DrawdownPct = 0x11e, AttackPct = 0x11f,
});
registry!(SortOrder { None = 0, Desc = 1, Asc = 2 });
registry!(FilterType { New = 1, Kc = 2, St = 4, Cy = 8, HkConnect = 16, Bj = 32, Approval = 64, Registration = 128 });

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct FilterMask(u8);
impl FilterMask {
    pub const fn empty() -> Self {
        Self(0)
    }
    pub fn from_flags(flags: impl IntoIterator<Item = FilterType>) -> Self {
        Self(
            flags
                .into_iter()
                .fold(0, |bits, flag| bits | flag.wire() as u8),
        )
    }
    pub fn from_bits(value: u32) -> Result<Self> {
        u8::try_from(value).map(Self).map_err(|_| {
            Error::new(
                ErrorKind::InvalidArgument,
                "MAC filter mask exceeds its eight source bits",
            )
            .with_context("domain", "mac")
        })
    }
    pub const fn bits(self) -> u8 {
        self.0
    }
    pub const fn contains(self, flag: FilterType) -> bool {
        self.0 & flag.wire() as u8 != 0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Board conversions preserve the exact caller text alongside the checked
/// source number. Prefixes are case-sensitive, as in the original parser.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardCodePlan {
    Prefix { skip: usize, add: u32 },
    Full { subtract: u32, add: u32 },
    Plain,
}
/// Shared source decision order; conversion/overflow are separate policies.
/// Python supplies its live string methods, len and arbitrary-precision int.
#[doc(hidden)]
pub fn board_code_plan<E>(
    mut starts: impl FnMut(&str) -> std::result::Result<bool, E>,
    six_characters: impl FnOnce() -> std::result::Result<bool, E>,
) -> std::result::Result<BoardCodePlan, E> {
    if starts("US")? {
        return Ok(BoardCodePlan::Prefix {
            skip: 2,
            add: 30000,
        });
    }
    if starts("HK")? {
        return Ok(BoardCodePlan::Prefix {
            skip: 2,
            add: 20000,
        });
    }
    if six_characters()? {
        for (prefix, subtract, add) in [
            ("88", 880000, 20000),
            ("399", 399000, 30000),
            ("899", 899000, 32000),
            ("000", 0, 31000),
        ] {
            if starts(prefix)? {
                return Ok(BoardCodePlan::Full { subtract, add });
            }
        }
    }
    Ok(BoardCodePlan::Plain)
}
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoardSymbol {
    source: String,
    wire_code: u32,
}
impl BoardSymbol {
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let source = value.into();
        let text = source.trim();
        let invalid = || {
            Error::new(
                ErrorKind::InvalidArgument,
                "invalid or overflowing MAC board symbol",
            )
            .with_context("domain", "mac")
        };
        let decimal = |text: &str| -> Result<u32> {
            if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(invalid());
            }
            text.parse::<u32>().map_err(|_| invalid())
        };
        let plan = board_code_plan::<std::convert::Infallible>(
            |prefix| Ok(text.starts_with(prefix)),
            || Ok(text.len() == 6),
        )
        .unwrap_or_else(|never| match never {});
        let wire_code = match plan {
            BoardCodePlan::Prefix { skip, add } => decimal(&text[skip..])?
                .checked_add(add)
                .ok_or_else(invalid)?,
            BoardCodePlan::Full { subtract, add } => decimal(text)?
                .checked_sub(subtract)
                .and_then(|value| value.checked_add(add))
                .ok_or_else(invalid)?,
            BoardCodePlan::Plain => decimal(text)?,
        };
        Ok(Self { source, wire_code })
    }
    pub fn as_str(&self) -> &str {
        &self.source
    }
    pub const fn wire_code(&self) -> u32 {
        self.wire_code
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BoardRankKey {
    #[default]
    ChangePct,
    Amount,
    Volume,
    MainNetAmount,
    MainNet3d,
    MainNet5d,
}

/// Source-order is the only sorting contract of the ordinary MAC-EX listing.
/// A sorted collector is a separate composition; this enum cannot silently
/// accept an ignored legacy SortType argument.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum QuoteListOrder {
    #[default]
    SourceOrder,
}
