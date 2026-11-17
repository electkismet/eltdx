//! Independent source-contract fixtures; authored, not executed. The field
//! names and preset memberships below are transcribed from Python FieldBit /
//! PresetField source, rather than derived from the Rust schema under test.
#![cfg(feature = "mac")]
use eltdx::mac::{bitmap, commands, routing, FieldId, FieldSelection, PresetField};
use eltdx::mac::models::{FieldContext, MacAdjust, MacCode, MacExMarketId, MacExRoute, MacInstrumentId, MacMarketId, MacPeriod, SlotKind, SlotValue};

const SOURCE_FIELDS: &str = "
0:PRE_CLOSE 1:OPEN 2:HIGH 3:LOW 4:CLOSE 5:VOL 6:VOL_RATIO 7:AMOUNT
8:INSIDE_VOLUME 9:OUTSIDE_VOLUME 10:TOTAL_SHARES 11:FLOAT_SHARES 12:EPS 13:NET_ASSETS 14:SECURITY_TYPE_PRICE 15:TOTAL_MARKET_CAP_AB
16:PE_DYNAMIC 17:BID_PRICE 18:ASK_PRICE 19:SERVER_UPDATE_DATE 20:SERVER_UPDATE_TIME 21:LOT_SIZE_INFO 22:BOARD_STRENGTH 23:DIVIDEND_YIELD
24:BID_VOLUME 25:ASK_VOLUME 26:LAST_VOLUME 27:TURNOVER 28:INDUSTRY 29:INDUSTRY_CHANGE_UP 30:STOCK_TAG_FLAGS 31:DECIMAL_POINT
32:BUY_PRICE_LIMIT 33:SELL_PRICE_LIMIT 34:PRICE_DECIMAL_INFO 35:LOT_SIZE 36:PRE_IOPV 37:SPEED_PCT 38:AVG_PRICE 39:IOPV
40:PE_TTM_VOL_RELATED 41:EX_PRICE_PLACEHOLDER 42:OPERATING_REVENUE 43:FLAG_KCB 44:FLAG_BJ 45:CIRCULATING_CAPITAL_Z 46:AFTER_HOURS_VOLUME
48:PE_TTM 49:PE_STATIC 55:INDEX_METRIC 56:MAIN_NET_AMOUNT 57:BID_ASK_RATIO 58:NON_INDEX_FLAG 59:CHANGE_20D_PCT 60:YTD_PCT 62:STOCK_CLASS_CODE 63:PERCENT_BASE
64:MTD_PCT 65:CHANGE_1Y_PCT 66:PREV_CHANGE_PCT 67:CHANGE_3D_PCT 68:CHANGE_60D_PCT 69:CHANGE_5D_PCT 70:CHANGE_10D_PCT 71:PREV2_CHANGE_PCT
72:BID2_PRICE 73:ASK2_PRICE 74:AH_CODE 75:UNKNOWN_CODE 87:OPEN_AMOUNT 88:ANNUAL_LIMIT_UP_DAYS 89:ACTIVITY 91:DIVIDEND_YIELD_RATE 92:CONSECUTIVE_UP_DAYS
93:BID2_VOLUME 94:ASK2_VOLUME 95:INDUSTRY_SUB 102:AUCTION_BUY_LIMIT 103:AUCTION_SELL_LIMIT 104:VOL_SPEED_PCT 105:SHORT_TURNOVER_PCT
106:AMOUNT_2M 107:MAIN_NET_AMOUNT_COPY 108:MAIN_NET_RATIO 109:RETAIL_NET_AMOUNT 110:MAIN_NET_5M_AMOUNT 111:MAIN_NET_3D_AMOUNT
112:MAIN_NET_5D_AMOUNT 113:MAIN_NET_10D_AMOUNT 114:MAIN_BUY_NET_AMOUNT 115:DDX 116:DDY 117:DDZ 118:DDF 119:STOCK_FLAG_A 120:STOCK_FLAG_B
122:AUCTION_VOL_RATIO 123:PREV_AMOUNT 125:RECENT_INDICATOR 128:BID3_PRICE 129:BID4_PRICE 130:BID5_PRICE 131:ASK3_PRICE 132:ASK4_PRICE 133:ASK5_PRICE
134:BID3_VOLUME 135:BID4_VOLUME 136:BID5_VOLUME 137:ASK3_VOLUME 138:ASK4_VOLUME 139:ASK5_VOLUME 140:BID_ASK_DIFF 141:CHANGE_UP_TYPE
142:SAFETY_SCORE 143:HIGHLIGHT_COUNT 144:CHANGE_AT_1000 145:CHANGE_AT_1030 146:CHANGE_AT_1100 147:CHANGE_AT_1130 148:CHANGE_AT_1330 149:CHANGE_AT_1400 150:CHANGE_AT_1430
";
fn source_fields() -> Vec<(u8,&'static str)> {
    SOURCE_FIELDS.split_whitespace().map(|value| { let (id,name) = value.split_once(':').unwrap(); (id.parse().unwrap(),name) }).collect()
}

