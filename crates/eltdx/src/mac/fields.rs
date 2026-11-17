//! Named schema views over the one bitmap implementation. Unknown legal bits
//! still occupy slots in `bitmap::decode_bitmap`; this named view omits them.
use crate::Result;
use super::bitmap::{self, BitmapLayout, FieldId, FieldSelection};
use super::models::{FieldContext, SlotKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnownField { bit: FieldId, name: &'static str, kind: SlotKind }
impl KnownField {
    pub const fn bit(&self) -> FieldId { self.bit }
    pub fn name(&self) -> &str { self.name }
    pub const fn kind(&self) -> SlotKind { self.kind }
}
pub fn build_bitmap(fields: &FieldSelection, layout: BitmapLayout) -> Result<Vec<u8>> {
    bitmap::encode_bitmap(fields,layout)
}
/// Returns known instrument fields in ascending bit order. This is a schema
/// convenience, never a way to compute wire row width or consume field slots.
pub fn get_active_fields(bytes: &[u8]) -> Result<Vec<KnownField>> {
    active_fields_in(bytes,FieldContext::Instrument)
}
pub fn active_fields_in(bytes: &[u8], context: FieldContext) -> Result<Vec<KnownField>> {
    let bitmap = match bytes.len() {
        16 => { let mut response = [0;20]; response[..16].copy_from_slice(bytes); bitmap::decode_bitmap(&response)? }
        _ => bitmap::decode_bitmap(bytes)?,
    };
    Ok(bitmap.slots.into_iter().filter_map(|slot| {
        let definition = slot.definition?;
        Some(KnownField { bit: slot.bit,name: bitmap::name_for(slot.bit,context)?,kind: definition.kind })
    }).collect())
}
