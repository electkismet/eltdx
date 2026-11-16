use crate::{Error, ErrorKind, Result};
use super::{ResultMeta, SecurityId};

#[derive(Clone, Debug, PartialEq)]
pub struct Table<T> { rows: Vec<T>, meta: ResultMeta, end: Option<super::EndEvidence> }
impl<T> Table<T> {
    pub(crate) fn new(rows: Vec<T>, meta: ResultMeta) -> Self { Self { rows, meta, end: None } }
    pub(crate) fn with_end(mut self, end: super::EndEvidence) -> Self { self.end = Some(end); self }
    pub fn rows(&self) -> &[T] { &self.rows }
    pub fn into_rows(self) -> Vec<T> { self.rows }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    /// Collection evidence where the table is built by a paginated helper.
    /// A non-collection table (for example a company directory) returns None.
    pub const fn end(&self) -> Option<super::EndEvidence> { self.end }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanySectionQuery { filename: String, offset: u32, length: u32 }
impl CompanySectionQuery {
    pub fn new(filename: String, offset: u32, length: u32) -> Result<Self> {
        let value = Self { filename: filename.trim().to_owned(), offset, length }; value.validate()?; Ok(value)
    }
    pub fn with_filename(mut self, value: String) -> Result<Self> { self.filename = value.trim().to_owned(); self.validate()?; Ok(self) }
    pub fn with_offset(mut self, value: u32) -> Result<Self> { self.offset = value; self.validate()?; Ok(self) }
    pub fn with_length(mut self, value: u32) -> Result<Self> { self.length = value; self.validate()?; Ok(self) }
    pub fn filename(&self) -> &str { &self.filename }
    pub const fn offset(&self) -> u32 { self.offset }
    pub const fn length(&self) -> u32 { self.length }
    pub(crate) fn validate(&self) -> Result<()> {
        crate::encoding::encode_gbk_bounded(&self.filename, 80)?;
        if self.length == 0 || self.offset.checked_add(self.length).is_none() {
            return Err(Error::new(ErrorKind::InvalidArgument, "company byte range must be nonempty and fit u32"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanySection { name: String, filename: String, offset: u32, length: u32 }
impl CompanySection {
    pub(crate) fn new(name: String, filename: String, offset: u32, length: u32) -> Self { Self { name, filename, offset, length } }
    pub fn name(&self) -> &str { &self.name }
    pub fn filename(&self) -> &str { &self.filename }
    pub const fn offset(&self) -> u32 { self.offset }
    pub const fn length(&self) -> u32 { self.length }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompanyText {
    pub(crate) id: SecurityId, pub(crate) filename: String, pub(crate) offset: u32,
    pub(crate) requested_length: u32, pub(crate) text: String, pub(crate) decoding_had_errors: bool,
    pub(crate) meta: ResultMeta,
}
impl CompanyText {
    pub fn id(&self) -> &SecurityId { &self.id }
    pub fn filename(&self) -> &str { &self.filename }
    pub const fn offset(&self) -> u32 { self.offset }
    pub const fn requested_length(&self) -> u32 { self.requested_length }
    pub fn text(&self) -> &str { &self.text }
    pub const fn decoding_had_errors(&self) -> bool { self.decoding_had_errors }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
