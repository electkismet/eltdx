//! Pure MAC command payload codecs. Stream order, profile negotiation, transport
//! admission and retries are deliberately left to Stage B.
use crate::mac::bitmap::{decode_bitmap, decode_slot, encode_bitmap, quote_row_width, BitmapLayout, DecodedBitmap, FieldSelection};
use crate::mac::models::{BarRecord, FieldContext, MacAdjust, MacCode, MacInstrumentId, MacPeriod, MacMarketId, QuoteField, QuoteRow, SlotValue, TransactionPrice, TransactionSlot, WireFloat};
use crate::{Error, ErrorKind, Result};

pub use super::layout::{HEAD_FLAG as MAC_HEAD_FLAG,STANDARD_HEAD_FLAG,
    REQUEST_HEADER_SIZE as MAC_REQUEST_HEADER_SIZE,RESPONSE_HEADER_SIZE as MAC_RESPONSE_HEADER_SIZE};
pub const MAC_EX_LOGIN_COMMAND: u16 = 0x2454;

fn invalid(message: impl Into<String>) -> Error { Error::new(ErrorKind::InvalidArgument,message).with_context("domain","mac") }
fn protocol(message: impl Into<String>) -> Error { Error::new(ErrorKind::Protocol,message).with_context("domain","mac") }
fn put_u16(out:&mut Vec<u8>,value:u16){out.extend_from_slice(&value.to_le_bytes());}
fn put_u32(out:&mut Vec<u8>,value:u32){out.extend_from_slice(&value.to_le_bytes());}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MacRequest { pub head_flag:u8, pub customize:u32, pub version:u8, pub command:u16, pub body:Vec<u8>, pub wire:Vec<u8> }
pub fn build_request(command:u16,body:&[u8],head_flag:u8)->Result<MacRequest>{
    let inner_len=2usize.checked_add(body.len()).ok_or_else(||invalid("MAC request length overflow"))?;
    let len=u16::try_from(inner_len).map_err(|_|invalid("MAC request body exceeds u16 length"))?;
    let mut wire=Vec::with_capacity(MAC_REQUEST_HEADER_SIZE+inner_len);
    wire.push(head_flag);put_u32(&mut wire,0);wire.push(1);put_u16(&mut wire,len);put_u16(&mut wire,len);put_u16(&mut wire,command);wire.extend_from_slice(body);
    Ok(MacRequest{head_flag,customize:0,version:1,command,body:body.to_vec(),wire})
}
pub fn mac_ex_login_request()->Result<MacRequest>{
    const LOGIN_BODY:[u8;80]=[
        0xe5,0xbb,0x1c,0x2f,0xaf,0xe5,0x25,0x94,0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,
        0x5b,0x73,0x4c,0xc9,0xcd,0xbf,0x0a,0xc9,0x20,0x21,0xbf,0xdd,0x1e,0xb0,0x6d,0x22,
        0xd0,0x08,0x88,0x4c,0x16,0x11,0xcb,0x13,0x78,0xf6,0xab,0xd8,0x24,0xd8,0x99,0xd2,
        0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,
        0xa9,0x32,0x5a,0xc9,0x35,0xdc,0x08,0x37,0x33,0x5a,0x16,0xe4,0xce,0x17,0xc1,0xbb,
    ];
    build_request(MAC_EX_LOGIN_COMMAND,&LOGIN_BODY,0x01)
}

