use bytes::Bytes;

use crate::commands::klines::KlinesRequest;
use crate::error::ProtocolError;
use crate::limits::MAX_RESPONSE_PAYLOAD_SIZE;
use crate::unit::{get_volume, little_u16, little_u32, DateParts};

/// Category selector of the `0x052d` K-line family used by the terminal's
/// historical money-flow view (`ZJLX`, category 22).
///
/// The request side reuses the shared `0x052d` frame, so this constant is the
/// only selector that has to travel to the wire.
pub const HISTORY_FUND_FLOW_CATEGORY: u16 = 22;

/// Opaque prefix that precedes the record count.
///
/// The reference implementation skips these bytes without interpreting them,
/// so they are deliberately not validated here.
pub const HISTORY_FUND_FLOW_PREFIX_SIZE: usize = 9;

/// A bare little-endian `u16` record count: the empty answer the servers give today.
pub const COUNT_ONLY_SIZE: usize = 2;

/// `date` plus eight TDX-encoded tier amounts.
pub const HISTORY_FUND_FLOW_RECORD_SIZE: usize = 36;

/// One daily money-flow record: a `yyyymmdd` date followed by the four tier
/// amounts for inflow and outflow.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryFundFlowRecord {
    pub date_raw: u32,
    pub date: Option<DateParts>,
    pub super_large_in: f64,
    pub large_in: f64,
    pub medium_in: f64,
    pub small_in: f64,
    pub super_large_out: f64,
    pub large_out: f64,
    pub medium_out: f64,
    pub small_out: f64,
    pub record_hex: String,
}

impl HistoryFundFlowRecord {
    pub fn total_in(&self) -> f64 {
        self.super_large_in + self.large_in + self.medium_in + self.small_in
    }

    pub fn total_out(&self) -> f64 {
        self.super_large_out + self.large_out + self.medium_out + self.small_out
    }

    pub fn net(&self) -> f64 {
        self.total_in() - self.total_out()
    }
}

/// Whole `0x052d` category-22 response for one security.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryFundFlowSeries {
    pub request: KlinesRequest,
    pub records: Vec<HistoryFundFlowRecord>,
    pub raw_payload: Bytes,
}

/// Parse the `0x052d` category-22 body.
///
/// Two shapes are accepted:
///
/// * a bare little-endian `u16` record count (`0000`) — the empty answer every
///   reachable server currently gives, and the same "count first" convention the
///   OHLC branches of this command use;
/// * the full record layout: opaque 9-byte prefix, little-endian `u16` record
///   count, then `count * 36` records, each holding a little-endian `u32`
///   `yyyymmdd` date followed by eight TDX custom-float words ordered
///   `super large / large / medium / small` for inflow and then for outflow.
///
/// The record layout is inherited from the reference implementation and has no
/// live sample: keep requests at `include_raw=true` so `raw_payload` /
/// `record_hex` can be compared against it before trusting the decoded amounts.
pub fn parse_history_fund_flow_payload(
    payload: &[u8],
    request: KlinesRequest,
) -> Result<HistoryFundFlowSeries, ProtocolError> {
    if payload.len() > MAX_RESPONSE_PAYLOAD_SIZE {
        return Err(ProtocolError::LimitExceeded {
            resource: "history fund flow",
            actual: payload.len(),
            limit: MAX_RESPONSE_PAYLOAD_SIZE,
        });
    }
    if payload.len() == COUNT_ONLY_SIZE {
        if little_u16(payload)? != 0 {
            return Err(ProtocolError::invalid_data(
                "history fund flow",
                "history fund flow record count arrived without records",
            ));
        }
        return Ok(HistoryFundFlowSeries {
            request,
            records: Vec::new(),
            raw_payload: Bytes::copy_from_slice(payload),
        });
    }
    let count_end = HISTORY_FUND_FLOW_PREFIX_SIZE + 2;
    if payload.len() < count_end {
        return Err(ProtocolError::invalid_data(
            "history fund flow",
            "invalid history fund flow payload",
        ));
    }
    let count = usize::from(little_u16(
        &payload[HISTORY_FUND_FLOW_PREFIX_SIZE..count_end],
    )?);
    let records_end = count_end + count * HISTORY_FUND_FLOW_RECORD_SIZE;
    if payload.len() < records_end {
        return Err(ProtocolError::invalid_data(
            "history fund flow",
            "truncated history fund flow record",
        ));
    }
    if payload.len() != records_end {
        return Err(ProtocolError::invalid_data(
            "history fund flow",
            format!(
                "unexpected trailing history fund flow bytes: {}",
                payload.len() - records_end
            ),
        ));
    }

    let mut offset = count_end;
    let mut records = Vec::with_capacity(count);
    for _ in 0..count {
        let record_start = offset;
        let date_raw = little_u32(&payload[offset..offset + 4])?;
        offset += 4;
        let mut amounts = [0.0_f64; 8];
        for amount in &mut amounts {
            *amount = get_volume(little_u32(&payload[offset..offset + 4])?);
            offset += 4;
        }
        records.push(HistoryFundFlowRecord {
            date_raw,
            date: DateParts::from_yyyymmdd(date_raw),
            super_large_in: amounts[0],
            large_in: amounts[1],
            medium_in: amounts[2],
            small_in: amounts[3],
            super_large_out: amounts[4],
            large_out: amounts[5],
            medium_out: amounts[6],
            small_out: amounts[7],
            record_hex: encode_hex(&payload[record_start..offset]),
        });
    }
    Ok(HistoryFundFlowSeries {
        request,
        records,
        raw_payload: Bytes::copy_from_slice(payload),
    })
}

