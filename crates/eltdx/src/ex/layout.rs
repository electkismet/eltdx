//! Source request headers shared by the independent codecs and legacy bridge.
//! No Python or transport dependency is present in this registry.
pub const MARKETS: [u8;12] = [1,2,0x48,0x69,0,1,2,0,2,0,0xf4,0x23];
pub const COUNT: [u8;12] = [1,3,0x48,0x66,0,1,2,0,2,0,0xf0,0x23];
pub const INFO: [u8;12] = [1,4,0x48,0x67,0,1,8,0,8,0,0xf5,0x23];
pub const QUOTE: [u8;12] = [1,1,8,2,2,1,12,0,12,0,0xfa,0x23];
pub const QUOTES: [u8;12] = [1,0xc1,6,0x0b,0,2,11,0,11,0,0,0x24];
pub const BARS: [u8;12] = [1,1,8,0x6a,1,1,22,0,22,0,0xff,0x23];
pub const RANGE: [u8;12] = [1,1,0x38,0x92,0,1,22,0,22,0,0x0d,0x24];
pub const MINUTE: [u8;12] = [1,7,8,0,1,1,12,0,12,0,0x0b,0x24];
pub const HISTORY_MINUTE: [u8;12] = [1,1,0x30,0,1,1,16,0,16,0,0x0c,0x24];
pub const TRANSACTIONS: [u8;12] = [1,1,8,0,3,1,18,0,18,0,0xfc,0x23];
pub const HISTORY_TRANSACTIONS: [u8;12] = [1,1,0x30,0,2,1,22,0,22,0,6,0x24];

pub const QUOTE_FORMAT: &str = "<fffffIIIIIIIIIfffffIIIIIfffffIIIII";
pub const FUTURES_FORMAT: &str = "<IfffffIIIIfIIfIfIIIIIIIIIfIIIIIIIII";
pub const HONG_KONG_FORMAT: &str = "<IfffffIfIIfIIIIfffffIIIIIfffffIIIII";
pub const MINUTE_FORMAT: &str = "<HffII";
pub const TRANSACTION_FORMAT: &str = "<HIIiH";
pub const BAR_FORMAT: &str = "<ffffIIf";
pub const RANGE_BAR_FORMAT: &str = "<HHffffIIf";

/// Source names, four-byte slots and their numeric interpretation. These are
/// shared by strict native projections and the separately versioned facade.
pub const QUOTE_FIELDS: &[(&str,u8,bool)] = &[
    ("pre_close",0,true),("open",1,true),("high",2,true),("low",3,true),("price",4,true),
    ("kaicang",5,false),("zongliang",7,false),("xianliang",8,false),("neipan",10,false),
    ("waipan",11,false),("chicang",13,false),
    ("bid1",14,true),("bid2",15,true),("bid3",16,true),("bid4",17,true),("bid5",18,true),
    ("bid_vol1",19,false),("bid_vol2",20,false),("bid_vol3",21,false),("bid_vol4",22,false),("bid_vol5",23,false),
    ("ask1",24,true),("ask2",25,true),("ask3",26,true),("ask4",27,true),("ask5",28,true),
    ("ask_vol1",29,false),("ask_vol2",30,false),("ask_vol3",31,false),("ask_vol4",32,false),("ask_vol5",33,false),
];

// The legacy futures unpack target has34 names for35 format slots. That facade
// must keep its original ValueError. The strict decoder keeps all35 raw slots
// and projects documented offsets without executing the legacy tuple bug.
pub const FUTURES_FIELDS: &[(&str,u8,bool)] = &[
    ("BiShu",0,false),("ZuoJie",1,true),("JinKai",2,true),("ZuiGao",3,true),("ZuiDi",4,true),("MaiChu",5,true),
    ("KaiCang",6,false),("ZongLiang",8,false),("XianLiang",9,false),("ZongJinE",10,true),
    ("NeiPan",11,false),("WaiPan",12,false),("ChiCangLiang",14,false),("MaiRuJia",15,true),
    ("MaiRuLiang",20,false),("MaiChuJia",25,true),("MaiChuLiang",30,false),
];
pub const HONG_KONG_FIELDS: &[(&str,u8,bool)] = &[
    ("HuoYueDu",0,false),("ZuoShou",1,true),("JinKai",2,true),("ZuiGao",3,true),("ZuiDi",4,true),("XianJia",5,true),
    ("MaiRuJia",7,true),("ZongLiang",8,false),("XianLiang",9,false),("ZongJinE",10,true),("Nei",13,false),("Wai",14,false),
    ("MaiRuJia1",15,true),("MaiRuJia2",16,true),("MaiRuJia3",17,true),("MaiRuJia4",18,true),("MaiRuJia5",19,true),
    ("MaiRuLiang1",20,false),("MaiRuLiang2",21,false),("MaiRuLiang3",22,false),("MaiRuLiang4",23,false),("MaiRuLiang5",24,false),
    ("MaiChuJia1",25,true),("MaiChuJia2",26,true),("MaiChuJia3",27,true),("MaiChuJia4",28,true),("MaiChuJia5",29,true),
    ("MaiChuLiang1",30,false),("MaiChuLiang2",31,false),("MaiChuLiang3",32,false),("MaiChuLiang4",33,false),("MaiChuLiang5",34,false),
];