/// MAC-EX's EX-wire adapter rewrites only the first byte of source MAC packets.
pub fn rewrite_mac_ex_head(packet:&[u8])->Result<Vec<u8>>{
    let Some(first)=packet.first() else{return Err(invalid("empty MAC-EX packet"));};
    let mut rewritten=packet.to_vec();
    if *first==MAC_HEAD_FLAG { rewritten[0]=0x01; }
    Ok(rewritten)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MacResponseHeader { pub magic:u32,pub sequence:u32,pub method:u32,pub zip_size:u16,pub unzip_size:u16 }
impl MacResponseHeader {
    pub fn parse(bytes:&[u8])->Result<Self>{
        if bytes.len()<MAC_RESPONSE_HEADER_SIZE{return Err(protocol("truncated MAC 16-byte response header"));}
        Ok(Self{magic:u32::from_le_bytes(bytes[0..4].try_into().map_err(|_|protocol("MAC header magic"))?),sequence:u32::from_le_bytes(bytes[4..8].try_into().map_err(|_|protocol("MAC header sequence"))?),method:u32::from_le_bytes(bytes[8..12].try_into().map_err(|_|protocol("MAC header method"))?),zip_size:u16::from_le_bytes(bytes[12..14].try_into().map_err(|_|protocol("MAC header zip size"))?),unzip_size:u16::from_le_bytes(bytes[14..16].try_into().map_err(|_|protocol("MAC header unzip size"))?)})
    }
    pub fn validate_wire_body(&self,raw:&[u8])->Result<()> { if raw.len()!=usize::from(self.zip_size){Err(protocol("MAC response compressed length differs from 16-byte header"))}else{Ok(())} }
}

pub fn quote_request(ids:&[MacInstrumentId],fields:&FieldSelection)->Result<MacRequest>{
    if ids.is_empty()||ids.len()>80{return Err(invalid("MAC quote request requires 1..=80 identities"));}
    let mut body=encode_bitmap(fields,BitmapLayout::Quote20)?;put_u16(&mut body,u16::try_from(ids.len()).map_err(|_|invalid("MAC quote count exceeds u16"))?);
    for id in ids { put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes()); }
    build_request(0x122b,&body,MAC_HEAD_FLAG)
}
pub fn board_members_request(board_code:u32,sort_type:u16,start:u32,count:u16,sort_order:u8,fields:&FieldSelection,exclude_mask:u8)->Result<MacRequest>{
    let mut body=Vec::with_capacity(58);put_u32(&mut body,board_code);body.extend_from_slice(&[0;9]);put_u16(&mut body,sort_type);put_u32(&mut body,start);put_u16(&mut body,count);body.push(sort_order);body.push(0);
    body.extend_from_slice(&encode_bitmap(fields,BitmapLayout::Board16)?);body.extend_from_slice(&[0,exclude_mask,0,1]);build_request(0x122c,&body,MAC_HEAD_FLAG)
}
pub fn bars_request(id:&MacInstrumentId,period:MacPeriod,times:u16,start:u32,count:u16,adjust:MacAdjust)->Result<MacRequest>{
    let mut body=Vec::with_capacity(46);put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes());put_u16(&mut body,period.wire());put_u16(&mut body,times);put_u32(&mut body,start);put_u16(&mut body,count);put_u16(&mut body,adjust.wire());body.extend_from_slice(&super::layout::BARS_FLAGS);put_u16(&mut body,0);body.extend_from_slice(&[0;4]);build_request(0x122e,&body,MAC_HEAD_FLAG)
}
pub fn tick_chart_request(id:&MacInstrumentId,date_yyyymmdd:Option<u32>)->Result<MacRequest>{
    let mut body=Vec::with_capacity(38);put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes());put_u32(&mut body,date_yyyymmdd.unwrap_or(0));body.extend_from_slice(&[1,0,0,0,0,0,0,0,0,0]);build_request(0x122d,&body,MAC_HEAD_FLAG)
}
pub fn tick_charts_request(id:&MacInstrumentId,start_date_yyyymmdd:Option<u32>,days:u16)->Result<MacRequest>{
    if !(1..=5).contains(&days){return Err(invalid("MAC multi-day tick query requires 1..=5 days"));}
    let mut body=Vec::with_capacity(36);put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes());put_u32(&mut body,start_date_yyyymmdd.unwrap_or(0));put_u16(&mut body,days);put_u16(&mut body,1);body.extend_from_slice(&[0;6]);build_request(0x123e,&body,MAC_HEAD_FLAG)
}
pub fn transactions_request(id:&MacInstrumentId,date_yyyymmdd:Option<u32>,start:u32,count:u16)->Result<MacRequest>{
    let mut body=Vec::with_capacity(46);put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes());put_u32(&mut body,date_yyyymmdd.unwrap_or(0));put_u32(&mut body,start);put_u16(&mut body,count);body.extend_from_slice(&[0;10]);build_request(0x122f,&body,MAC_HEAD_FLAG)
}
pub fn symbol_info_request(id:&MacInstrumentId)->Result<MacRequest>{let mut body=Vec::with_capacity(40);put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes());put_u32(&mut body,1);body.extend_from_slice(&[0;12]);build_request(0x122a,&body,MAC_HEAD_FLAG)}
pub fn board_list_request(board_type:u16,start:u16,page_size:u16)->Result<MacRequest>{let mut body=Vec::with_capacity(20);put_u16(&mut body,page_size);put_u16(&mut body,board_type);body.extend_from_slice(&[0,0]);put_u16(&mut body,start);put_u16(&mut body,1);body.extend_from_slice(&[0;8]);build_request(0x1231,&body,MAC_HEAD_FLAG)}
fn request_1218(market:MacMarketId,code:&MacCode,tag:&[u8;10],head_flag:u8)->Result<MacRequest>{if code.encoded().len()>8{return Err(invalid("MAC 0x1218 code exceeds its source 8-byte request field"));}let mut body=Vec::with_capacity(47);put_u16(&mut body,market.get());let mut short=[0;8];short[..code.encoded().len()].copy_from_slice(code.encoded());body.extend_from_slice(&short);body.extend_from_slice(&[0;16]);body.extend_from_slice(tag);body.extend_from_slice(&[0;11]);build_request(0x1218,&body,head_flag)}
pub fn belonging_board_request(market:MacMarketId,code:&MacCode)->Result<MacRequest>{request_1218(market,code,super::layout::BELONG_TAG,1)}
pub fn capital_flow_request(market:MacMarketId,code:&MacCode)->Result<MacRequest>{request_1218(market,code,super::layout::CAPITAL_TAG,2)}
pub fn auction_request(id:&MacInstrumentId,start:u32,count:u32)->Result<MacRequest>{let mut body=Vec::with_capacity(48);put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes());put_u32(&mut body,start);put_u32(&mut body,count);body.extend_from_slice(&[0;10]);build_request(0x123d,&body,MAC_HEAD_FLAG)}
pub fn unusual_request(market:MacMarketId,start:u16,count:u16)->Result<MacRequest>{let mut body=Vec::with_capacity(20);put_u16(&mut body,market.get());put_u16(&mut body,start);body.extend_from_slice(&[0;2]);put_u16(&mut body,count);body.extend_from_slice(&[0;2]);for value in [1_u16,200,30,40,50,200]{put_u16(&mut body,value);}build_request(0x1237,&body,MAC_HEAD_FLAG)}
pub fn server_info_request()->Result<MacRequest>{let mut body=super::layout::SERVER_PREFIX.to_vec();body.extend_from_slice(&[0;8]);body.extend_from_slice(&super::layout::SERVER_MARKER);body.extend_from_slice(&[0;52]);build_request(0x120f,&body,MAC_HEAD_FLAG)}
pub fn kline_offsets_request(offset:u32,count:u32)->Result<MacRequest>{let mut body=Vec::with_capacity(13);put_u32(&mut body,offset);put_u32(&mut body,count);body.extend_from_slice(&[0;5]);build_request(0x124a,&body,MAC_HEAD_FLAG)}
pub fn chart_sampling_request(id:&MacInstrumentId)->Result<MacRequest>{let mut body=Vec::with_capacity(40);put_u16(&mut body,id.market().get());body.extend_from_slice(&id.code().wire_bytes());put_u16(&mut body,1);put_u16(&mut body,20);body.extend_from_slice(&[0;9]);build_request(0x254d,&body,MAC_HEAD_FLAG)}
pub fn goods_request(market:MacMarketId,start:u32,count:u32)->Result<MacRequest>{if count>1000{return Err(invalid("MAC goods page count exceeds 1000"));}let mut body=Vec::with_capacity(10);put_u16(&mut body,market.get());put_u32(&mut body,start);put_u32(&mut body,count);build_request(0x2562,&body,MAC_HEAD_FLAG)}
fn file_name_wire(name:&str)->Result<[u8;70]>{Ok(crate::mac::models::RemoteFileName::new(name)?.wire_bytes())}
pub fn file_meta_request(name:&str,offset:u32)->Result<MacRequest>{let mut body=Vec::with_capacity(104);put_u32(&mut body,offset);body.extend_from_slice(&file_name_wire(name)?);body.extend_from_slice(&[0;30]);build_request(0x1215,&body,MAC_HEAD_FLAG)}
pub fn download_chunk_request(name:&str,index:u32,offset:u32,size:u32)->Result<MacRequest>{if size==0||size>30000{return Err(invalid("MAC file chunk size must be in 1..=30000"));}let mut body=Vec::with_capacity(112);put_u32(&mut body,index);put_u32(&mut body,offset);put_u32(&mut body,size);body.extend_from_slice(&file_name_wire(name)?);body.extend_from_slice(&[0;30]);build_request(0x1217,&body,MAC_HEAD_FLAG)}

