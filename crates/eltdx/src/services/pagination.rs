//! Common count/offset admission for directory, bar and tick collectors.

use crate::error::{Error, ErrorKind, Result};

#[derive(Clone, Copy, Debug)]
pub(crate) struct CollectionLimits {
    pub(crate) pages: Option<usize>,
    pub(crate) records: Option<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct PageProgress {
    pages: usize,
    records: usize,
}

impl PageProgress {
    pub(crate) fn before_request(&self, limits: CollectionLimits) -> Result<()> {
        if limits.pages.is_some_and(|maximum| self.pages >= maximum) {
            return Err(limit_error("pages", limits.pages.unwrap_or(0)));
        }
        Ok(())
    }

    /// Called before reserving/appending an incoming page. A full record cap
    /// still permits the terminal empty probe, subject to the page cap.
    pub(crate) fn accept(&mut self, count: usize, requested: usize, limits: CollectionLimits) -> Result<()> {
        if count > requested { return Err(Error::new(ErrorKind::Protocol, "response page exceeds requested size")); }
        let records = self.records.checked_add(count).ok_or_else(offset_error)?;
        if limits.records.is_some_and(|maximum| records > maximum) {
            return Err(limit_error("records", limits.records.unwrap_or(0)));
        }
        self.pages = self.pages.checked_add(1).ok_or_else(offset_error)?;
        self.records = records;
        Ok(())
    }

    pub(crate) fn pages(&self) -> usize { self.pages }
    pub(crate) fn records(&self) -> usize { self.records }
}

pub(crate) fn advance_offset(offset: u32, count: usize) -> Result<u32> {
    offset.checked_add(u32::try_from(count).map_err(|_| offset_error())?).ok_or_else(offset_error)
}

fn limit_error(resource: &'static str, maximum: usize) -> Error {
    Error::new(ErrorKind::LimitExceeded, "complete collection reached its cap without EOF")
        .with_context("resource", resource).with_context("limit", format!("{maximum}"))
}
fn offset_error() -> Error { Error::new(ErrorKind::LimitExceeded, "page count, record count or continuation offset overflows") }
