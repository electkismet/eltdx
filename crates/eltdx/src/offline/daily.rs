//! `.day` layout `<IIIIIfII>`, retaining integer source slots independently of raw policy.
use std::ffi::OsStr;
use std::path::Path;
use crate::error::Result;
use crate::TradingDate;
use super::{bars, diagnostic, AppendOptions, CoefficientDecision, Coefficients, DailyBar,
    Evidence, ReadOptions, ReadResult, RecordKey, SecurityType, WireFloat, WriteReport};

pub fn detect_security_type(filename: &OsStr) -> SecurityType {
    let name = Path::new(filename).file_name().unwrap_or(filename).to_str().unwrap_or("").to_lowercase();
    python_legacy::classify_leaf(&name)
}

fn classify_leaf(name: &str) -> SecurityType {
    let bytes = name.as_bytes();
    if bytes.len() < 4 { return SecurityType::Unknown; }
    match (&bytes[..2], &bytes[2..4]) {
        (b"sz", b"00" | b"30") => SecurityType::SzA,
        (b"sz", b"20") => SecurityType::SzB,
        (b"sz", b"39") => SecurityType::SzIndex,
        (b"sz", b"15" | b"16" | b"17" | b"18") => SecurityType::SzFund,
        (b"sz", b"10" | b"11" | b"12" | b"13" | b"14") => SecurityType::SzBond,
        (b"sh", b"60" | b"68") => SecurityType::ShA,
        (b"sh", b"90") => SecurityType::ShB,
        (b"sh", b"00" | b"88" | b"99") => SecurityType::ShIndex,
        (b"sh", b"50" | b"51" | b"52" | b"53" | b"55" | b"56" | b"58") => SecurityType::ShFund,
        (b"sh", b"01" | b"10" | b"11" | b"12" | b"13" | b"14" | b"20") => SecurityType::ShBond,
        _ => SecurityType::Unknown,
    }
}

#[doc(hidden)]
pub mod python_legacy {
    /// Receives the leaf normalized by the caller's Path/lower implementation.
    pub fn classify_leaf(name: &str) -> super::SecurityType { super::classify_leaf(name) }
}

pub fn security_coefficients(filename: &OsStr) -> CoefficientDecision {
    use SecurityType::*;
    let security_type = detect_security_type(filename);
    let (price, volume) = match security_type {
        ShA | SzA | SzB | Unknown => (0.01, 0.01),
        ShB | SzFund | SzBond => (0.001, 0.01),
        ShIndex | SzIndex => (0.01, 1.0),
        ShFund | ShBond => (0.001, 1.0),
    };
    CoefficientDecision::new(Coefficients::source(price, volume), security_type,
        if security_type == Unknown { Evidence::Unknown } else { Evidence::Inferred })
}

pub fn read(path: &Path, coefficients: Option<Coefficients>, options: &ReadOptions) -> Result<ReadResult<DailyBar>> {
    let decision = security_coefficients(path.file_name().unwrap_or(path.as_os_str()));
    let selected = coefficients.unwrap_or(decision.coefficients());
    let result = bars::read_records(path, options, |record, retain| decode(record, selected, retain))?;
    if coefficients.is_none() && decision.is_fallback() {
        let tail = result.ignored_tail_bytes();
        let mut diagnostics = result.diagnostics().to_vec();
        diagnostics.push(diagnostic("coefficients", "unknown security prefix: source fallback price=0.01, volume=0.01"));
        return Ok(ReadResult::new(result.into_rows(), tail, diagnostics));
    }
    Ok(result)
}

fn decode(record: &[u8; 32], coefficients: Coefficients, retain: bool) -> Result<DailyBar> {
    let date = bars::decode_date(bars::u32_at(record, 0))?;
    let integers = [bars::u32_at(record, 4), bars::u32_at(record, 8), bars::u32_at(record, 12),
        bars::u32_at(record, 16), bars::u32_at(record, 24)];
    Ok(DailyBar::decoded(date, integers, coefficients, WireFloat::from_bits(bars::u32_at(record, 20)),
        bars::u32_at(record, 28), retain.then_some(*record)))
}

pub fn encode(bar: &DailyBar, coefficients: Coefficients) -> Result<[u8; 32]> {
    let integers = match bar.integers {
        Some((integers, original)) if original == coefficients => integers,
        _ => [bars::round_u32(bar.open() / coefficients.price(), "open")?,
            bars::round_u32(bar.high() / coefficients.price(), "high")?,
            bars::round_u32(bar.low() / coefficients.price(), "low")?,
            bars::round_u32(bar.close() / coefficients.price(), "close")?,
            bars::round_u32(bar.volume() / coefficients.volume(), "volume")?],
    };
    let mut bytes = [0_u8; 32];
    bars::put_u32(&mut bytes, 0, bars::date_integer(bar.date()));
    for (index, value) in integers[..4].iter().enumerate() { bars::put_u32(&mut bytes, 4 + index * 4, *value); }
    bars::put_u32(&mut bytes, 20, bar.amount().bits());
    bars::put_u32(&mut bytes, 24, integers[4]);
    bars::put_u32(&mut bytes, 28, bar.adjust_flag());
    Ok(bytes)
}

pub fn last_date(path: &Path) -> Result<Option<TradingDate>> {
    bars::last_record(path)?.map(|record| bars::decode_date(bars::u32_at(&record, 0))).transpose()
}

pub fn append(path: &Path, rows: &[DailyBar], coefficients: Coefficients, options: &AppendOptions) -> Result<WriteReport> {
    bars::append_records(path, rows, options, true, |row| encode(row, coefficients),
        |row| RecordKey::Date(row.date()), |path| Ok(last_date(path)?.map(RecordKey::Date)))
}

/// Appends with the source daily sync policy. `Coefficients::default()` is .01/.01.
pub fn write(path: &Path, rows: &[DailyBar], coefficients: Coefficients, options: &AppendOptions) -> Result<WriteReport> {
    append(path, rows, coefficients, options)
}
