use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::{DEFAULT_CODE_PAGE_SIZE, MAX_CODE_PAGE_SIZE, MAX_RESPONSE_PAYLOAD_SIZE};
use crate::unit::{decode_gbk_text, little_f32, little_u16, Market};

pub const TYPE_SECURITY_LIST: u16 = 0x044d;
pub const TYPE_SECURITY_COUNT: u16 = 0x044e;
pub const CODE_RECORD_SIZE: usize = 37;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecurityCountRequest {
    pub market: Market,
    pub client_date: u32,
}

impl SecurityCountRequest {
    pub fn frame(self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(6);
        data.extend_from_slice(&u16::from(self.market.id()).to_le_bytes());
        data.extend_from_slice(&self.client_date.to_le_bytes());
        RequestFrame::new(msg_id, TYPE_SECURITY_COUNT, data)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecurityListRequest {
    pub market: Market,
    pub start: u32,
    pub limit: u16,
}

impl SecurityListRequest {
    pub fn new(market: Market, start: u32, limit: u16) -> Result<Self, ProtocolError> {
        if limit > MAX_CODE_PAGE_SIZE {
            return Err(ProtocolError::invalid_argument(
                "limit",
                format!("limit must be between 0 and {MAX_CODE_PAGE_SIZE}"),
            ));
        }
        Ok(Self {
            market,
            start,
            limit,
        })
    }

    pub fn first_page(market: Market) -> Self {
        Self {
            market,
            start: 0,
            limit: DEFAULT_CODE_PAGE_SIZE,
        }
    }

    pub fn frame(self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(14);
        data.extend_from_slice(&u16::from(self.market.id()).to_le_bytes());
        data.extend_from_slice(&self.start.to_le_bytes());
        data.extend_from_slice(&u32::from(self.limit).to_le_bytes());
        data.extend_from_slice(&[0_u8; 4]);
        RequestFrame::new(msg_id, TYPE_SECURITY_LIST, data)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecurityCategory {
    Index,
    Etf,
    AShare,
    BShare,
    PrivateConvertibleBond,
    Bond,
    Unknown,
}

impl SecurityCategory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Index => "index",
            Self::Etf => "etf",
            Self::AShare => "a_share",
            Self::BShare => "b_share",
            Self::PrivateConvertibleBond => "private_convertible_bond",
            Self::Bond => "bond",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecurityBoard {
    SseMainBoard,
    SseStarMarket,
    SzseMainBoard,
    SzseChinext,
    BseListedStock,
    None,
}

impl SecurityBoard {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SseMainBoard => "sse_main_board",
            Self::SseStarMarket => "sse_star_market",
            Self::SzseMainBoard => "szse_main_board",
            Self::SzseChinext => "szse_chinext",
            Self::BseListedStock => "bse_listed_stock",
            Self::None => "none",
        }
    }

    pub const fn reason(self) -> &'static str {
        match self {
            Self::SseMainBoard => "SSE main-board prefix",
            Self::SseStarMarket => "SSE STAR Market prefix",
            Self::SzseMainBoard => "SZSE main-board prefix",
            Self::SzseChinext => "SZSE ChiNext prefix",
            Self::BseListedStock => "BSE listed stock prefix",
            Self::None => "no stock board matched",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SecurityCode {
    pub market: Market,
    pub code: String,
    pub name: String,
    pub multiple: u16,
    pub decimal: u8,
    pub previous_close_price: f32,
    pub volume_ratio_base: f32,
    pub unknown0_raw: [u8; 4],
    pub previous_close_raw: [u8; 4],
    pub unknown3_raw: [u8; 4],
    pub category: SecurityCategory,
    pub category_reason: &'static str,
    pub board: SecurityBoard,
}

impl SecurityCode {
    pub fn full_code(&self) -> String {
        let mut value = String::with_capacity(8);
        value.push_str(self.market.as_str());
        value.push_str(&self.code);
        value
    }

    pub const fn category_reason(&self) -> &'static str {
        self.category_reason
    }

    pub fn board_reason(&self) -> &'static str {
        if self.category != SecurityCategory::AShare {
            "not an A-share stock"
        } else {
            self.board.reason()
        }
    }
}

pub fn parse_security_count_payload(payload: &[u8]) -> Result<u16, ProtocolError> {
    ensure_payload_bound(payload, "security count")?;
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "security count",
            "invalid security count payload",
        ));
    }
    little_u16(&payload[..2])
}