pub fn decode_quote_rows(body:&[u8],context:FieldContext)->Result<(u32,Vec<QuoteRow>,DecodedBitmap)>{
    if body.len()<26{return Err(protocol("truncated MAC quote response bitmap/count header"));}
    let bitmap=decode_bitmap(&body[..20])?;let total=u32::from_le_bytes(body[20..24].try_into().map_err(|_|protocol("quote total"))?);let count=u16::from_le_bytes(body[24..26].try_into().map_err(|_|protocol("quote row count"))?);
    let width=quote_row_width(&bitmap)?;let expected=26usize.checked_add(width.checked_mul(usize::from(count)).ok_or_else(||protocol("MAC quote row count overflow"))?).ok_or_else(||protocol("MAC quote response length overflow"))?;
    if body.len()!=expected{return Err(protocol("MAC quote payload is not an exact set of complete rows").with_context("expected_bytes",format!("{expected}")).with_context("actual_bytes",format!("{}",body.len())));}
    let mut rows=Vec::with_capacity(usize::from(count));
    for index in 0..usize::from(count){let start=26+index*width;let row=&body[start..start+width];let market=MacMarketId::new(u16::from_le_bytes([row[0],row[1]]));let code_raw: [u8;22]=row[2..24].try_into().map_err(|_|protocol("MAC code row"))?;let name_raw:[u8;44]=row[24..68].try_into().map_err(|_|protocol("MAC name row"))?;let mut fields=Vec::with_capacity(bitmap.slots.len());
        for slot in &bitmap.slots {let offset=68+slot.slot*4;let raw:[u8;4]=row[offset..offset+4].try_into().map_err(|_|protocol("MAC field slot"))?;let name=crate::mac::bitmap::name_for(slot.bit,context);let derived_text=if slot.bit.get()==74 {match decode_slot(*slot,raw){SlotValue::U32(value) if value!=0=>{let width=crate::mac::routing::ah_code_width(market.get()==0||market.get()==1);Some(format!("{value:0width$}"))},_=>None}}else{None};
            fields.push(QuoteField{bit:slot.bit,name,context,value:decode_slot(*slot,raw),derived_text,meaning:crate::mac::bitmap::meaning(slot.bit)});
        }
        rows.push(QuoteRow{market,code_raw,name_raw,fields,reserved_bitmap:bitmap.reserved});
    }
    Ok((total,rows,bitmap))
}

