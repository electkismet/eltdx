#![cfg(feature = "offline")]
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use eltdx::{CancellationToken, ErrorKind};
use eltdx::offline::{industry, ReadOptions};
type TestResult = Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("eltdx-offline-industry-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }

#[test]
fn bytes_and_file_keep_first_insertion_replace_later_and_preserve_unknowns() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("tdxhy.cfg");
    let bytes = b" 0|000001|A|||S1\r\n0|abcdef|UNKNOWN|||Z\n0|000001| B |||S2 \n0|12345|X\nbad\n0|000002|\xff\n0|1234567|X\n0|abcdef|Y|||SW\n0|ab cd | Q | | | W \n";
    std::fs::write(&path, bytes)?;
    let parsed = industry::parse(bytes)?;
    assert_eq!(parsed, industry::read(&path, &ReadOptions::default())?);
    assert_eq!(parsed.entries().iter().map(|(code, _)| code.as_str()).collect::<Vec<_>>(), ["000001", "abcdef", "000002", "ab cd "]);
    assert_eq!(parsed.get("000001").map(|v| (v.tdx(), v.sw())), Some((" B ", "S2")));
    assert_eq!(parsed.get("abcdef").map(|v| (v.tdx(), v.sw())), Some(("Y", "SW")));
    assert_eq!(parsed.get("000002").map(|v| v.tdx()), Some("\u{fffd}"));
    assert_eq!(parsed.get("ab cd ").map(|v| (v.tdx(), v.sw())), Some((" Q ", " W")));
    assert!(parsed.get("12345").is_none());
    Ok(())
}

#[test]
fn gbk_replacement_is_not_utf8_and_source_line_rules_apply() -> TestResult {
    // GBK d6 d0 ce c4 = 中文; an invalid lead followed by ASCII consumes only the lead.
    let bytes = b"0|abcdef|\xd6\xd0\xce\xc4|||\x81!\x1c0|ghijkl|X\x0b0|mnopqr|Y";
    let parsed = industry::parse(bytes)?;
    assert_eq!(parsed.get("abcdef").map(|v| (v.tdx(), v.sw())), Some(("中文", "\u{fffd}!")));
    assert_eq!(parsed.len(), 3);
    // An illegal multibyte lead resumes at the ASCII byte. Python also rejects
    // standalone 80 rather than decoding it as euro.
    let parsed = industry::parse(b"0|abcdef|\x81!|||\x80")?;
    assert_eq!(parsed.get("abcdef").map(|v| (v.tdx(), v.sw())), Some(("\u{fffd}!", "\u{fffd}")));
    Ok(())
}

#[test]
fn empty_missing_limit_and_cancel_boundaries() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("tdxhy.cfg");
    assert!(industry::parse(&[])?.is_empty());
    std::fs::write(&path, [])?;
    assert!(industry::read(&path, &ReadOptions::default())?.is_empty());
    assert_eq!(industry::read(&scratch.0.join("missing"), &ReadOptions::default()).err().map(|e| e.kind()), Some(ErrorKind::Io));
    std::fs::write(&path, b"0|abcdef|Y")?;
    assert_eq!(industry::read(&path, &ReadOptions::default().with_max_bytes(2)?).err().map(|e| e.kind()), Some(ErrorKind::LimitExceeded));
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(industry::read(&path, &ReadOptions::default().with_cancel(Some(token))).err().map(|e| e.kind()), Some(ErrorKind::Cancelled));
    Ok(())
}

#[test]
fn shared_industry_visit_preserves_rows_and_checks_invalid_lines() -> TestResult {
    let text = "invalid\n0|abcdef|A|||S1\x1c0|abcdef|B|||S2\n0|一二三四五六|C";
    let mut rows = Vec::new();
    let mut boundaries = 0;
    industry::python_legacy::visit_rows(text, |code, tdx, sw| {
        rows.push((code.to_owned(), tdx.to_owned(), sw.to_owned()));
        Ok::<_, eltdx::Error>(())
    }, &mut || { boundaries += 1; Ok(()) })?;
    assert_eq!(rows, [("abcdef".into(), "A".into(), "S1".into()),
        ("abcdef".into(), "B".into(), "S2".into()), ("一二三四五六".into(), "C".into(), "".into())]);
    assert_eq!(boundaries, 5);
    let error = industry::python_legacy::visit_rows("bad\nbad", |_, _, _| Ok::<_, eltdx::Error>(()),
        &mut || Err(eltdx::Error::new(ErrorKind::Cancelled, "source stop"))).unwrap_err();
    assert_eq!(error.message(), "source stop");
    Ok(())
}
