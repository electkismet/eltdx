use super::{
    bars, cancelled, decode_gbk, source_lines, source_trim, IndustryCodes, IndustryMap, ReadOptions,
};
use crate::error::Result;
use std::collections::HashMap;
use std::path::Path;

/// Source-compatible parser: six Unicode characters, without a digit restriction.
/// Only the whole line is stripped. Individual fields keep their original whitespace.
pub fn parse(content: &[u8]) -> Result<IndustryMap> {
    parse_with_options(content, None)
}

fn parse_with_options(content: &[u8], options: Option<&ReadOptions>) -> Result<IndustryMap> {
    let text = decode_gbk(content);
    parse_text_with_boundary(&text, &mut || {
        cancelled(options.and_then(ReadOptions::cancel))
    })
}

fn parse_text_with_boundary(
    text: &str,
    boundary: &mut impl FnMut() -> Result<()>,
) -> Result<IndustryMap> {
    let mut entries: Vec<(String, IndustryCodes)> = Vec::new();
    let mut positions: HashMap<String, usize> = HashMap::new();
    visit_rows(
        text,
        |code, industry, sw_industry| {
            let value = IndustryCodes::new(industry.to_owned(), sw_industry.to_owned());
            if let Some(index) = positions.get(code).copied() {
                entries[index] = (code.to_owned(), value);
            } else {
                positions.insert(code.to_owned(), entries.len());
                entries.push((code.to_owned(), value));
            }
            Ok(())
        },
        boundary,
    )?;
    Ok(IndustryMap::from_entries(entries))
}

fn visit_rows<E>(
    text: &str,
    mut visit: impl FnMut(&str, &str, &str) -> std::result::Result<(), E>,
    boundary: &mut impl FnMut() -> std::result::Result<(), E>,
) -> std::result::Result<(), E> {
    for line in source_lines(text) {
        boundary()?;
        let mut parts = source_trim(line).split('|');
        parts.next();
        let Some(code) = parts.next() else {
            continue;
        };
        let Some(industry) = parts.next() else {
            continue;
        };
        if code.chars().count() != 6 {
            continue;
        }
        let sw_industry = parts.nth(2).unwrap_or("");
        visit(code, industry, sw_industry)?;
    }
    boundary()
}

#[doc(hidden)]
pub mod python_legacy {
    /// Shared line/field logic after the caller's original GBK decode hook.
    pub fn visit_rows<E>(
        text: &str,
        visit: impl FnMut(&str, &str, &str) -> std::result::Result<(), E>,
        boundary: &mut impl FnMut() -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        super::visit_rows(text, visit, boundary)
    }
}

pub fn read(path: &Path, options: &ReadOptions) -> Result<IndustryMap> {
    parse_with_options(&bars::load(path, options)?, Some(options))
}
