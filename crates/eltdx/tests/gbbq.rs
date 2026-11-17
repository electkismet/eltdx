#![cfg(feature = "offline")]
use eltdx::offline::{gbbq, ReadOptions};
use eltdx::{CancellationToken, ErrorKind, RawPolicy};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

// Source constants are data; this independently authored inverse uses u64 modulo
// arithmetic and ascending subkeys, unlike the production descending decryptor.
#[path = "../src/offline/gbbq_keys.rs"]
mod original_constants;
type TestResult = Result<(), Box<dyn std::error::Error>>;
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "eltdx-offline-gbbq-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}
fn key(offset: usize) -> u64 {
    u64::from(word(original_constants::KEYS, offset))
}
fn encrypt(source: &[u8], output: &mut [u8]) {
    let mut left = u64::from(word(source, 4));
    let mut right = u64::from(word(source, 0)) ^ key(0);
    for subkey in 1..=16 {
        let octets = (right as u32).to_be_bytes();
        let sum = (key(0x448 + octets[1] as usize * 4) + key(0x48 + octets[0] as usize * 4))
            % 0x1_0000_0000;
        let mix = sum ^ key(0x848 + octets[2] as usize * 4);
        let nonlinear = (mix + key(0xc48 + octets[3] as usize * 4)) % 0x1_0000_0000;
        let next = left ^ nonlinear ^ key(subkey * 4);
        left = right;
        right = next;
    }
    output[..4].copy_from_slice(&((left ^ key(0x44)) as u32).to_le_bytes());
    output[4..8].copy_from_slice(&(right as u32).to_le_bytes());
}
fn clear_row(category: u8, market: u8, date_raw: u32, bits: [u32; 4]) -> [u8; 29] {
    let mut bytes = [0; 29];
    bytes[0] = market;
    bytes[1..7].copy_from_slice(b"600000");
    bytes[8..12].copy_from_slice(&date_raw.to_le_bytes());
    bytes[12] = category;
    for (index, value) in bits.into_iter().enumerate() {
        bytes[13 + index * 4..17 + index * 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
fn fixture(rows: &[[u8; 29]]) -> Vec<u8> {
    let mut bytes = (rows.len() as u32).to_le_bytes().to_vec();
    for row in rows {
        let mut encrypted = [0; 29];
        for start in [0, 8, 16] {
            encrypt(&row[start..start + 8], &mut encrypted[start..start + 8]);
        }
        encrypted[24..].copy_from_slice(&row[24..]);
        bytes.extend_from_slice(&encrypted);
    }
    bytes
}

#[test]
fn independent_clear_vectors_preserve_category_date_and_nonfinite_bits() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("gbbq");
    let rows = [
        clear_row(
            1,
            1,
            20261116,
            [0x3f800000, 0x40000000, 0x40400000, 0x40800000],
        ),
        clear_row(
            5,
            255,
            u32::MAX,
            [0x80000000, 0x7fc12345, 0xff800000, 0xffffffff],
        ),
        clear_row(255, 0, 0, [0, 0x7f7fffff, 0x00000001, 0x7fa54321]),
    ];
    let bytes = fixture(&rows);
    std::fs::write(&path, &bytes)?;
    for raw in [
        RawPolicy::Omit,
        RawPolicy::RetainPayload,
        RawPolicy::RetainWire,
    ] {
        let result = gbbq::read(&path, &ReadOptions::default().with_raw(raw))?;
        assert_eq!(result.rows().len(), rows.len());
        for (index, row) in result.rows().iter().enumerate() {
            assert_eq!(row.clear_bytes(), &rows[index]);
            assert_eq!(row.date_raw(), word(&rows[index], 8));
            assert_eq!(row.category(), rows[index][12]);
            assert_eq!(row.market(), rows[index][0]);
            assert_eq!(row.code(), "600000");
            assert_eq!(
                row.values().map(|value| value.bits()),
                [13, 17, 21, 25].map(|offset| word(&rows[index], offset))
            );
            assert_eq!(row.raw().is_some(), raw != RawPolicy::Omit);
        }
    }
    Ok(())
}

#[test]
fn short_header_empty_declared_incomplete_is_indexed_regardless_of_bar_tail_policy() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("gbbq");
    for bytes in [vec![], vec![1], vec![1, 2, 3], vec![0, 0, 0, 0, 0xff]] {
        std::fs::write(&path, bytes)?;
        assert!(gbbq::read(&path, &ReadOptions::default())?
            .rows()
            .is_empty());
    }
    let row = clear_row(1, 1, 20261116, [0; 4]);
    let mut bytes = fixture(&[row]);
    bytes[..4].copy_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&[0xff; 28]);
    std::fs::write(&path, &bytes)?;
    let error = match gbbq::read(&path, &ReadOptions::default()) {
        Err(error) => error,
        Ok(_) => return Err("expected truncated GBBQ".into()),
    };
    assert_eq!(error.kind(), ErrorKind::ResourceFormat);
    assert!(error
        .context()
        .iter()
        .any(|(key, value)| key == "record_index" && value == "1"));
    bytes[..4].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&path, &bytes)?;
    assert_eq!(
        gbbq::read(&path, &ReadOptions::default())
            .err()
            .map(|e| e.kind()),
        Some(ErrorKind::ResourceFormat)
    );
    Ok(())
}

