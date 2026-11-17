use crate::error::{Error, ErrorKind, Result};
use crate::types::batch::{PageCursor, PageOptions};
use crate::types::{Price, SecurityId};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum SecurityCategory {
    AShare,
    BShare,
    Etf,
    Index,
    PrivateConvertibleBond,
    Bond,
    Unknown,
    Other(String),
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum SecurityBoard {
    SseMain,
    SzseMain,
    Star,
    ChiNext,
    Beijing,
    None,
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Security {
    id: SecurityId,
    name: String,
    multiple: u16,
    decimal: u8,
    previous_close: Price,
    volume_ratio_base: f64,
    category: SecurityCategory,
    category_reason: String,
    board: SecurityBoard,
    board_reason: String,
}

impl Security {
    pub(crate) fn new(
        id: SecurityId,
        name: String,
        multiple: u16,
        decimal: u8,
        previous_close: Price,
        volume_ratio_base: f64,
        category: SecurityCategory,
        category_reason: String,
        board: SecurityBoard,
        board_reason: String,
    ) -> Self {
        Self {
            id,
            name,
            multiple,
            decimal,
            previous_close,
            volume_ratio_base,
            category,
            category_reason,
            board,
            board_reason,
        }
    }

    pub fn id(&self) -> &SecurityId {
        &self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn multiple(&self) -> u16 {
        self.multiple
    }
    pub const fn decimal(&self) -> u8 {
        self.decimal
    }
    pub const fn previous_close(&self) -> Price {
        self.previous_close
    }
    pub const fn volume_ratio_base(&self) -> f64 {
        self.volume_ratio_base
    }
    pub fn category(&self) -> &SecurityCategory {
        &self.category
    }
    pub fn category_reason(&self) -> &str {
        &self.category_reason
    }
    pub fn board(&self) -> &SecurityBoard {
        &self.board
    }
    pub fn board_reason(&self) -> &str {
        &self.board_reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeQuery {
    start: u32,
    limit: usize,
    cursor: Option<PageCursor>,
}

impl CodeQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_start(mut self, value: u32) -> Result<Self> {
        self.start = value;
        self.cursor = None;
        Ok(self)
    }

    pub fn with_limit(mut self, value: usize) -> Result<Self> {
        if value > 1_600 {
            return Err(query_error("limit", "must be between 0 and 1600"));
        }
        self.limit = value;
        Ok(self)
    }

    pub fn with_cursor(mut self, cursor: &PageCursor) -> Result<Self> {
        self.start = cursor.next_offset();
        self.cursor = Some(cursor.clone());
        Ok(self)
    }

    pub const fn start(&self) -> u32 {
        self.start
    }
    pub const fn limit(&self) -> usize {
        self.limit
    }
    pub fn cursor(&self) -> Option<&PageCursor> {
        self.cursor.as_ref()
    }
}

impl Default for CodeQuery {
    fn default() -> Self {
        Self {
            start: 0,
            limit: 1_600,
            cursor: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeScanQuery {
    pages: PageOptions,
    category: Option<SecurityCategory>,
    cursor: Option<PageCursor>,
}

impl CodeScanQuery {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pages(mut self, value: PageOptions) -> Result<Self> {
        validate_scan_pages(&value)?;
        self.pages = value;
        Ok(self)
    }

    pub fn with_category(mut self, value: Option<SecurityCategory>) -> Result<Self> {
        self.category = value;
        Ok(self)
    }

    pub fn with_cursor(mut self, cursor: &PageCursor) -> Result<Self> {
        if cursor.page_size() != self.pages.page_size() {
            return Err(query_error("cursor", "page size does not match query"));
        }
        self.cursor = Some(cursor.clone());
        Ok(self)
    }

    pub fn pages(&self) -> &PageOptions {
        &self.pages
    }
    pub fn category(&self) -> Option<&SecurityCategory> {
        self.category.as_ref()
    }
    pub fn cursor(&self) -> Option<&PageCursor> {
        self.cursor.as_ref()
    }
}

impl Default for CodeScanQuery {
    fn default() -> Self {
        Self {
            pages: PageOptions::new(1_600, 200, 320_000).unwrap_or_else(|_| unreachable!()),
            category: None,
            cursor: None,
        }
    }
}

pub(crate) fn validate_scan_pages(value: &PageOptions) -> Result<()> {
    if value.page_size() > 1_600 {
        return Err(query_error("page_size", "must not exceed 1600"));
    }
    Ok(())
}

pub(crate) fn query_error(field: &'static str, message: &'static str) -> Error {
    Error::new(ErrorKind::InvalidArgument, message).with_context("field", field)
}