pub fn decode_bars(body:&[u8],period:MacPeriod)->Result<Vec<BarRecord>>{
    if body.len()<33{return Err(protocol("truncated MAC 0x122E response header"));}
    let count=u16::from_le_bytes([body[27],body[28]]);let expected=33usize.checked_add(usize::from(count).checked_mul(36).ok_or_else(||protocol("MAC bar count overflow"))?).ok_or_else(||protocol("MAC bar length overflow"))?;
    if body.len()!=expected{return Err(protocol("MAC bar payload does not match declared complete rows").with_context("expected_bytes",format!("{expected}")).with_context("actual_bytes",format!("{}",body.len())));}
    let mut rows=Vec::with_capacity(usize::from(count));for index in 0..usize::from(count){let offset=33+index*36;let raw:[u8;36]=body[offset..offset+36].try_into().map_err(|_|protocol("MAC bar record"))?;let date_raw=u32::from_le_bytes(raw[0..4].try_into().map_err(|_|protocol("MAC bar date"))?);let wire_time=u32::from_le_bytes(raw[4..8].try_into().map_err(|_|protocol("MAC bar time"))?);let mut values=[WireFloat::from_bits(0);7];for (slot,value) in values.iter_mut().enumerate(){let pos=8+slot*4;*value=WireFloat::from_bits(u32::from_le_bytes(raw[pos..pos+4].try_into().map_err(|_|protocol("MAC bar value"))?));}rows.push(BarRecord{date_raw,time_seconds:if period.has_intraday_time(){wire_time}else{0},values,source_bytes:raw});}
    Ok(rows)
}

