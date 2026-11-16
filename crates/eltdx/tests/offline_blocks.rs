#![cfg(feature = "offline")]
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use eltdx::{CancellationToken, ErrorKind};
use eltdx::offline::{block, ReadOptions};
type TestResult = Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!("eltdx-offline-block-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
fn cfg(name: &[u8], leaf: &[u8]) -> [u8; 120] {
    let mut row = [0; 120];
    row[..name.len()].copy_from_slice(name);
    row[50..50 + leaf.len()].copy_from_slice(leaf);
    row
}
fn dat(declared: u16) -> Vec<u8> {
    let mut bytes = vec![0; 386 + 2813];
    bytes[384..386].copy_from_slice(&1_u16.to_le_bytes());
    bytes[386..390].copy_from_slice(&[0xd6, 0xd0, 0xce, 0xc4]);
    bytes[395..397].copy_from_slice(&declared.to_le_bytes());
    bytes[397..399].copy_from_slice(&0xbeef_u16.to_le_bytes());
    for index in 0..400 {
        let code = format!("{index:06}");
        let start = 399 + index * 7;
        bytes[start..start + 6].copy_from_slice(code.as_bytes());
    }
    bytes
}

#[test]
fn declared_stock_count_over_400_is_preserved_and_categories_follow_source_filename() -> TestResult {
    let scratch = Scratch::new()?;
    for (name, category) in [("block_gn.dat", 2), ("block_fg.dat", 3), ("block_dy.dat", 1), ("block_hy.dat", 0), ("block_gnfgdy.dat", 2)] {
        let path = scratch.0.join(name);
        std::fs::write(&path, dat(401))?;
        let result = block::read(&path, &ReadOptions::default())?;
        let row = &result.rows()[0];
        assert_eq!(row.name(), "中文");
        assert_eq!(row.category(), category);
        assert_eq!(row.declared_count(), 401);
        assert_eq!(row.codes().len(), 400);
        assert_eq!(row.reserved(), 0xbeef);
    }
    Ok(())
}

#[test]
fn incomplete_header_and_declared_record_are_errors_with_index() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("block.dat");
    for length in [0, 385, 386, 386 + 2812] {
        let mut bytes = dat(1);
        bytes.truncate(length);
        std::fs::write(&path, bytes)?;
        let result = block::read(&path, &ReadOptions::default());
        assert_eq!(result.err().map(|e| e.kind()), Some(ErrorKind::ResourceFormat));
    }
    let mut bytes = dat(1);
    bytes[384..386].copy_from_slice(&2_u16.to_le_bytes());
    std::fs::write(&path, bytes)?;
    let error = match block::read(&path, &ReadOptions::default()) { Err(error) => error, Ok(_) => return Err("expected truncated record".into()) };
    assert!(error.context().iter().any(|(key, value)| key == "record_index" && value == "1"));
    Ok(())
}

#[test]
fn customer_gbk_utf8_lines_skips_and_total_byte_cap() -> TestResult {
    let scratch = Scratch::new()?;
    let mut config = cfg(&[0xd6, 0xd0, 0xce, 0xc4], b"first").to_vec();
    config.extend_from_slice(&cfg(b"", b"second"));
    config.extend_from_slice(&cfg(b"Missing", b"missing"));
    config.extend_from_slice(&cfg(b"Empty filename", b""));
    config.extend_from_slice(&[1, 2]);
    std::fs::write(scratch.0.join("blocknew.cfg"), &config)?;
    let content = b" 0000001 \r\n1600000\n0\n\n0\xff\x1c 1abcdef \n";
    std::fs::write(scratch.0.join("first.blk"), content)?;
    std::fs::write(scratch.0.join("second.blk"), b"0000002")?;
    let result = block::read_custom(&scratch.0, &ReadOptions::default())?;
    assert_eq!(result.rows().len(), 1);
    assert_eq!(result.rows()[0].blockname(), "中文");
    assert_eq!(result.rows()[0].block_type(), "first");
    assert_eq!(result.rows()[0].codes(), ["000001", "600000", "\u{fffd}", "abcdef"]);
    assert_eq!(result.ignored_tail_bytes(), 2);
    assert_eq!(result.diagnostics().len(), 4);
    let limit = config.len() as u64 + content.len() as u64;
    assert!(block::read_custom(&scratch.0, &ReadOptions::default().with_max_bytes(limit)?).is_ok());
    assert_eq!(block::read_custom(&scratch.0, &ReadOptions::default().with_max_bytes(limit - 1)?).err().map(|e| e.kind()), Some(ErrorKind::LimitExceeded));
    Ok(())
}

#[test]
fn customer_rejects_nonleaf_names_and_cancelled_reads() -> TestResult {
    let scratch = Scratch::new()?;
    for leaf in [b".".as_slice(), b"..", b"../escape", b"sub/file", b"sub\\file", b"C:escape"] {
        std::fs::write(scratch.0.join("blocknew.cfg"), cfg(b"Name", leaf))?;
        assert_eq!(block::read_custom(&scratch.0, &ReadOptions::default()).err().map(|e| e.kind()), Some(ErrorKind::ResourceFormat));
    }
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(block::read_custom(&scratch.0, &ReadOptions::default().with_cancel(Some(token))).err().map(|e| e.kind()), Some(ErrorKind::Cancelled));
    Ok(())
}

#[cfg(any(unix, windows))]
fn link(target: &Path, path: &Path) -> std::io::Result<()> {
    #[cfg(unix)] { std::os::unix::fs::symlink(target, path) }
    #[cfg(windows)] { std::os::windows::fs::symlink_file(target, path) }
}

#[test]
#[cfg(any(unix, windows))]
fn customer_symlink_escape_is_rejected_even_for_dangling_targets() -> TestResult {
    let scratch = Scratch::new()?;
    let outside = Scratch::new()?;
    std::fs::write(scratch.0.join("blocknew.cfg"), cfg(b"Name", b"escape"))?;
    let destination = outside.0.join("outside.blk");
    std::fs::write(&destination, b"0000001")?;
    match link(&destination, &scratch.0.join("escape.blk")) {
        Ok(()) => (),
        Err(error) if matches!(error.kind(), std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::Unsupported) => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    assert_eq!(block::read_custom(&scratch.0, &ReadOptions::default()).err().map(|e| e.kind()), Some(ErrorKind::ResourceFormat));
    std::fs::remove_file(&destination)?;
    assert_eq!(block::read_custom(&scratch.0, &ReadOptions::default()).err().map(|e| e.kind()), Some(ErrorKind::ResourceFormat));
    Ok(())
}
