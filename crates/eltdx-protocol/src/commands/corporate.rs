use bytes::Bytes;

use crate::error::ProtocolError;
use crate::frame::RequestFrame;
use crate::limits::{MAX_COMMAND_ITEMS, MAX_RESPONSE_PAYLOAD_SIZE};
use crate::unit::{little_f32, little_u16, little_u32, DateParts, Market, NormalizedCode};

pub const TYPE_CAPITAL_CHANGES: u16 = 0x000f;
pub const TYPE_FINANCE_BATCH: u16 = 0x0010;
pub const CAPITAL_CHANGE_RECORD_SIZE: usize = 29;
pub const FINANCE_RECORD_SIZE: usize = 143;
pub const FINANCE_INFO_SIZE: usize = 136;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapitalChangesRequest {
    pub code: NormalizedCode,
    pub include_raw: bool,
}

impl CapitalChangesRequest {
    pub fn new(code: NormalizedCode) -> Self {
        Self::with_include_raw(code, false)
    }

    pub const fn with_include_raw(code: NormalizedCode, include_raw: bool) -> Self {
        Self { code, include_raw }
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data = Vec::with_capacity(9);
        data.extend_from_slice(&[1, 0, self.code.market().id()]);
        data.extend_from_slice(self.code.number().as_bytes());
        RequestFrame::new(msg_id, TYPE_CAPITAL_CHANGES, data)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinanceBatchRequest {
    codes: Vec<NormalizedCode>,
    count: u16,
    include_raw: bool,
}

impl FinanceBatchRequest {
    pub fn new(codes: Vec<NormalizedCode>) -> Result<Self, ProtocolError> {
        Self::with_include_raw(codes, false)
    }

    pub fn with_include_raw(
        codes: Vec<NormalizedCode>,
        include_raw: bool,
    ) -> Result<Self, ProtocolError> {
        if codes.len() > MAX_COMMAND_ITEMS {
            return Err(ProtocolError::LimitExceeded {
                resource: "finance codes",
                actual: codes.len(),
                limit: MAX_COMMAND_ITEMS,
            });
        }
        let count = u16::try_from(codes.len())
            .map_err(|_| ProtocolError::invalid_argument("codes", "too many finance codes"))?;
        Ok(Self {
            codes,
            count,
            include_raw,
        })
    }

    pub fn codes(&self) -> &[NormalizedCode] {
        &self.codes
    }

    pub const fn count(&self) -> u16 {
        self.count
    }

    pub const fn include_raw(&self) -> bool {
        self.include_raw
    }

    pub fn frame(&self, msg_id: u32) -> RequestFrame {
        let mut data =
            Vec::with_capacity(2_usize.saturating_add(self.codes.len().saturating_mul(7)));
        data.extend_from_slice(&self.count.to_le_bytes());
        for code in &self.codes {
            data.push(code.market().id());
            data.extend_from_slice(code.number().as_bytes());
        }
        RequestFrame::new(msg_id, TYPE_FINANCE_BATCH, data)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapitalChangeRecord {
    pub market_id: u8,
    pub market: Option<Market>,
    pub code: String,
    pub reserved_7: u8,
    pub date_raw: u32,
    pub date: Option<DateParts>,
    pub category_raw: u8,
    pub category_name: Option<&'static str>,
    pub c1_raw: [u8; 4],
    pub c2_raw: [u8; 4],
    pub c3_raw: [u8; 4],
    pub c4_raw: [u8; 4],
    pub c1_float: f32,
    pub c2_float: f32,
    pub c3_float: f32,
    pub c4_float: f32,
    pub c1_value: f64,
    pub c2_value: f64,
    pub c3_value: f64,
    pub c4_value: f64,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CapitalChangeBlock {
    pub request: CapitalChangesRequest,
    pub block_count: u16,
    pub market_id: u8,
    pub market: Option<Market>,
    pub code: String,
    pub records: Vec<CapitalChangeRecord>,
    pub raw_payload: Bytes,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinanceRecord {
    pub market_id: u8,
    pub market: Option<Market>,
    pub code: String,
    pub finance_info_raw: Bytes,
    pub liu_tong_gu_ben_raw_float: f32,
    pub province_raw: u16,
    pub industry_raw: u16,
    pub updated_date_raw: u32,
    pub updated_date: Option<DateParts>,
    pub ipo_date_raw: u32,
    pub ipo_date: Option<DateParts>,
    pub zong_gu_ben_raw_float: f32,
    pub guo_jia_gu_raw_float: f32,
    pub fa_qi_ren_fa_ren_gu_raw_float: f32,
    pub fa_ren_gu_raw_float: f32,
    pub b_gu_raw_float: f32,
    pub h_gu_raw_float: f32,
    pub eps_raw: f32,
    pub zong_zi_chan_raw_float: f32,
    pub liu_dong_zi_chan_raw_float: f32,
    pub gu_ding_zi_chan_raw_float: f32,
    pub wu_xing_zi_chan_raw_float: f32,
    pub gu_dong_ren_shu_raw_float: f32,
    pub liu_dong_fu_zhai_raw_float: f32,
    pub chang_qi_fu_zhai_raw_float: f32,
    pub zi_ben_gong_ji_jin_raw_float: f32,
    pub jing_zi_chan_raw_float: f32,
    pub zhu_ying_shou_ru_raw_float: f32,
    pub zhu_ying_li_run_raw_float: f32,
    pub ying_shou_zhang_kuan_raw_float: f32,
    pub ying_ye_li_run_raw_float: f32,
    pub tou_zi_shou_yu_raw_float: f32,
    pub jing_ying_xian_jin_liu_raw_float: f32,
    pub zong_xian_jin_liu_raw_float: f32,
    pub cun_huo_raw_float: f32,
    pub li_run_zong_he_raw_float: f32,
    pub shui_hou_li_run_raw_float: f32,
    pub jing_li_run_raw_float: f32,
    pub wei_fen_li_run_raw_float: f32,
    pub mei_gu_jing_zi_chan_raw_float: f32,
    pub bao_liu_2_raw_float: f32,
    pub record_hex: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FinanceBatch {
    pub request: FinanceBatchRequest,
    pub records: Vec<FinanceRecord>,
    pub raw_payload: Bytes,
}

pub fn parse_capital_changes_payload(
    payload: &[u8],
    request: CapitalChangesRequest,
) -> Result<CapitalChangeBlock, ProtocolError> {
    check_payload(payload, "capital changes")?;
    if payload.len() < 11 {
        return Err(ProtocolError::invalid_data(
            "capital changes",
            "invalid capital changes payload",
        ));
    }
    let block_count = little_u16(&payload[..2])?;
    let market_id = payload[2];
    let market = Market::from_id(i64::from(market_id)).ok();
    let code = ascii_code(&payload[3..9], "capital changes")?;
    let record_count = usize::from(little_u16(&payload[9..11])?);
    let records_length = record_count
        .checked_mul(CAPITAL_CHANGE_RECORD_SIZE)
        .ok_or_else(|| {
            ProtocolError::invalid_data("capital changes", "capital changes length overflow")
        })?;
    let expected_length = 11_usize.checked_add(records_length).ok_or_else(|| {
        ProtocolError::invalid_data("capital changes", "capital changes length overflow")
    })?;
    if payload.len() < expected_length {
        return Err(ProtocolError::invalid_data(
            "capital changes",
            "truncated capital changes payload",
        ));
    }
    let mut records = Vec::with_capacity(record_count);
    let mut offset = 11_usize;
    for _ in 0..record_count {
        let end = offset.saturating_add(CAPITAL_CHANGE_RECORD_SIZE);
        let record = payload.get(offset..end).ok_or_else(|| {
            ProtocolError::invalid_data("capital changes", "truncated capital change record")
        })?;
        records.push(parse_capital_change_record(record)?);
        offset = end;
    }
    if offset != payload.len() {
        return Err(ProtocolError::invalid_data(
            "capital changes",
            format!(
                "unexpected trailing capital changes payload bytes: {}",
                payload.len().saturating_sub(offset)
            ),
        ));
    }
    Ok(CapitalChangeBlock {
        request,
        block_count,
        market_id,
        market,
        code,
        records,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

pub fn parse_finance_batch_payload(
    payload: &[u8],
    request: FinanceBatchRequest,
) -> Result<FinanceBatch, ProtocolError> {
    check_payload(payload, "finance batch")?;
    if payload.len() < 2 {
        return Err(ProtocolError::invalid_data(
            "finance batch",
            "invalid finance batch payload",
        ));
    }
    let record_count = usize::from(little_u16(&payload[..2])?);
    let records_length = record_count
        .checked_mul(FINANCE_RECORD_SIZE)
        .ok_or_else(|| {
            ProtocolError::invalid_data("finance batch", "finance batch length overflow")
        })?;
    let expected_length = 2_usize.checked_add(records_length).ok_or_else(|| {
        ProtocolError::invalid_data("finance batch", "finance batch length overflow")
    })?;
    if payload.len() != expected_length {
        return Err(ProtocolError::invalid_data(
            "finance batch",
            format!(
                "invalid finance batch length: expected {expected_length}, got {}",
                payload.len()
            ),
        ));
    }
    let mut records = Vec::with_capacity(record_count);
    let mut offset = 2_usize;
    for _ in 0..record_count {
        let end = offset.saturating_add(FINANCE_RECORD_SIZE);
        let record = payload.get(offset..end).ok_or_else(|| {
            ProtocolError::invalid_data("finance batch", "truncated finance record")
        })?;
        records.push(parse_finance_record(record)?);
        offset = end;
    }
    Ok(FinanceBatch {
        request,
        records,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn parse_capital_change_record(record: &[u8]) -> Result<CapitalChangeRecord, ProtocolError> {
    if record.len() != CAPITAL_CHANGE_RECORD_SIZE {
        return Err(ProtocolError::invalid_data(
            "capital changes",
            "invalid capital change record length",
        ));
    }
    let market_id = record[0];
    let code = ascii_code(&record[1..7], "capital changes")?;
    let date_raw = little_u32(&record[8..12])?;
    let category_raw = record[12];
    let c1_raw = array4(&record[13..17], "capital c1")?;
    let c2_raw = array4(&record[17..21], "capital c2")?;
    let c3_raw = array4(&record[21..25], "capital c3")?;
    let c4_raw = array4(&record[25..29], "capital c4")?;
    let c1_float = little_f32(&c1_raw)?;
    let c2_float = little_f32(&c2_raw)?;
    let c3_float = little_f32(&c3_raw)?;
    let c4_float = little_f32(&c4_raw)?;
    let (c1_value, c2_value, c3_value, c4_value) = match category_raw {
        value if is_share_count_category(value) => (
            f64::from(c1_float) * 10_000.0,
            f64::from(c2_float) * 10_000.0,
            f64::from(c3_float) * 10_000.0,
            f64::from(c4_float) * 10_000.0,
        ),
        6 => (
            f64::from(c1_float),
            f64::from(c2_float),
            f64::from(c3_float) * 10_000.0,
            f64::from(c4_float),
        ),
        _ => (
            f64::from(c1_float),
            f64::from(c2_float),
            f64::from(c3_float),
            f64::from(c4_float),
        ),
    };
    Ok(CapitalChangeRecord {
        market_id,
        market: Market::from_id(i64::from(market_id)).ok(),
        code,
        reserved_7: record[7],
        date_raw,
        date: DateParts::from_yyyymmdd(date_raw),
        category_raw,
        category_name: capital_change_category_name(category_raw),
        c1_raw,
        c2_raw,
        c3_raw,
        c4_raw,
        c1_float,
        c2_float,
        c3_float,
        c4_float,
        c1_value,
        c2_value,
        c3_value,
        c4_value,
        record_hex: encode_hex(record),
    })
}

fn parse_finance_record(record: &[u8]) -> Result<FinanceRecord, ProtocolError> {
    if record.len() != FINANCE_RECORD_SIZE {
        return Err(ProtocolError::invalid_data(
            "finance batch",
            "invalid finance record length",
        ));
    }
    let market_id = record[0];
    let code = ascii_code(&record[1..7], "finance batch")?;
    let info = &record[7..143];
    if info.len() != FINANCE_INFO_SIZE {
        return Err(ProtocolError::invalid_data(
            "finance batch",
            "invalid finance info length",
        ));
    }
    let liu_tong_gu_ben_raw_float = little_f32(&info[..4])?;
    let province_raw = little_u16(&info[4..6])?;
    let industry_raw = little_u16(&info[6..8])?;
    let updated_date_raw = little_u32(&info[8..12])?;
    let ipo_date_raw = little_u32(&info[12..16])?;
    let mut offset = 16;
    let zong_gu_ben_raw_float = next_f32(info, &mut offset)?;
    let guo_jia_gu_raw_float = next_f32(info, &mut offset)?;
    let fa_qi_ren_fa_ren_gu_raw_float = next_f32(info, &mut offset)?;
    let fa_ren_gu_raw_float = next_f32(info, &mut offset)?;
    let b_gu_raw_float = next_f32(info, &mut offset)?;
    let h_gu_raw_float = next_f32(info, &mut offset)?;
    let eps_raw = next_f32(info, &mut offset)?;
    let zong_zi_chan_raw_float = next_f32(info, &mut offset)?;
    let liu_dong_zi_chan_raw_float = next_f32(info, &mut offset)?;
    let gu_ding_zi_chan_raw_float = next_f32(info, &mut offset)?;
    let wu_xing_zi_chan_raw_float = next_f32(info, &mut offset)?;
    let gu_dong_ren_shu_raw_float = next_f32(info, &mut offset)?;
    let liu_dong_fu_zhai_raw_float = next_f32(info, &mut offset)?;
    let chang_qi_fu_zhai_raw_float = next_f32(info, &mut offset)?;
    let zi_ben_gong_ji_jin_raw_float = next_f32(info, &mut offset)?;
    let jing_zi_chan_raw_float = next_f32(info, &mut offset)?;
    let zhu_ying_shou_ru_raw_float = next_f32(info, &mut offset)?;
    let zhu_ying_li_run_raw_float = next_f32(info, &mut offset)?;
    let ying_shou_zhang_kuan_raw_float = next_f32(info, &mut offset)?;
    let ying_ye_li_run_raw_float = next_f32(info, &mut offset)?;
    let tou_zi_shou_yu_raw_float = next_f32(info, &mut offset)?;
    let jing_ying_xian_jin_liu_raw_float = next_f32(info, &mut offset)?;
    let zong_xian_jin_liu_raw_float = next_f32(info, &mut offset)?;
    let cun_huo_raw_float = next_f32(info, &mut offset)?;
    let li_run_zong_he_raw_float = next_f32(info, &mut offset)?;
    let shui_hou_li_run_raw_float = next_f32(info, &mut offset)?;
    let jing_li_run_raw_float = next_f32(info, &mut offset)?;
    let wei_fen_li_run_raw_float = next_f32(info, &mut offset)?;
    let mei_gu_jing_zi_chan_raw_float = next_f32(info, &mut offset)?;
    let bao_liu_2_raw_float = next_f32(info, &mut offset)?;
    if offset != FINANCE_INFO_SIZE {
        return Err(ProtocolError::invalid_data(
            "finance batch",
            "finance info field layout mismatch",
        ));
    }
    Ok(FinanceRecord {
        market_id,
        market: Market::from_id(i64::from(market_id)).ok(),
        code,
        finance_info_raw: Bytes::copy_from_slice(info),
        liu_tong_gu_ben_raw_float,
        province_raw,
        industry_raw,
        updated_date_raw,
        updated_date: DateParts::from_yyyymmdd(updated_date_raw),
        ipo_date_raw,
        ipo_date: DateParts::from_yyyymmdd(ipo_date_raw),
        zong_gu_ben_raw_float,
        guo_jia_gu_raw_float,
        fa_qi_ren_fa_ren_gu_raw_float,
        fa_ren_gu_raw_float,
        b_gu_raw_float,
        h_gu_raw_float,
        eps_raw,
        zong_zi_chan_raw_float,
        liu_dong_zi_chan_raw_float,
        gu_ding_zi_chan_raw_float,
        wu_xing_zi_chan_raw_float,
        gu_dong_ren_shu_raw_float,
        liu_dong_fu_zhai_raw_float,
        chang_qi_fu_zhai_raw_float,
        zi_ben_gong_ji_jin_raw_float,
        jing_zi_chan_raw_float,
        zhu_ying_shou_ru_raw_float,
        zhu_ying_li_run_raw_float,
        ying_shou_zhang_kuan_raw_float,
        ying_ye_li_run_raw_float,
        tou_zi_shou_yu_raw_float,
        jing_ying_xian_jin_liu_raw_float,
        zong_xian_jin_liu_raw_float,
        cun_huo_raw_float,
        li_run_zong_he_raw_float,
        shui_hou_li_run_raw_float,
        jing_li_run_raw_float,
        wei_fen_li_run_raw_float,
        mei_gu_jing_zi_chan_raw_float,
        bao_liu_2_raw_float,
        record_hex: encode_hex(record),
    })
}

fn next_f32(info: &[u8], offset: &mut usize) -> Result<f32, ProtocolError> {
    let end = offset.saturating_add(4);
    let value = little_f32(info.get(*offset..end).ok_or_else(|| {
        ProtocolError::invalid_data("finance batch", "truncated finance float field")
    })?)?;
    *offset = end;
    Ok(value)
}

fn ascii_code(value: &[u8], context: &'static str) -> Result<String, ProtocolError> {
    if value.len() != 6 || !value.iter().all(u8::is_ascii) {
        return Err(ProtocolError::invalid_data(
            context,
            "invalid ASCII response code",
        ));
    }
    std::str::from_utf8(value)
        .map(str::to_owned)
        .map_err(|_| ProtocolError::invalid_data(context, "invalid ASCII response code"))
}

fn array4(value: &[u8], context: &'static str) -> Result<[u8; 4], ProtocolError> {
    value
        .try_into()
        .map_err(|_| ProtocolError::invalid_data(context, "invalid four-byte field"))
}

fn capital_change_category_name(value: u8) -> Option<&'static str> {
    match value {
        1 => Some("除权除息"),
        2 => Some("送配股上市"),
        3 => Some("非流通股上市"),
        4 => Some("未知股本变动"),
        5 => Some("股本变化"),
        6 => Some("增发新股"),
        7 => Some("股份回购"),
        8 => Some("增发新股上市"),
        9 => Some("转配股上市"),
        10 => Some("可转债上市"),
        11 => Some("扩缩股"),
        12 => Some("非流通股缩股"),
        13 => Some("送认购权证"),
        14 => Some("送认沽权证"),
        15 => Some("重整调整"),
        _ => None,
    }
}

fn is_share_count_category(value: u8) -> bool {
    matches!(value, 2 | 3 | 5 | 7 | 8 | 9 | 10)
}

fn check_payload(payload: &[u8], resource: &'static str) -> Result<(), ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource,
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    Ok(())
}

fn encode_hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(data.len().saturating_mul(2));
    for byte in data {
        output.push(char::from(DIGITS[usize::from(*byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(*byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        parse_capital_changes_payload, parse_finance_batch_payload, CapitalChangesRequest,
        FinanceBatchRequest, FINANCE_INFO_SIZE,
    };
    use crate::unit::{DateParts, NormalizedCode};
    use crate::ProtocolError;

    #[test]
    fn corporate_requests_match_frozen_wire_layouts() -> Result<(), ProtocolError> {
        let capital = CapitalChangesRequest::new(NormalizedCode::parse("sz000001")?).frame(1);
        assert_eq!(
            &capital.data[..],
            &[1, 0, 0, b'0', b'0', b'0', b'0', b'0', b'1']
        );
        let finance = FinanceBatchRequest::new(vec![NormalizedCode::parse("sz000001")?])?.frame(1);
        assert_eq!(
            &finance.data[..],
            &[1, 0, 0, b'0', b'0', b'0', b'0', b'0', b'1']
        );
        let capital_raw =
            CapitalChangesRequest::with_include_raw(NormalizedCode::parse("sz000001")?, true);
        let finance_raw =
            FinanceBatchRequest::with_include_raw(vec![NormalizedCode::parse("sz000001")?], true)?;
        assert!(capital_raw.include_raw);
        assert!(finance_raw.include_raw());
        Ok(())
    }

    #[test]
    fn parses_capital_categories_with_their_units() -> Result<(), ProtocolError> {
        let mut float_record = Vec::with_capacity(29);
        float_record.extend_from_slice(&[0]);
        float_record.extend_from_slice(b"000001");
        float_record.push(0);
        float_record.extend_from_slice(&20_260_511_u32.to_le_bytes());
        float_record.push(15);
        for value in [0.0_f32, 0.0, 3.5, 0.0] {
            float_record.extend_from_slice(&value.to_le_bytes());
        }
        let mut payload = vec![1, 0, 0];
        payload.extend_from_slice(b"000001");
        payload.extend_from_slice(&1_u16.to_le_bytes());
        payload.extend_from_slice(&float_record);
        let parsed = parse_capital_changes_payload(
            &payload,
            CapitalChangesRequest::new(NormalizedCode::parse("sz000001")?),
        )?;
        assert_eq!(parsed.records[0].category_name, Some("重整调整"));
        assert_eq!(parsed.records[0].c3_value, 3.5);

        let mut volume_record = float_record.clone();
        volume_record[12] = 5;
        volume_record[13..17].copy_from_slice(&2500.0_f32.to_le_bytes());
        volume_record[17..21].copy_from_slice(&4850.0_f32.to_le_bytes());
        volume_record[21..25].copy_from_slice(&3000.0_f32.to_le_bytes());
        volume_record[25..29].copy_from_slice(&9775.0_f32.to_le_bytes());
        payload.truncate(11);
        payload.extend_from_slice(&volume_record);
        let volume = parse_capital_changes_payload(
            &payload,
            CapitalChangesRequest::new(NormalizedCode::parse("sz000001")?),
        )?;
        assert_eq!(volume.records[0].c1_value, 25_000_000.0);
        assert_eq!(volume.records[0].c4_value, 97_750_000.0);

        let mut rights_issue_record = float_record;
        rights_issue_record[12] = 6;
        rights_issue_record[17..21].copy_from_slice(&13.98_f32.to_le_bytes());
        rights_issue_record[21..25].copy_from_slice(&1846.0_f32.to_le_bytes());
        payload.truncate(11);
        payload.extend_from_slice(&rights_issue_record);
        let rights_issue = parse_capital_changes_payload(
            &payload,
            CapitalChangesRequest::new(NormalizedCode::parse("sz000001")?),
        )?;
        assert!((rights_issue.records[0].c2_value - 13.98).abs() < 0.001);
        assert_eq!(rights_issue.records[0].c3_value, 18_460_000.0);
        Ok(())
    }

    #[test]
    fn parses_all_finance_fields_and_dates() -> Result<(), ProtocolError> {
        let mut info = Vec::with_capacity(FINANCE_INFO_SIZE);
        info.extend_from_slice(&100.0_f32.to_le_bytes());
        info.extend_from_slice(&1_u16.to_le_bytes());
        info.extend_from_slice(&2_u16.to_le_bytes());
        info.extend_from_slice(&20_260_425_u32.to_le_bytes());
        info.extend_from_slice(&19_910_403_u32.to_le_bytes());
        info.extend_from_slice(&[0_u8; 120]);
        assert_eq!(info.len(), FINANCE_INFO_SIZE);
        let mut payload = vec![1, 0, 0];
        payload.extend_from_slice(b"000001");
        payload.extend_from_slice(&info);
        let parsed = parse_finance_batch_payload(
            &payload,
            FinanceBatchRequest::new(vec![NormalizedCode::parse("sz000001")?])?,
        )?;
        assert_eq!(parsed.records[0].liu_tong_gu_ben_raw_float, 100.0);
        assert_eq!(
            parsed.records[0].updated_date,
            DateParts::new(2026, 4, 25).ok()
        );
        assert_eq!(parsed.records[0].ipo_date, DateParts::new(1991, 4, 3).ok());
        assert_eq!(parsed.records[0].finance_info_raw.len(), FINANCE_INFO_SIZE);
        Ok(())
    }
}