fn encode_hex(data: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(data.len() * 2);
    for byte in data {
        output.push(char::from(DIGITS[usize::from(*byte >> 4)]));
        output.push(char::from(DIGITS[usize::from(*byte & 0x0f)]));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{
        parse_history_fund_flow_payload, COUNT_ONLY_SIZE, HISTORY_FUND_FLOW_CATEGORY,
        HISTORY_FUND_FLOW_PREFIX_SIZE, HISTORY_FUND_FLOW_RECORD_SIZE,
    };
    use crate::commands::klines::{KlineKind, KlinesRequest};
    use crate::unit::{AdjustMode, DateParts, KlinePeriod, NormalizedCode};
    use crate::ProtocolError;

    /// TDX custom-float words that decode to exactly 2.0, 3.0 and 4.0.
    const TWO: u32 = 0x4000_0000;
    const THREE: u32 = 0x4040_0000;
    const FOUR: u32 = 0x4080_0000;

    fn flow_request() -> Result<KlinesRequest, ProtocolError> {
        KlinesRequest::new(
            NormalizedCode::parse("sz000001")?,
            KlinePeriod::normalize("flow")?,
            0,
            5,
            AdjustMode::None,
            0,
            KlineKind::Stock,
        )
    }

    #[test]
    fn flow_period_maps_to_category_twenty_two_on_the_shared_kline_frame(
    ) -> Result<(), ProtocolError> {
        let request = flow_request()?;
        assert_eq!(request.period.raw, HISTORY_FUND_FLOW_CATEGORY);
        assert_eq!(request.period.parameter, 1);
        assert_eq!(request.period.name(), "flow");

        let frame = request.frame(10);
        assert_eq!(frame.data.len(), 42);
        // The first 16 bytes stay byte-identical to the reference body
        // (market, code, category, parameter, start, count); the shared frame
        // then appends adjust, anchor date and reserved zeroes.
        assert_eq!(
            &frame.data[..16],
            &[0, 0, b'0', b'0', b'0', b'0', b'0', b'1', 22, 0, 1, 0, 0, 0, 5, 0]
        );
        assert_eq!(&frame.data[16..], &[0_u8; 26]);
        Ok(())
    }

    #[test]
    fn parses_records_and_decodes_tdx_amount_words() -> Result<(), ProtocolError> {
        let mut payload = vec![0xAA_u8; HISTORY_FUND_FLOW_PREFIX_SIZE];
        payload.extend_from_slice(&1_u16.to_le_bytes());
        payload.extend_from_slice(&20_260_831_u32.to_le_bytes());
        for word in [THREE, FOUR, TWO, THREE, FOUR, TWO, THREE, FOUR] {
            payload.extend_from_slice(&word.to_le_bytes());
        }

        let series = parse_history_fund_flow_payload(&payload, flow_request()?)?;
        assert_eq!(series.records.len(), 1);
        let record = &series.records[0];
        assert_eq!(record.date_raw, 20_260_831);
        assert_eq!(record.date, Some(DateParts::new(2026, 8, 31)?));
        assert_eq!(record.super_large_in, 3.0);
        assert_eq!(record.large_in, 4.0);
        assert_eq!(record.medium_in, 2.0);
        assert_eq!(record.small_in, 3.0);
        assert_eq!(record.super_large_out, 4.0);
        assert_eq!(record.large_out, 2.0);
        assert_eq!(record.medium_out, 3.0);
        assert_eq!(record.small_out, 4.0);
        assert_eq!(record.total_in(), 12.0);
        assert_eq!(record.total_out(), 13.0);
        assert_eq!(record.net(), -1.0);
        assert_eq!(record.record_hex.len(), HISTORY_FUND_FLOW_RECORD_SIZE * 2);
        assert_eq!(series.raw_payload.len(), payload.len());
        assert!(!series.request.include_raw);
        Ok(())
    }

    #[test]
    fn accepts_both_empty_forms_and_rejects_malformed_lengths() -> Result<(), ProtocolError> {
        // 纯 count 形式：服务端「无数据」时只回 2 字节 `0000`。
        let bare = parse_history_fund_flow_payload(&0_u16.to_le_bytes(), flow_request()?)?;
        assert!(bare.records.is_empty());
        assert_eq!(bare.raw_payload.len(), COUNT_ONLY_SIZE);

        // 完整布局 + count=0 同样是合法空序列。
        let mut prefixed = vec![0_u8; HISTORY_FUND_FLOW_PREFIX_SIZE];
        prefixed.extend_from_slice(&0_u16.to_le_bytes());
        let series = parse_history_fund_flow_payload(&prefixed, flow_request()?)?;
        assert!(series.records.is_empty());

        for bad in [
            Vec::new(),
            vec![0_u8; HISTORY_FUND_FLOW_PREFIX_SIZE],
            // count 说有条目，却没有任何记录字节。
            1_u16.to_le_bytes().to_vec(),
            {
                let mut truncated = vec![0_u8; HISTORY_FUND_FLOW_PREFIX_SIZE];
                truncated.extend_from_slice(&1_u16.to_le_bytes());
                truncated.extend_from_slice(&[0_u8; 8]);
                truncated
            },
            {
                let mut trailing = prefixed.clone();
                trailing.push(0);
                trailing
            },
        ] {
            assert!(matches!(
                parse_history_fund_flow_payload(&bad, flow_request()?),
                Err(ProtocolError::InvalidData { .. })
            ));
        }
        Ok(())
    }
}