pub fn decode_transactions(body:&[u8])->Result<Vec<TransactionSlot>>{
    if body.len()<39{return Err(protocol("truncated MAC transaction header"));}let count=u16::from_le_bytes([body[29],body[30]]);let expected=39usize.checked_add(usize::from(count).checked_mul(18).ok_or_else(||protocol("MAC transaction count overflow"))?).ok_or_else(||protocol("MAC transaction length overflow"))?;
    if body.len()!=expected{return Err(protocol("MAC transaction payload does not match declared complete rows").with_context("expected_bytes",format!("{expected}")).with_context("actual_bytes",format!("{}",body.len())));}
    let mut out=Vec::with_capacity(usize::from(count));for i in 0..usize::from(count){let p=39+i*18;let raw_time=u32::from_le_bytes(body[p..p+4].try_into().map_err(|_|protocol("MAC transaction time"))?);let price=WireFloat::from_bits(u32::from_le_bytes(body[p+4..p+8].try_into().map_err(|_|protocol("MAC transaction price"))?));let volume=u32::from_le_bytes(body[p+8..p+12].try_into().map_err(|_|protocol("MAC transaction volume"))?);let trade_count=u32::from_le_bytes(body[p+12..p+16].try_into().map_err(|_|protocol("MAC transaction count field"))?);let nature=u16::from_le_bytes(body[p+16..p+18].try_into().map_err(|_|protocol("MAC transaction nature"))?);out.push(TransactionSlot{time_seconds:raw_time,price:TransactionPrice::MacF32(price),volume,trade_count,nature,trade_count_provided:true});}Ok(out)
}

pub fn decode_hk_transaction(raw_time:u32,raw_price:u32,volume:u32,nature:u16)->TransactionSlot { TransactionSlot{time_seconds:raw_time,price:TransactionPrice::HongKongScaledU32{raw:raw_price,divisor:1000},volume,trade_count:0,nature,trade_count_provided:false} }

pub fn decode_kline_offsets(body:&[u8])->Result<(u32,u32)>{if body.len()!=8{return Err(protocol("MAC kline offset response must be exactly 8 bytes"));}Ok((u32::from_be_bytes(body[..4].try_into().map_err(|_|protocol("MAC kline total"))?),u32::from_le_bytes(body[4..].try_into().map_err(|_|protocol("MAC kline returned"))?)))}

pub fn decode_bitmap_for_slots(bytes:&[u8])->Result<DecodedBitmap>{decode_bitmap(bytes)}
pub fn encode_board_bitmap(selection:&FieldSelection)->Result<Vec<u8>>{encode_bitmap(selection,BitmapLayout::Board16)}
