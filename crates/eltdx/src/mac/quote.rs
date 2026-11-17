//! Validated public quote projection, preserving bit identity and source text.
use super::models::{MacCode, MacInstrumentId, QuoteField, QuoteRow};
use super::responses::WireText;
use super::FieldId;
use crate::types::ResultMeta;
use crate::{Error, ErrorKind, Result};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Quote {
    id: MacInstrumentId,
    name: WireText<44>,
    code_raw: [u8; 22],
    fields: Vec<QuoteField>,
    reserved_bitmap: [u8; 20],
    meta: Arc<ResultMeta>,
}
impl Quote {
    pub fn id(&self) -> &MacInstrumentId {
        &self.id
    }
    pub fn name(&self) -> &str {
        self.name.as_str()
    }
    pub fn name_source(&self) -> &WireText<44> {
        &self.name
    }
    pub const fn code_raw(&self) -> &[u8; 22] {
        &self.code_raw
    }
    pub fn fields(&self) -> &[QuoteField] {
        &self.fields
    }
    pub fn field(&self, bit: FieldId) -> Option<&QuoteField> {
        self.fields.iter().find(|value| value.bit == bit)
    }
    pub const fn reserved_bitmap(&self) -> &[u8; 20] {
        &self.reserved_bitmap
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
    /// Heap storage copied by Quote::clone. Shared Arc metadata is charged by
    /// the enclosing composition once, before it creates its aggregate view.
    pub(crate) fn clone_heap_bytes(&self) -> Result<usize> {
        let mut charge = crate::types::memory::MemoryCharge::default();
        charge.add(self.id.code().heap_bytes())?;
        charge.add(self.name.heap_bytes())?;
        charge.array::<QuoteField>(self.fields.capacity())?;
        for field in &self.fields {
            if let Some(text) = &field.derived_text {
                charge.string(text)?;
            }
            if let super::models::NativeUnit::Currency(unit) = field.meaning.unit() {
                charge.string(unit)?;
            }
        }
        Ok(charge.total())
    }
    pub(crate) fn from_row(row: QuoteRow) -> Result<Self> {
        let code = WireText::from_wire(row.code_raw);
        if code.replacement_used() {
            return Err(protocol("MAC quote identity has invalid GBK bytes"));
        }
        let id = MacInstrumentId::new(
            row.market,
            MacCode::new(code.as_str()).map_err(|error| {
                protocol("MAC quote identity is not representable").with_source(error)
            })?,
        );
        Ok(Self {
            id,
            name: WireText::from_wire(row.name_raw),
            code_raw: row.code_raw,
            fields: row.fields,
            reserved_bitmap: row.reserved_bitmap,
            meta: Arc::new(ResultMeta::default()),
        })
    }
    pub(crate) fn with_meta(mut self, meta: Arc<ResultMeta>) -> Self {
        self.meta = meta;
        self
    }
    pub(crate) fn set_meta(&mut self, meta: Arc<ResultMeta>) {
        self.meta = meta;
    }
}
fn protocol(message: &'static str) -> Error {
    Error::new(ErrorKind::Protocol, message).with_context("domain", "mac")
}
