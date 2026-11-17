//! MAC wire tools with explicit size and association bounds. Shared framing
//! and zlib validation are the same implementation used by EX/MAC owners.
use crate::ex::frame::{self, FrameHeader};
pub use crate::ex::frame::{Association, Frame, FrameAccumulator, FrameLimits};
pub use crate::mac::commands::MacResponseHeader;
use crate::mac::{
    bitmap::{BitmapLayout, FieldSelection},
    commands,
};
use crate::Result;
use bytes::Bytes;

pub fn build_mac_request(command: u16, body: &[u8], head_flag: u8) -> Result<Vec<u8>> {
    commands::build_request(command, body, head_flag).map(|request| request.wire)
}
/// Exact header input is required; trailing bytes must be supplied separately
/// as a frame/body and cannot silently disappear into a successful header parse.
pub fn parse_mac_header(bytes: &[u8]) -> Result<MacResponseHeader> {
    let header = FrameHeader::parse(bytes).map_err(domain)?;
    Ok(MacResponseHeader {
        magic: header.magic(),
        sequence: header.sequence(),
        method: header.method(),
        zip_size: header.compressed_len(),
        unzip_size: header.decoded_len(),
    })
}
pub fn decompress_mac_body(
    header: MacResponseHeader,
    body: Bytes,
    limits: FrameLimits,
) -> Result<Bytes> {
    frame::decode_body(
        FrameHeader::new(
            header.magic,
            header.sequence,
            header.method,
            header.zip_size,
            header.unzip_size,
        ),
        body,
        limits,
    )
    .map_err(domain)
}
/// DEBUG's all-ones bitmap is deliberately available only on this advanced
/// surface. Ordinary selections still use the shared checked encoder.
/// Board16 DEBUG affects its bitmap bytes only, never the exclude control word.
pub fn build_bitmap(fields: &FieldSelection, layout: BitmapLayout) -> Result<Vec<u8>> {
    if fields.is_debug_all() {
        return Ok(vec![
            0xff;
            match layout {
                BitmapLayout::Quote20 => 20,
                BitmapLayout::Board16 => 16,
            }
        ]);
    }
    crate::mac::fields::build_bitmap(fields, layout)
}
fn domain(error: crate::Error) -> crate::Error {
    error.with_context("codec", "mac")
}
