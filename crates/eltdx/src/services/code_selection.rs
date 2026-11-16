//! Directory selection shared by typed Rust services and the legacy row adapter.

use crate::types::{Security, SecurityCategory};
use crate::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodeSelection { Stocks, AShares, Etfs, Indices, StName }

impl CodeSelection {
    pub fn from_name(name: &str) -> Result<Self> {
        match name {
            "stocks" => Ok(Self::Stocks), "a_shares" => Ok(Self::AShares),
            "etfs" => Ok(Self::Etfs), "indices" => Ok(Self::Indices), "st" => Ok(Self::StName),
            _ => Err(Error::new(ErrorKind::InvalidArgument, "unknown directory selection")),
        }
    }

    pub fn matches_legacy(self, category: &str, name: &str) -> bool {
        self.matches_parts(category, name, true)
    }

    pub(crate) fn matches_security(self, row: &Security) -> bool {
        let category = match row.category() {
            SecurityCategory::AShare => "a_share", SecurityCategory::BShare => "b_share",
            SecurityCategory::Etf => "etf", SecurityCategory::Index => "index",
            _ => "other",
        };
        self.matches_parts(category, row.name(), false)
    }

    fn matches_parts(self, category: &str, name: &str, legacy: bool) -> bool {
        match self {
            Self::Stocks => matches!(category, "a_share" | "b_share"),
            Self::AShares => category == "a_share", Self::Etfs => category == "etf",
            Self::Indices => category == "index", Self::StName => st_name(name, legacy),
        }
    }
}

pub(crate) fn st_name(name: &str, legacy: bool) -> bool {
    let upper = if legacy {
        // Python str.strip includes these four ASCII information separators;
        // its full Unicode upper also expands long-s and ligatures.
        name.trim_matches(|scalar: char| scalar.is_whitespace() || matches!(scalar, '\u{1c}'..='\u{1f}'))
            .to_uppercase()
    } else { name.trim().to_ascii_uppercase() };
    ["ST", "*ST", "SST", "S*ST"].iter().any(|prefix| upper.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_and_typed_selection_share_categories_without_reordering() -> Result<()> {
        let rows = [("a_share", "ordinary"), ("b_share", "ordinary"), ("etf", "ordinary"),
            ("index", "ordinary"), ("a_share", "*ST TEST"), ("unknown", "SST TEST")];
        for (name, expected) in [("stocks", vec![0, 1, 4]), ("a_shares", vec![0, 4]),
            ("etfs", vec![2]), ("indices", vec![3]), ("st", vec![4, 5])] {
            let selection = CodeSelection::from_name(name)?;
            let actual = rows.iter().enumerate().filter_map(|(index, (category, name))|
                selection.matches_legacy(category, name).then_some(index)).collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
        assert!(st_name("\u{1c}ſt股份", true));
        assert!(!st_name("\u{1c}ſt股份", false));
        assert!(st_name("  s*st name  ", true));
        assert!(!st_name("normal ST name", true));
        Ok(())
    }
}