#[test]
fn every_python_named_bit_alias_and_wire_kind_has_one_canonical_slot() {
    let source = source_fields();
    let unsigned = [5,8,9,19,20,21,24,25,26,28,30,31,34,35,43,44,58,62,63,74,75,89,93,94,95,134,135,136,137,138,139];
    let signed = [46,88,92,140,141];
    assert_eq!(bitmap::FIELDS.len(),source.len()+4);
    for raw in 0..=150 {
        let id = FieldId::new(raw).unwrap(); let expected = source.iter().find(|(value,_)| *value == raw);
        match expected {
            Some((_,name)) => {
                let field = bitmap::definition(id).expect("source named field"); assert_eq!(field.name,*name);
                let kind = if unsigned.contains(&raw) { SlotKind::U32 } else if signed.contains(&raw) { SlotKind::I32 } else { SlotKind::F32 };
                assert_eq!(field.kind,kind,"{name}");
            }
            None => assert!(bitmap::definition(id).is_none(),"unknown legal bit {raw} must stay unknown"),
        }
    }
    for (raw,name) in [(93,"LIMIT_UP_COUNT"),(94,"LIMIT_DOWN_COUNT"),(136,"UP_COUNT"),(139,"DOWN_COUNT")] {
        let id = FieldId::new(raw).unwrap(); assert_eq!(bitmap::name_for(id,FieldContext::Board),Some(name));
        assert!(bitmap::FIELDS.iter().any(|field| field.id == id && field.name == name && field.kind == SlotKind::U32));
    }
}

#[test]
fn every_named_preset_retains_the_python_field_membership_and_wire_order() {
    let presets: &[(PresetField,&[u8])] = &[
        (PresetField::None,&[]),(PresetField::Ohlc,&[1,2,3,4]),(PresetField::Basic,&[0,1,2,3,4,5]),
        (PresetField::Quote,&[17,18,24,25,26]),(PresetField::Volume,&[5,6,7,27]),(PresetField::Fundamental,&[10,11,12,13]),
        (PresetField::Enhanced,&[1,2,3,4,5,11,89]),(PresetField::AhCodeFields,&[1,2,3,4,5,28,35,74]),
        (PresetField::BoardStats,&[93,94,136,139]),
        (PresetField::Handicap,&[17,18,24,25,72,73,93,94,128,129,130,131,132,133,134,135,136,137,138,139]),
        (PresetField::Common,&[0,1,2,3,4,5,6,7,10,11,12,13,14,15,16,21,23,26,27,30,31,32,33,34,35,36,37,43,45,48,49,56,104,105]),
    ];
    for &(preset,expected) in presets { assert_eq!(FieldSelection::preset(preset).ids().iter().map(|id| id.get()).collect::<Vec<_>>(),expected); }
    assert_eq!(FieldSelection::preset(PresetField::All).ids().iter().map(|id| id.get()).collect::<Vec<_>>(),
        source_fields().iter().map(|(id,_)| *id).collect::<Vec<_>>());
    let debug = FieldSelection::preset(PresetField::Debug); assert!(debug.is_debug_all());
    assert!(bitmap::encode_bitmap(&debug,bitmap::BitmapLayout::Quote20).is_err());
    assert_eq!(routing::default_quote_selection().ids().iter().map(|id| id.get()).collect::<Vec<_>>(),[0,1,2,3,4,5,7]);
}

#[test]
fn every_ex_market_routes_only_the_six_hk_stock_sets_to_ordinary_ex() {
    for raw in 0..=255_u8 {
        let expected = if [27,31,48,49,71,98].contains(&raw) { MacExRoute::ExHongKongTransactions } else { MacExRoute::MacCommand };
        assert_eq!(routing::transaction_route(MacExMarketId::new(raw)),expected,"source market {raw}");
    }
    assert_eq!(routing::hk_price_from_wire(12345),12.345);
}

