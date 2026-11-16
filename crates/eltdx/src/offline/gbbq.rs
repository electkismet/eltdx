//! Source GBBQ decryption: three independent eight-byte blocks and five clear bytes.
use std::path::Path;
use crate::error::Result;
use crate::RawPolicy;
use super::{bars, cancelled, malformed, ReadOptions, ReadResult, WireFloat};

#[path = "gbbq_keys.rs"]
mod keys;

/// The category determines the meaning of the four values. They are deliberately
/// not presented as uniformly scaled cash dividends. The source date is not normalized.
#[derive(Clone, Debug)]
pub struct GbbqRecord {
    market: u8, code: String, date_raw: u32, category: u8, values: [WireFloat; 4],
    clear_bytes: [u8; 29], raw: Option<[u8; 29]>,
}
impl GbbqRecord {
    pub const fn market(&self) -> u8 { self.market }
    pub fn code(&self) -> &str { &self.code }
    pub const fn date_raw(&self) -> u32 { self.date_raw }
    pub const fn category(&self) -> u8 { self.category }
    pub const fn values(&self) -> &[WireFloat; 4] { &self.values }
    pub const fn hongli_panqianliutong(&self) -> WireFloat { self.values[0] }
    pub const fn peigujia_qianzongguben(&self) -> WireFloat { self.values[1] }
    pub const fn songgu_qianzongguben(&self) -> WireFloat { self.values[2] }
    pub const fn peigu_houzongguben(&self) -> WireFloat { self.values[3] }
    /// Decrypted business bytes are always retained, including all seven original code bytes.
    pub const fn clear_bytes(&self) -> &[u8; 29] { &self.clear_bytes }
    /// Original encrypted row is available under either Retain policy.
    pub fn raw(&self) -> Option<&[u8; 29]> { self.raw.as_ref() }
}

fn key(offset: usize) -> u32 { bars::u32_at(keys::KEYS, offset) }

fn decrypt_block(input: &[u8], output: &mut [u8]) {
    let mut num = key(0x44) ^ bars::u32_at(input, 0);
    let mut old = bars::u32_at(input, 4);
    for index in (4..=0x40).step_by(4).rev() {
        let mut eax = key((((num & 0xff0000) >> 16) * 4) as usize + 0x448);
        eax = eax.wrapping_add(key(((num >> 24) * 4) as usize + 0x48));
        eax ^= key((((num & 0xff00) >> 8) * 4) as usize + 0x848);
        eax = eax.wrapping_add(key(((num & 0xff) * 4) as usize + 0xc48));
        eax ^= key(index);
        let previous = num;
        num = old ^ eax;
        old = previous;
    }
    old ^= key(0);
    output[..4].copy_from_slice(&old.to_le_bytes());
    output[4..8].copy_from_slice(&num.to_le_bytes());
}

pub fn read(path: &Path, options: &ReadOptions) -> Result<ReadResult<GbbqRecord>> {
    let content = bars::load(path, options)?;
    // Source behavior: even nonempty short headers are an empty result.
    if content.len() < 4 { return Ok(ReadResult::new(Vec::new(), 0, Vec::new())); }
    let count = bars::u32_at(&content, 0) as u64;
    let complete = (content.len() - 4) / 29;
    if count > complete as u64 {
        return Err(malformed("declared GBBQ record is incomplete")
            .with_context("record_index", complete.to_string())
            .with_context("declared_count", count.to_string())
            .with_context("record_size", "29"));
    }
    let mut rows = Vec::new();
    for index in 0..count as usize {
        cancelled(options.cancel())?;
        let offset = 4 + index * 29;
        let encrypted = &content[offset..offset + 29];
        let mut clear = [0_u8; 29];
        for block in 0..3 {
            decrypt_block(&encrypted[block * 8..block * 8 + 8], &mut clear[block * 8..block * 8 + 8]);
        }
        clear[24..].copy_from_slice(&encrypted[24..]);
        let mut code_end = 8;
        while code_end > 1 && clear[code_end - 1] == 0 { code_end -= 1; }
        let code = std::str::from_utf8(&clear[1..code_end]).map_err(|error|
            malformed("GBBQ code is not UTF-8").with_context("record_index", index.to_string()).with_source(error))?.to_owned();
        let values = [13, 17, 21, 25].map(|start| WireFloat::from_bits(bars::u32_at(&clear, start)));
        let raw = if options.raw() == RawPolicy::Omit { None } else {
            let mut bytes = [0_u8; 29];
            bytes.copy_from_slice(encrypted);
            Some(bytes)
        };
        rows.push(GbbqRecord { market: clear[0], code, date_raw: bars::u32_at(&clear, 8),
            category: clear[12], values, clear_bytes: clear, raw });
    }
    cancelled(options.cancel())?;
    Ok(ReadResult::new(rows, 0, Vec::new()))
}
