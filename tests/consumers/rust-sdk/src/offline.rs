//! Local public-API consumer with a caller-owned temporary directory.
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use eltdx::offline::{daily, industry, AppendOptions, Coefficients, DailyBar,
    ReadOptions, TailPolicy, WireFloat};
use eltdx::{ErrorKind, RawPolicy, TradingDate};

struct TemporaryDirectory(PathBuf);
impl TemporaryDirectory {
    fn create() -> std::io::Result<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).map_err(std::io::Error::other)?.as_nanos();
        let path = std::env::temp_dir().join(format!("eltdx-consumer-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?; Ok(Self(path))
    }
}
impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        // Only this fixture's two known files; never recursively remove a path.
        let _ = fs::remove_file(self.0.join("sh600000.day"));
        let _ = fs::remove_file(self.0.join("tdxhy.cfg"));
        let _ = fs::remove_dir(&self.0);
    }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let directory = TemporaryDirectory::create()?; let path = directory.0.join("sh600000.day");
    let coefficients = Coefficients::default();
    let first = DailyBar::new(TradingDate::new(2024, 6, 28)?, [10.0, 12.0, 9.0, 11.0],
        100.0, WireFloat::from_bits(0x80000000), 17)?;
    let next = DailyBar::new(TradingDate::new(2024, 7, 1)?, [11.0, 13.0, 10.0, 12.0],
        200.0, WireFloat::from_f32(1234.5), 19)?;
    let initial = daily::write(&path, &[first.clone()], coefficients, &AppendOptions::new())?;
    super::require(initial.accepted() == 1 && initial.bytes_written() == 32, "daily write changed")?;
    // Both newer duplicates pass the fixed on-disk threshold, in input order.
    let report = daily::append(&path, &[next.clone(), first, next], coefficients, &AppendOptions::new())?;
    super::require(report.accepted() == 2 && report.skipped() == 1, "append sorted/deduplicated its input")?;
    let bytes = fs::read(&path)?;
    let read = daily::read(&path, None, &ReadOptions::new().with_raw(RawPolicy::RetainPayload))?;
    super::require(read.rows().len() == 3 && read.rows()[0].amount().bits() == 0x80000000, "daily raw float bits changed")?;
    for (row, expected) in read.rows().iter().zip(bytes.chunks_exact(32)) {
        super::require(daily::encode(row, coefficients)?.as_slice() == expected, "daily byte round trip changed")?;
        super::require(row.raw().is_some(), "daily raw retention missing")?;
    }
    let mut tailed = bytes.clone(); tailed.extend_from_slice(b"tail"); fs::write(&path, &tailed)?;
    let read = daily::read(&path, None, &ReadOptions::new())?;
    super::require(read.rows().len() == 3 && read.ignored_tail_bytes() == 4, "complete records lost after partial tail")?;
    super::require(daily::read(&path, None, &ReadOptions::new().with_tail(TailPolicy::Reject)).is_err(), "strict tail policy ignored")?;
    super::require(daily::append(&path, &[], coefficients, &AppendOptions::new())
        .is_err_and(|error| error.kind() == ErrorKind::ResourceFormat), "empty append accepted a corrupt tail")?;
    super::require(fs::read(&path)? == tailed, "rejected append modified local bytes")?;
    let config = directory.0.join("tdxhy.cfg");
    fs::write(&config, b"1|600000|first|x|y|old\n1|600001|other|x|y|other\n1|600000|last|x|y|new\n")?;
    let map = industry::read(&config, &ReadOptions::new())?;
    super::require(map.entries().iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>() == ["600000", "600001"],
        "industry first insertion order changed")?;
    let industry = map.get("600000").ok_or("missing industry")?;
    super::require(industry.tdx() == "last" && industry.sw() == "new", "industry duplicate overwrite changed")?;
    Ok(())
}