#[test]
fn ah_code_projection_uses_response_market_without_changing_the_raw_integer() {
    for (market,expected) in [(0,"00123"),(1,"00123"),(2,"000123"),(31,"000123"),(71,"000123")] {
        let mut body = vec![0;98]; body[9] = 4; body[20..24].copy_from_slice(&1_u32.to_le_bytes()); body[24..26].copy_from_slice(&1_u16.to_le_bytes());
        body[26..28].copy_from_slice(&u16::to_le_bytes(market)); body[28..33].copy_from_slice(b"00700"); body[94..98].copy_from_slice(&123_u32.to_le_bytes());
        let (_,rows,_) = commands::decode_quote_rows(&body,FieldContext::Instrument).unwrap();
        assert_eq!(rows[0].fields[0].value,SlotValue::U32(123)); assert_eq!(rows[0].fields[0].derived_text.as_deref(),Some(expected));
        assert_eq!(&rows[0].code_raw[..5],b"00700");
    }
}

#[test]
fn bar_adjustment_and_all_four_request_flags_survive_mac_ex_head_rewrite() {
    let id = MacInstrumentId::new(MacMarketId::new(47),MacCode::new("IFL0").unwrap());
    for (adjust,raw) in [(MacAdjust::None,0_u16),(MacAdjust::Qfq,1),(MacAdjust::Hfq,2)] {
        let request = commands::bars_request(&id,MacPeriod::Seconds,3,5,7,adjust).unwrap();
        assert_eq!(request.body.len(),46); assert_eq!(&request.body[24..28],&[13,0,3,0]);
        assert_eq!(&request.body[32..34],&7_u16.to_le_bytes()); assert_eq!(&request.body[34..36],&raw.to_le_bytes());
        assert_eq!(&request.body[36..],&[1,1,0,1,0,0,0,0,0,0]);
        let mapped = commands::rewrite_mac_ex_head(&request.wire).unwrap(); assert_eq!(mapped[0],1); assert_eq!(&mapped[1..],&request.wire[1..]);
    }
}

#[test]
fn shared_bit_scan_keeps_unknown_positions_and_caller_limit() {
    let mut source = [0; 51];
    for bit in [1,47,48,150,151,159,400] { source[bit/8] |= 1 << (bit%8); }
    for (limit,expected) in [(151,&[1,47,48,150][..]),(401,&[1,47,48,150,151,159,400][..]),(0,&[][..])] {
        let mut cursor = 0; let mut result = Vec::new();
        while let Some(bit) = bitmap::next_set_bit(&source,&mut cursor,limit) { result.push(bit); }
        assert_eq!(result,expected);
    }
    let mut cursor = 0;
    assert_eq!(bitmap::next_set_bit(&[],&mut cursor,usize::MAX),None);
    let mut cursor = usize::MAX;
    assert_eq!(bitmap::next_set_bit(&source,&mut cursor,usize::MAX),None);
    let mut destination = [0;20];
    assert!(bitmap::write_bit(&mut destination,159));
    assert_eq!(destination[19],128);
    assert!(!bitmap::write_bit(&mut destination,160));
    assert_eq!(destination[19],128);
}

#[test]
fn shared_board_plan_preserves_decision_short_circuit_and_error() {
    use eltdx::mac::enums::{board_code_plan, BoardCodePlan};
    let mut seen = Vec::new();
    let plan = board_code_plan::<&str>(|prefix| { seen.push(prefix.to_owned()); Ok(prefix == "US") },
        || Err("len must not be read for US")).unwrap();
    assert_eq!(plan,BoardCodePlan::Prefix { skip: 2,add: 30000 });
    assert_eq!(seen,["US"]);
    seen.clear();
    assert_eq!(board_code_plan::<&str>(|prefix| { seen.push(prefix.to_owned()); Ok(prefix == "899") },|| Ok(true)).unwrap(),
        BoardCodePlan::Full { subtract: 899000,add: 32000 });
    assert_eq!(seen,["US","HK","88","399","899"]);
    assert_eq!(board_code_plan::<&str>(|_| Ok(false),|| Err("custom length failure")),Err("custom length failure"));
}

#[test]
fn shared_board_aggregate_registry_keeps_source_bits_and_legacy_names() {
    use eltdx::mac::python_legacy::BOARD_MONEY_AGGREGATES;
    assert_eq!(BOARD_MONEY_AGGREGATES,[(7,"amount","amount"),(56,"main_net_amount","main_net_amount"),
        (111,"main_net_3d_amount","main_net_3d"),(112,"main_net_5d_amount","main_net_5d")]);
    for (bit,_,_) in BOARD_MONEY_AGGREGATES {
        let field = bitmap::definition(FieldId::new(bit).unwrap()).unwrap();
        assert_eq!(field.kind,SlotKind::F32);
    }
}