pub fn parse_security_list_payload(
    payload: &[u8],
    market: Market,
) -> Result<Vec<SecurityCode>, ProtocolError> {
    ensure_payload_bound(payload, "security list")?;
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "security list",
            "invalid security list payload",
        ));
    }
    let count = usize::from(little_u16(&payload[..2])?);
    let records_size = count.checked_mul(CODE_RECORD_SIZE).ok_or_else(|| {
        ProtocolError::invalid_data("security list", "security list length overflow")
    })?;
    let expected_length = 2_usize.checked_add(records_size).ok_or_else(|| {
        ProtocolError::invalid_data("security list", "security list length overflow")
    })?;
    if payload.len() < expected_length {
        return Err(ProtocolError::invalid_data(
            "security list",
            "truncated security list payload",
        ));
    }

    let mut items = Vec::with_capacity(count);
    for record in payload[2..expected_length].chunks_exact(CODE_RECORD_SIZE) {
        let code = decode_security_code(&record[..6])?;
        let unknown0_raw = exact_four(&record[24..28], "security unknown0")?;
        let previous_close_raw = exact_four(&record[29..33], "security previous close")?;
        let unknown3_raw = exact_four(&record[33..37], "security unknown3")?;
        let (category, category_reason) = classify_security_details(market, &code);
        let board = classify_board(market, &code, category);
        items.push(SecurityCode {
            market,
            code,
            name: decode_gbk_text(&record[8..24]),
            multiple: little_u16(&record[6..8])?,
            decimal: record[28],
            previous_close_price: little_f32(&previous_close_raw)?,
            volume_ratio_base: little_f32(&unknown0_raw)?,
            unknown0_raw,
            previous_close_raw,
            unknown3_raw,
            category,
            category_reason,
            board,
        });
    }
    Ok(items)
}

pub fn classify_security(market: Market, code: &str) -> SecurityCategory {
    classify_security_details(market, code).0
}

fn classify_security_details(market: Market, code: &str) -> (SecurityCategory, &'static str) {
    let prefix = |value: &str| code.starts_with(value);
    if (market == Market::Shanghai
        && ["000", "880", "881", "999"]
            .iter()
            .any(|value| prefix(value)))
        || (market == Market::Shenzhen && prefix("399"))
        || (market == Market::Beijing && prefix("899"))
    {
        return (SecurityCategory::Index, "index code prefix");
    }
    if (market == Market::Shanghai
        && [
            "510", "511", "512", "513", "515", "516", "517", "518", "520", "560", "561", "562",
            "563", "588",
        ]
        .iter()
        .any(|value| prefix(value)))
        || (market == Market::Shenzhen && ["158", "159"].iter().any(|value| prefix(value)))
    {
        return (SecurityCategory::Etf, "ETF code prefix");
    }
    if market == Market::Shanghai
        && ["600", "601", "603", "605", "688", "689"]
            .iter()
            .any(|value| prefix(value))
    {
        return (SecurityCategory::AShare, "SSE A-share code prefix");
    }
    if market == Market::Shenzhen
        && ["000", "001", "002", "003", "004", "300", "301"]
            .iter()
            .any(|value| prefix(value))
    {
        return (SecurityCategory::AShare, "SZSE A-share code prefix");
    }
    if market == Market::Beijing && prefix("92") {
        return (SecurityCategory::AShare, "BSE listed stock code prefix");
    }
    let shenzhen_b_share = market == Market::Shenzhen
        && prefix("20")
        && matches!(code.as_bytes().get(2), Some(byte) if byte.is_ascii_digit());
    if (market == Market::Shanghai && prefix("900")) || shenzhen_b_share {
        return (SecurityCategory::BShare, "B-share code prefix");
    }
    if market == Market::Beijing && prefix("810") {
        return (
            SecurityCategory::PrivateConvertibleBond,
            "BSE private convertible bond prefix",
        );
    }
    if market == Market::Beijing && prefix("821") {
        return (SecurityCategory::Bond, "BSE bond sample prefix");
    }
    (SecurityCategory::Unknown, "no matched code prefix")
}

pub fn classify_board(market: Market, code: &str, category: SecurityCategory) -> SecurityBoard {
    if category != SecurityCategory::AShare {
        return SecurityBoard::None;
    }
    let prefix = |value: &str| code.starts_with(value);
    if market == Market::Shanghai
        && ["600", "601", "603", "605"]
            .iter()
            .any(|value| prefix(value))
    {
        return SecurityBoard::SseMainBoard;
    }
    if market == Market::Shanghai && ["688", "689"].iter().any(|value| prefix(value)) {
        return SecurityBoard::SseStarMarket;
    }
    if market == Market::Shenzhen
        && ["000", "001", "002", "003", "004"]
            .iter()
            .any(|value| prefix(value))
    {
        return SecurityBoard::SzseMainBoard;
    }
    if market == Market::Shenzhen && ["300", "301"].iter().any(|value| prefix(value)) {
        return SecurityBoard::SzseChinext;
    }
    if market == Market::Beijing && prefix("92") {
        return SecurityBoard::BseListedStock;
    }
    SecurityBoard::None
}

