use std::collections::HashMap;
use std::path::Path;
use crate::error::Result;
use super::{bars, cancelled, decode_gbk, source_lines, source_trim,
    IndustryCodes, IndustryMap, ReadOptions};

/// Source-compatible parser: six Unicode characters, without a digit restriction.
/// Only the whole line is stripped. Individual fields keep their original whitespace.
pub fn parse(content: &[u8]) -> Result<IndustryMap> { parse_with_options(content, None) }

fn parse_with_options(content: &[u8], options: Option<&ReadOptions>) -> Result<IndustryMap> {
    let text = decode_gbk(content);
    let mut entries: Vec<(String, IndustryCodes)> = Vec::new();
    let mut positions = HashMap::new();
    for line in source_lines(&text) {
        cancelled(options.and_then(ReadOptions::cancel))?;
        let parts: Vec<&str> = source_trim(line).split('|').collect();
        if parts.len() < 3 || parts[1].chars().count() != 6 { continue; }
        let value = IndustryCodes::new(parts[2].to_owned(), parts.get(5).copied().unwrap_or("").to_owned());
        if let Some(index) = positions.get(parts[1]).copied() {
            entries[index] = (parts[1].to_owned(), value);
        } else {
            positions.insert(parts[1].to_owned(), entries.len());
            entries.push((parts[1].to_owned(), value));
        }
    }
    cancelled(options.and_then(ReadOptions::cancel))?;
    Ok(IndustryMap::from_entries(entries))
}

pub fn read(path: &Path, options: &ReadOptions) -> Result<IndustryMap> {
    parse_with_options(&bars::load(path, options)?, Some(options))
}
