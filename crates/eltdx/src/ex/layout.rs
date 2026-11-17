//! Source request headers shared by the independent codecs and legacy bridge.
//! No Python or transport dependency is present in this registry.
pub const MARKETS: [u8;12] = [1,2,0x48,0x69,0,1,2,0,2,0,0xf4,0x23];
pub const COUNT: [u8;12] = [1,3,0x48,0x66,0,1,2,0,2,0,0xf0,0x23];
pub const INFO: [u8;12] = [1,4,0x48,0x67,0,1,8,0,8,0,0xf5,0x23];
pub const QUOTE: [u8;12] = [1,1,8,2,2,1,12,0,12,0,0xfa,0x23];
pub const QUOTES: [u8;12] = [1,0xc1,6,0x0b,0,2,11,0,11,0,0,0x24];
pub const BARS: [u8;12] = [1,1,8,0x6a,1,1,22,0,22,0,0xff,0x23];
pub const RANGE: [u8;12] = [1,1,0x38,0x92,0,1,22,0,22,0,0x0d,0x24];
pub const MINUTE: [u8;12] = [1,7,8,0,1,1,12,0,12,0,0x0b,0x24];
pub const HISTORY_MINUTE: [u8;12] = [1,1,0x30,0,1,1,16,0,16,0,0x0c,0x24];
pub const TRANSACTIONS: [u8;12] = [1,1,8,0,3,1,18,0,18,0,0xfc,0x23];
pub const HISTORY_TRANSACTIONS: [u8;12] = [1,1,0x30,0,2,1,22,0,22,0,6,0x24];
