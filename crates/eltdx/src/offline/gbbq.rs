//! Source GBBQ decryption: three independent eight-byte blocks and five clear bytes.
use std::path::Path;
use crate::error::Result;
use crate::RawPolicy;
use super::{bars, cancelled, context_value, malformed, ReadOptions, ReadResult, WireFloat};

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

/// One cipher traversal for checked Rust words and the Python object/Struct
/// adapter. The adapter controls scalar operations, never the round order.
#[doc(hidden)]
pub mod python_legacy {
    pub trait Cipher {
        type Word;
        type Error;
        fn key(&mut self, offset: usize) -> Result<Self::Word, Self::Error>;
        fn input(&mut self, offset: usize) -> Result<Self::Word, Self::Error>;
        fn table(&mut self, word: &Self::Word, mask: Option<u32>, shift: u32,
            base: usize) -> Result<Self::Word, Self::Error>;
        fn xor_mask(&mut self, left: &Self::Word, right: &Self::Word) -> Result<Self::Word, Self::Error>;
        fn add_mask(&mut self, left: &Self::Word, right: &Self::Word) -> Result<Self::Word, Self::Error>;
    }

    pub fn decrypt_block<C: Cipher>(cipher: &mut C, offset: usize) -> Result<(C::Word, C::Word), C::Error> {
        let initial = cipher.key(0x44)?;
        let input = cipher.input(offset)?;
        let mut num = cipher.xor_mask(&initial, &input)?;
        let mut old = cipher.input(offset + 4)?;
        for round in (1..=16).rev() {
            let mut eax = cipher.table(&num, Some(0xff0000), 16, 0x448)?;
            let add = cipher.table(&num, None, 24, 0x48)?;
            eax = cipher.add_mask(&eax, &add)?;
            let xor = cipher.table(&num, Some(0xff00), 8, 0x848)?;
            eax = cipher.xor_mask(&eax, &xor)?;
            let add = cipher.table(&num, Some(0xff), 0, 0xc48)?;
            eax = cipher.add_mask(&eax, &add)?;
            let xor = cipher.key(round * 4)?;
            eax = cipher.xor_mask(&eax, &xor)?;
            let previous = num;
            num = cipher.xor_mask(&old, &eax)?;
            old = previous;
        }
        let last = cipher.key(0)?;
        old = cipher.xor_mask(&old, &last)?;
        Ok((old, num))
    }
}

struct NativeCipher<'a> { input: &'a [u8] }
impl python_legacy::Cipher for NativeCipher<'_> {
    type Word = u32;
    type Error = std::convert::Infallible;
    fn key(&mut self, offset: usize) -> std::result::Result<u32, Self::Error> { Ok(bars::u32_at(keys::KEYS, offset)) }
    fn input(&mut self, offset: usize) -> std::result::Result<u32, Self::Error> { Ok(bars::u32_at(self.input, offset)) }
    fn table(&mut self, word: &u32, mask: Option<u32>, shift: u32, base: usize) -> std::result::Result<u32, Self::Error> {
        self.key((((word & mask.unwrap_or(u32::MAX)) >> shift) * 4) as usize + base)
    }
    fn xor_mask(&mut self, left: &u32, right: &u32) -> std::result::Result<u32, Self::Error> { Ok(left ^ right) }
    fn add_mask(&mut self, left: &u32, right: &u32) -> std::result::Result<u32, Self::Error> { Ok(left.wrapping_add(*right)) }
}

fn decrypt_block(input: &[u8], output: &mut [u8]) {
    let result = python_legacy::decrypt_block(&mut NativeCipher { input }, 0);
    let (old, num) = match result { Ok(words) => words, Err(impossible) => match impossible {} };
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
            .with_context("record_index", context_value(complete))
            .with_context("declared_count", context_value(count))
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
            malformed("GBBQ code is not UTF-8").with_context("record_index", context_value(index)).with_source(error))?.to_owned();
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
