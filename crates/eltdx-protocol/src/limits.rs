pub const REQUEST_HEADER_SIZE: usize = 12;
pub const RESPONSE_HEADER_SIZE: usize = 16;

pub const MAX_REQUEST_DATA_SIZE: usize = u16::MAX as usize - 2;
pub const MAX_REQUEST_FRAME_SIZE: usize = REQUEST_HEADER_SIZE + MAX_REQUEST_DATA_SIZE;
pub const MAX_RESPONSE_PAYLOAD_SIZE: usize = u16::MAX as usize;
pub const MAX_RESPONSE_BUFFER_SIZE: usize = RESPONSE_HEADER_SIZE + MAX_RESPONSE_PAYLOAD_SIZE;
pub const MAX_RESPONSE_RESYNC_BYTES: usize = 0x1_0000;

pub const DEFAULT_CODE_PAGE_SIZE: u16 = 1_600;
pub const MAX_CODE_PAGE_SIZE: u16 = 1_600;
pub const MAX_KLINE_PAGE_SIZE: u16 = 800;
pub const DEFAULT_TRADE_PAGE_SIZE: u16 = 1_800;
pub const MAX_TRADE_PAGE_SIZE: u16 = 1_800;

pub const DEFAULT_FILE_CHUNK_SIZE: u32 = 30_000;
pub const MAX_FILE_CHUNK_SIZE: u32 = 60_000;
pub const MAX_FILE_PATH_BYTES: usize = 300;
pub const MAX_REFRESH_CODES: usize = 100;
pub const MAX_COMMAND_ITEMS: usize = u16::MAX as usize;

pub const MAX_VARINT_BYTES: usize = 10;

pub const SLOT_WIRE_BUDGET_BYTES: usize = 256 * 1024;
pub const SLOT_FRAME_BUDGET: usize = 64;
pub const SLOT_DECODED_BUDGET_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_RAW_STAGING_BUFFER_SIZE: usize = SLOT_WIRE_BUDGET_BYTES + MAX_RESPONSE_BUFFER_SIZE;
pub const MAX_DECODED_QUEUE_FRAMES: usize = 1_024;
pub const MAX_DECODED_QUEUE_BYTES: usize = 8 * 1024 * 1024;

#[cfg(test)]
mod tests {
    use super::{
        MAX_DECODED_QUEUE_BYTES, MAX_DECODED_QUEUE_FRAMES, MAX_RAW_STAGING_BUFFER_SIZE,
        MAX_REQUEST_DATA_SIZE, MAX_REQUEST_FRAME_SIZE, MAX_RESPONSE_BUFFER_SIZE,
        MAX_RESPONSE_PAYLOAD_SIZE, REQUEST_HEADER_SIZE, RESPONSE_HEADER_SIZE,
        SLOT_DECODED_BUDGET_BYTES, SLOT_FRAME_BUDGET, SLOT_WIRE_BUDGET_BYTES,
    };

    #[test]
    fn response_and_staging_bounds_match_the_frozen_plan() {
        assert_eq!(MAX_REQUEST_DATA_SIZE, 65_533);
        assert_eq!(MAX_REQUEST_FRAME_SIZE, REQUEST_HEADER_SIZE + 65_533);
        assert_eq!(MAX_RESPONSE_PAYLOAD_SIZE, 65_535);
        assert_eq!(MAX_RESPONSE_BUFFER_SIZE, RESPONSE_HEADER_SIZE + 65_535);
        assert_eq!(SLOT_WIRE_BUDGET_BYTES, 256 * 1024);
        assert_eq!(SLOT_FRAME_BUDGET, 64);
        assert_eq!(SLOT_DECODED_BUDGET_BYTES, 4 * 1024 * 1024);
        assert_eq!(
            MAX_RAW_STAGING_BUFFER_SIZE,
            256 * 1024 + RESPONSE_HEADER_SIZE + 65_535
        );
        assert_eq!(MAX_DECODED_QUEUE_FRAMES, 1_024);
        assert_eq!(MAX_DECODED_QUEUE_BYTES, 8 * 1024 * 1024);
    }
}
