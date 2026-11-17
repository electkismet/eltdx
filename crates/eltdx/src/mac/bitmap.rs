//! Source-defined MAC bitmap schema and slot positioning.
use crate::{Error, ErrorKind, Result};
use crate::mac::models::{FieldContext, FieldMeaning, NativeUnit, SlotKind, SlotValue, WireFloat};

pub const BITMAP_BYTES: usize = 20;
pub const BOARD_BITMAP_BYTES: usize = 16;
pub const FIELD_LIMIT: u8 = 151;
pub const MAX_FIELD_ID: u8 = 150;

fn invalid(message: impl Into<String>) -> Error { Error::new(ErrorKind::InvalidArgument,message).with_context("domain","mac") }
fn protocol(message: impl Into<String>) -> Error { Error::new(ErrorKind::Protocol,message).with_context("domain","mac") }

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FieldId(u8);
impl FieldId {
    pub fn new(value: u8) -> Result<Self> { if value <= MAX_FIELD_ID { Ok(Self(value)) } else { Err(invalid("MAC field id must be in 0..=150")) } }
    pub const fn get(self) -> u8 { self.0 }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FieldDefinition { pub id: FieldId, pub name: &'static str, pub kind: SlotKind }

macro_rules! f { ($id:literal,$name:literal,$kind:ident) => { FieldDefinition { id: FieldId($id), name: $name, kind: SlotKind::$kind } }; }
/// Every named FieldBit in the source. Aliases are preserved as context-specific names.
pub const FIELDS: &[FieldDefinition] = &[
    f!(0,"PRE_CLOSE",F32),f!(1,"OPEN",F32),f!(2,"HIGH",F32),f!(3,"LOW",F32),f!(4,"CLOSE",F32),f!(5,"VOL",U32),f!(6,"VOL_RATIO",F32),f!(7,"AMOUNT",F32),
    f!(8,"INSIDE_VOLUME",U32),f!(9,"OUTSIDE_VOLUME",U32),f!(10,"TOTAL_SHARES",F32),f!(11,"FLOAT_SHARES",F32),f!(12,"EPS",F32),f!(13,"NET_ASSETS",F32),f!(14,"SECURITY_TYPE_PRICE",F32),f!(15,"TOTAL_MARKET_CAP_AB",F32),
    f!(16,"PE_DYNAMIC",F32),f!(17,"BID_PRICE",F32),f!(18,"ASK_PRICE",F32),f!(19,"SERVER_UPDATE_DATE",U32),f!(20,"SERVER_UPDATE_TIME",U32),f!(21,"LOT_SIZE_INFO",U32),f!(22,"BOARD_STRENGTH",F32),f!(23,"DIVIDEND_YIELD",F32),f!(24,"BID_VOLUME",U32),f!(25,"ASK_VOLUME",U32),f!(26,"LAST_VOLUME",U32),f!(27,"TURNOVER",F32),f!(28,"INDUSTRY",U32),f!(29,"INDUSTRY_CHANGE_UP",F32),f!(30,"STOCK_TAG_FLAGS",U32),f!(31,"DECIMAL_POINT",U32),
    f!(32,"BUY_PRICE_LIMIT",F32),f!(33,"SELL_PRICE_LIMIT",F32),f!(34,"PRICE_DECIMAL_INFO",U32),f!(35,"LOT_SIZE",U32),f!(36,"PRE_IOPV",F32),f!(37,"SPEED_PCT",F32),f!(38,"AVG_PRICE",F32),f!(39,"IOPV",F32),f!(40,"PE_TTM_VOL_RELATED",F32),f!(41,"EX_PRICE_PLACEHOLDER",F32),f!(42,"OPERATING_REVENUE",F32),f!(43,"FLAG_KCB",U32),f!(44,"FLAG_BJ",U32),f!(45,"CIRCULATING_CAPITAL_Z",F32),f!(46,"AFTER_HOURS_VOLUME",I32),
    f!(48,"PE_TTM",F32),f!(49,"PE_STATIC",F32),f!(55,"INDEX_METRIC",F32),f!(56,"MAIN_NET_AMOUNT",F32),f!(57,"BID_ASK_RATIO",F32),f!(58,"NON_INDEX_FLAG",U32),f!(59,"CHANGE_20D_PCT",F32),f!(60,"YTD_PCT",F32),f!(62,"STOCK_CLASS_CODE",U32),f!(63,"PERCENT_BASE",U32),
    f!(64,"MTD_PCT",F32),f!(65,"CHANGE_1Y_PCT",F32),f!(66,"PREV_CHANGE_PCT",F32),f!(67,"CHANGE_3D_PCT",F32),f!(68,"CHANGE_60D_PCT",F32),f!(69,"CHANGE_5D_PCT",F32),f!(70,"CHANGE_10D_PCT",F32),f!(71,"PREV2_CHANGE_PCT",F32),f!(72,"BID2_PRICE",F32),f!(73,"ASK2_PRICE",F32),f!(74,"AH_CODE",U32),f!(75,"UNKNOWN_CODE",U32),
    f!(87,"OPEN_AMOUNT",F32),f!(88,"ANNUAL_LIMIT_UP_DAYS",I32),f!(89,"ACTIVITY",U32),f!(91,"DIVIDEND_YIELD_RATE",F32),f!(92,"CONSECUTIVE_UP_DAYS",I32),
    f!(93,"BID2_VOLUME",U32),f!(93,"LIMIT_UP_COUNT",U32),f!(94,"ASK2_VOLUME",U32),f!(94,"LIMIT_DOWN_COUNT",U32),f!(95,"INDUSTRY_SUB",U32),
    f!(102,"AUCTION_BUY_LIMIT",F32),f!(103,"AUCTION_SELL_LIMIT",F32),f!(104,"VOL_SPEED_PCT",F32),f!(105,"SHORT_TURNOVER_PCT",F32),f!(106,"AMOUNT_2M",F32),f!(107,"MAIN_NET_AMOUNT_COPY",F32),f!(108,"MAIN_NET_RATIO",F32),f!(109,"RETAIL_NET_AMOUNT",F32),f!(110,"MAIN_NET_5M_AMOUNT",F32),f!(111,"MAIN_NET_3D_AMOUNT",F32),f!(112,"MAIN_NET_5D_AMOUNT",F32),f!(113,"MAIN_NET_10D_AMOUNT",F32),f!(114,"MAIN_BUY_NET_AMOUNT",F32),f!(115,"DDX",F32),f!(116,"DDY",F32),f!(117,"DDZ",F32),f!(118,"DDF",F32),f!(119,"STOCK_FLAG_A",F32),f!(120,"STOCK_FLAG_B",F32),f!(122,"AUCTION_VOL_RATIO",F32),f!(123,"PREV_AMOUNT",F32),f!(125,"RECENT_INDICATOR",F32),
    f!(128,"BID3_PRICE",F32),f!(129,"BID4_PRICE",F32),f!(130,"BID5_PRICE",F32),f!(131,"ASK3_PRICE",F32),f!(132,"ASK4_PRICE",F32),f!(133,"ASK5_PRICE",F32),f!(134,"BID3_VOLUME",U32),f!(135,"BID4_VOLUME",U32),f!(136,"BID5_VOLUME",U32),f!(136,"UP_COUNT",U32),f!(137,"ASK3_VOLUME",U32),f!(138,"ASK4_VOLUME",U32),f!(139,"ASK5_VOLUME",U32),f!(139,"DOWN_COUNT",U32),f!(140,"BID_ASK_DIFF",I32),f!(141,"CHANGE_UP_TYPE",I32),f!(142,"SAFETY_SCORE",F32),f!(143,"HIGHLIGHT_COUNT",F32),f!(144,"CHANGE_AT_1000",F32),f!(145,"CHANGE_AT_1030",F32),f!(146,"CHANGE_AT_1100",F32),f!(147,"CHANGE_AT_1130",F32),f!(148,"CHANGE_AT_1330",F32),f!(149,"CHANGE_AT_1400",F32),f!(150,"CHANGE_AT_1430",F32),
];

pub fn definition(id: FieldId) -> Option<&'static FieldDefinition> { FIELDS.iter().find(|field| field.id == id && !matches!((id.get(), field.name), (93,"LIMIT_UP_COUNT") | (94,"LIMIT_DOWN_COUNT") | (136,"UP_COUNT") | (139,"DOWN_COUNT"))) }
pub fn name_for(id: FieldId, context: FieldContext) -> Option<&'static str> {
    Some(match (id.get(), context) { (93,FieldContext::Board)=>"LIMIT_UP_COUNT",(94,FieldContext::Board)=>"LIMIT_DOWN_COUNT",(136,FieldContext::Board)=>"UP_COUNT",(139,FieldContext::Board)=>"DOWN_COUNT",_=>definition(id)?.name })
}
pub fn meaning(id: FieldId) -> FieldMeaning {
    match id.get() {
        7|87|106|123 => FieldMeaning::documented_as(NativeUnit::Currency("元".into()), "source comment specifies yuan; currency context remains unverified"),
        10|11|45 => FieldMeaning::documented_as(NativeUnit::Shares, "source comment specifies 10,000 shares"),
        19 => FieldMeaning::documented_as(NativeUnit::Unknown, "YYYYMMDD"),
        20 => FieldMeaning::documented_as(NativeUnit::Unknown, "HHMMSS"),
        59|60|64..=70|71|91|104|105|108|144..=150 => FieldMeaning::documented_as(NativeUnit::Percent, "percentage-point wire value; no scaling applied"),
        21|35|41 => FieldMeaning::unknown(),
        _ => FieldMeaning::unknown(),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresetField { None, Ohlc, Basic, Quote, Volume, Fundamental, Enhanced, AhCodeFields, BoardStats, Handicap, Common, All, Debug }
impl PresetField {
    fn ids(self) -> &'static [u8] { match self {
        Self::None=>&[],Self::Ohlc=>&[1,2,3,4],Self::Basic=>&[1,2,3,4,0,5],
        Self::Quote=>&[17,18,24,25,26],Self::Volume=>&[5,7,27,6],Self::Fundamental=>&[10,11,12,13],
        Self::Enhanced=>&[1,2,3,4,5,11,89],Self::AhCodeFields=>&[1,2,3,4,5,74,35,28],
        Self::BoardStats=>&[93,94,136,139],
        Self::Handicap=>&[17,72,128,129,130,18,73,131,132,133,24,93,134,135,136,25,94,137,138,139],
        Self::Common=>&[0,1,2,3,4,5,6,7,10,11,12,13,14,15,16,21,23,26,27,30,31,32,33,34,35,36,37,43,48,49,56,104,105,45],
        Self::All=>&[0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31,32,33,34,35,36,37,38,39,40,41,42,43,44,45,46,48,49,55,56,57,58,59,60,62,63,64,65,66,67,68,69,70,71,72,73,74,75,87,88,89,91,92,93,94,95,102,103,104,105,106,107,108,109,110,111,112,113,114,115,116,117,118,119,120,122,123,125,128,129,130,131,132,133,134,135,136,137,138,139,140,141,142,143,144,145,146,147,148,149,150],
        Self::Debug=>&[],
    }}
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldSelection { bits: [u64; 3], debug_all: bool }
impl Default for FieldSelection { fn default() -> Self { Self::empty() } }
impl FieldSelection {
    pub const fn empty() -> Self { Self { bits:[0;3],debug_all:false } }
    pub fn from_ids(ids: impl IntoIterator<Item=FieldId>) -> Self { let mut result=Self::empty(); for id in ids { result.insert(id); } result }
    pub fn preset(preset: PresetField) -> Self {
        if preset==PresetField::Debug { return Self { bits:[0;3],debug_all:true }; }
        Self::from_ids(preset.ids().iter().filter_map(|id| FieldId::new(*id).ok()))
    }
    pub fn insert(&mut self,id:FieldId) { let n=usize::from(id.get()); self.bits[n/64]|=1_u64<<(n%64); }
    pub const fn contains(&self,id:FieldId)->bool { let n=id.get() as usize; self.bits[n/64]&(1_u64<<(n%64))!=0 }
    pub fn ids(&self)->Vec<FieldId> { (0..=MAX_FIELD_ID).filter_map(|id| {let value=FieldId(id);self.contains(value).then_some(value)}).collect() }
    pub fn is_debug_all(&self)->bool { self.debug_all }
    pub fn mac_ex_default()->Self { Self::from_ids([0,1,2,3,4,5,7].into_iter().map(FieldId)) }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitmapLayout { Quote20, Board16 }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SlotDescriptor { pub slot: usize, pub bit: FieldId, pub definition: Option<&'static FieldDefinition> }
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedBitmap { pub raw: [u8;20], pub reserved: [u8;20], pub slots: Vec<SlotDescriptor> }

pub fn encode_bitmap(selection:&FieldSelection,layout:BitmapLayout)->Result<Vec<u8>> {
    let width=match layout { BitmapLayout::Quote20=>20,BitmapLayout::Board16=>16 };
    if selection.debug_all { return Err(invalid("DEBUG all-ones bitmap is reserved for the advanced surface")); }
    if layout==BitmapLayout::Board16 && selection.ids().iter().any(|id| id.get()>=128) { return Err(invalid("0x122C bitmap cannot select field bits 128..=150")); }
    let mut out=vec![0;width];
    for id in selection.ids(){let bit=usize::from(id.get());out[bit/8]|=1<<(bit%8);}
    Ok(out)
}
pub fn decode_bitmap(bytes:&[u8])->Result<DecodedBitmap> {
    if bytes.len()!=20 { return Err(protocol("MAC response bitmap must be exactly 20 bytes")); }
    let raw:[u8;20]=bytes.try_into().map_err(|_|protocol("MAC response bitmap length mismatch"))?;
    let mut reserved=[0;20];
    for bit in 151..160 { if raw[bit/8]&(1<<(bit%8))!=0 { reserved[bit/8]|=1<<(bit%8); } }
    let mut slots=Vec::new();
    let mut cursor = 0;
    while let Some(bit) = next_set_bit(&raw,&mut cursor,usize::from(FIELD_LIMIT)) {
        let id=FieldId(bit as u8);slots.push(SlotDescriptor{slot:slots.len(),bit:id,definition:definition(id)});
    }
    Ok(DecodedBitmap{raw,reserved,slots})
}
/// Shared ascending bit traversal. `limit` excludes reserved positions for the
/// strict decoder; the Python adapter supplies its already-masked integer's
/// bytes and keeps the legacy dynamic FIELD_BIT_LIMIT policy.
#[doc(hidden)]
pub fn next_set_bit(bytes: &[u8], cursor: &mut usize, limit: usize) -> Option<usize> {
    while *cursor < limit {
        let byte = *bytes.get(*cursor/8)?;
        let candidate = byte & (u8::MAX << (*cursor%8));
        if candidate == 0 { *cursor = (*cursor/8).checked_add(1)?.checked_mul(8)?; continue; }
        let bit = (*cursor/8).checked_mul(8)?.checked_add(candidate.trailing_zeros() as usize)?;
        if bit >= limit { return None; }
        *cursor = bit.checked_add(1)?; return Some(bit);
    }
    None
}
pub fn decode_slot(desc:SlotDescriptor, raw:[u8;4])->SlotValue {
    match desc.definition.map(|field|field.kind) {Some(SlotKind::F32)=>SlotValue::F32(WireFloat::from_bits(u32::from_le_bytes(raw))),Some(SlotKind::U32)=>SlotValue::U32(u32::from_le_bytes(raw)),Some(SlotKind::I32)=>SlotValue::I32(i32::from_le_bytes(raw)),None=>SlotValue::Unknown(raw)}
}
pub fn quote_row_width(bitmap:&DecodedBitmap)->Result<usize> { 68usize.checked_add(bitmap.slots.len().checked_mul(4).ok_or_else(||protocol("MAC slot count overflow"))?).ok_or_else(||protocol("MAC row width overflow")) }