#[test]
fn missing_limit_cancel_and_invalid_utf8_code_are_explicit() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.0.join("gbbq");
    assert_eq!(
        gbbq::read(&path, &ReadOptions::default())
            .err()
            .map(|e| e.kind()),
        Some(ErrorKind::Io)
    );
    let mut row = clear_row(1, 0, 0, [0; 4]);
    row[1] = 0xff;
    std::fs::write(&path, fixture(&[row]))?;
    assert_eq!(
        gbbq::read(&path, &ReadOptions::default())
            .err()
            .map(|e| e.kind()),
        Some(ErrorKind::ResourceFormat)
    );
    assert_eq!(
        gbbq::read(&path, &ReadOptions::default().with_max_bytes(32)?)
            .err()
            .map(|e| e.kind()),
        Some(ErrorKind::LimitExceeded)
    );
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        gbbq::read(&path, &ReadOptions::default().with_cancel(Some(token)))
            .err()
            .map(|e| e.kind()),
        Some(ErrorKind::Cancelled)
    );
    Ok(())
}

#[test]
fn shared_cipher_preserves_read_order_and_returns_callback_errors() -> TestResult {
    use gbbq::python_legacy::{decrypt_block, Cipher};
    struct Words {
        reads: Vec<usize>,
        fail: bool,
    }
    impl Cipher for Words {
        type Word = u32;
        type Error = &'static str;
        fn key(&mut self, offset: usize) -> Result<u32, Self::Error> {
            self.reads.push(offset);
            if self.fail && offset == 64 {
                Err("key callback")
            } else {
                Ok(0)
            }
        }
        fn input(&mut self, offset: usize) -> Result<u32, Self::Error> {
            self.reads.push(0x10000 + offset);
            Ok(if offset == 12 { 10 } else { 20 })
        }
        fn table(
            &mut self,
            _: &u32,
            _: Option<u32>,
            _: u32,
            base: usize,
        ) -> Result<u32, Self::Error> {
            self.key(base)
        }
        fn xor_mask(&mut self, left: &u32, right: &u32) -> Result<u32, Self::Error> {
            Ok(left ^ right)
        }
        fn add_mask(&mut self, left: &u32, right: &u32) -> Result<u32, Self::Error> {
            Ok(left.wrapping_add(*right))
        }
    }
    let mut words = Words {
        reads: Vec::new(),
        fail: false,
    };
    assert_eq!(decrypt_block(&mut words, 12), Ok((20, 10)));
    assert_eq!(
        &words.reads[..8],
        &[0x44, 0x1000c, 0x10010, 0x448, 0x48, 0x848, 0xc48, 64]
    );
    assert_eq!(words.reads.last(), Some(&0));
    let mut words = Words {
        reads: Vec::new(),
        fail: true,
    };
    assert_eq!(decrypt_block(&mut words, 12), Err("key callback"));
    assert_eq!(words.reads.last(), Some(&64));
    Ok(())
}
