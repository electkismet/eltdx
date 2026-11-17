//! Source request layout registry. Native byte codecs and the Python facade
//! share flags, tags and field layouts without sharing input-validation policy.
pub const HEAD_FLAG: u8 = 0x1c;
pub const STANDARD_HEAD_FLAG: u8 = 0x0c;
pub const REQUEST_HEADER_SIZE: usize = 10;
pub const RESPONSE_HEADER_SIZE: usize = 16;
pub const BARS_FORMAT: &str = "<H22sHHIHHbbbbH4s";
pub const BARS_FLAGS: [u8;4] = [1,1,0,1];
pub const MINUTE_FORMAT: &str = "<H22sI5H";
pub const MULTI_MINUTE_FORMAT: &str = "<H22sIHH6x";
pub const TRANSACTIONS_FORMAT: &str = "<H22sIIH10x";
pub const INFO_FORMAT: &str = "<H22sI12x";
pub const BOARDS_FORMAT: &str = "<HHBBHH8x";
pub const BOARD_MEMBERS_FORMAT: &str = "<I9xHIHBB";
pub const JSON_FORMAT: &str = "<H8s16x21s";
pub const AUCTION_FORMAT: &str = "<H22sII10x";
pub const UNUSUAL_FORMAT: &str = "<HH2xH2xH5H";
pub const OFFSETS_FORMAT: &str = "<II5x";
pub const SAMPLING_FORMAT: &str = "<H22sHH9x";
pub const GOODS_FORMAT: &str = "<HII";
pub const BELONG_TAG: &[u8;10] = b"Stock_GLHQ";
pub const CAPITAL_TAG: &[u8;10] = b"Stock_ZJLX";
pub const SERVER_PREFIX: [u8;4] = [4,0,0x2d,0x31];
pub const SERVER_MARKER: [u8;4] = [0,0x27,6,0x0e];