fn decode_security_code(data: &[u8]) -> Result<String, ProtocolError> {
    if data.len() != 6 || !data.is_ascii() {
        return Err(ProtocolError::invalid_data(
            "security list",
            "invalid security code",
        ));
    }
    let text = std::str::from_utf8(data)
        .map_err(|_| ProtocolError::invalid_data("security list", "invalid security code"))?;
    Ok(text.to_owned())
}

fn exact_four(data: &[u8], field: &'static str) -> Result<[u8; 4], ProtocolError> {
    data.try_into().map_err(|_| ProtocolError::LengthMismatch {
        field,
        expected: 4,
        actual: data.len(),
    })
}

fn ensure_payload_bound(payload: &[u8], context: &'static str) -> Result<(), ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: context,
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        classify_board, classify_security, parse_security_count_payload,
        parse_security_list_payload, SecurityBoard, SecurityCategory, SecurityCountRequest,
        SecurityListRequest,
    };
    use crate::unit::Market;

    #[test]
    fn security_requests_match_frozen_wire_data() {
        let count = SecurityCountRequest {
            market: Market::Shenzhen,
            client_date: 20_260_519,
        }
        .frame(3);
        assert_eq!(count.data.as_ref(), &[0x00, 0x00, 0xa7, 0x26, 0x35, 0x01]);

        let list = SecurityListRequest::first_page(Market::Beijing).frame(4);
        assert_eq!(
            list.data.as_ref(),
            &[2, 0, 0, 0, 0, 0, 0x40, 0x06, 0, 0, 0, 0, 0, 0]
        );
        assert!(SecurityListRequest::new(Market::Shenzhen, 0, 1_601).is_err());
    }

    #[test]
    fn parses_security_count_and_typed_code_record() {
        assert_eq!(parse_security_count_payload(&[0xf5, 0x5a]), Ok(23_285));

        let mut record = Vec::with_capacity(37);
        record.extend_from_slice(b"000001");
        record.extend_from_slice(&100_u16.to_le_bytes());
        record.extend_from_slice(&[0xc6, 0xbd, 0xb0, 0xb2, 0xd2, 0xf8, 0xd0, 0xd0]);
        record.extend_from_slice(&[0_u8; 8]);
        record.extend_from_slice(&3_956.656_5_f32.to_le_bytes());
        record.push(2);
        record.extend_from_slice(&10.99_f32.to_le_bytes());
        record.extend_from_slice(&[0x67, 0x31, 0x68, 0x25]);
        let mut payload = vec![1, 0];
        payload.extend_from_slice(&record);

        let parsed = parse_security_list_payload(&payload, Market::Shenzhen);
        assert!(matches!(
            parsed,
            Ok(items)
                if items.len() == 1
                    && items[0].full_code() == "sz000001"
                    && items[0].name == "平安银行"
                    && items[0].category == SecurityCategory::AShare
                    && items[0].category_reason == "SZSE A-share code prefix"
                    && items[0].board == SecurityBoard::SzseMainBoard
                    && items[0].unknown3_raw == [0x67, 0x31, 0x68, 0x25]
        ));
    }

    #[test]
    fn classifies_all_frozen_security_categories_and_boards() {
        assert_eq!(
            classify_security(Market::Shanghai, "000001"),
            SecurityCategory::Index
        );
        assert_eq!(
            classify_security(Market::Shanghai, "510300"),
            SecurityCategory::Etf
        );
        assert_eq!(
            classify_security(Market::Beijing, "920001"),
            SecurityCategory::AShare
        );
        assert_eq!(
            classify_board(Market::Shanghai, "688001", SecurityCategory::AShare),
            SecurityBoard::SseStarMarket
        );
        assert_eq!(
            classify_security(Market::Beijing, "810001"),
            SecurityCategory::PrivateConvertibleBond
        );
    }

    #[test]
    fn rejects_truncated_security_records() {
        assert!(parse_security_list_payload(&[1, 0, b'0'], Market::Shenzhen).is_err());
    }
}
