#![allow(clippy::unwrap_used, clippy::expect_used)]
#![cfg(feature = "offline")]
use eltdx::offline::{
    daily, ex_daily, minute, paths, AppendOptions, Coefficients, DailyBar, Durability, ExDailyBar,
    LocalCode, MinBar, MinuteKey, ReadOptions, RecordKey, SecurityType, TailPolicy, TimeOfDay,
    WireFloat,
};
use eltdx::{CancellationToken, ErrorKind, RawPolicy, TradingDate};
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn shared_path_candidate_search_stops_at_first_match() -> TestResult {
    let mut visited = Vec::new();
    let value =
        paths::python_legacy::first_directory([Ok("first"), Ok("second"), Ok("third")], |value| {
            visited.push(*value);
            Ok::<_, eltdx::Error>(*value == "second")
        })?;
    assert_eq!(value, Some("second"));
    assert_eq!(visited, ["first", "second"]);
    let mut compared = Vec::new();
    let market = paths::python_legacy::market_to_exchange_with(|id| {
        compared.push(id);
        Ok::<_, eltdx::Error>(id == 1)
    })?;
    assert_eq!(market.map(|value| value.as_str()), Some("sh"));
    assert_eq!(compared, [0, 1]);
    assert_eq!(
        daily::python_legacy::classify_leaf("s中00.day"),
        SecurityType::Unknown
    );
    Ok(())
}

#[test]
fn shared_bit_records_keep_invalid_dates_tail_and_encoder_order() -> TestResult {
    use eltdx::offline::bars::python_legacy::{self as codec, Layout, Slot};
    let bytes = daily_bytes(1);
    let mut with_tail = bytes.to_vec();
    with_tail.extend([9; 7]);
    let mut rows = Vec::new();
    codec::visit(
        &with_tail,
        Layout::Daily,
        |row| {
            rows.push(row);
            Ok::<_, eltdx::Error>(())
        },
        &mut || Ok(()),
    )?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].raw, bytes);
    assert_eq!(rows[0].amount, 0x7fc12345);
    assert_eq!(codec::date_parts(20261399), (2026, 13, 99));
    assert_eq!(codec::packed_date_parts(0), (2004, 0, 0));
    assert_eq!(codec::packed_time_parts(1501), (25, 1));
    let fields = codec::encode_fields(Layout::Daily, |slot| {
        Ok::<_, eltdx::Error>(match slot {
            Slot::Date => rows[0].date,
            Slot::Price(index) => rows[0].prices[index],
            Slot::Amount => rows[0].amount,
            Slot::Volume => rows[0].volume,
            Slot::Tail => rows[0].tail,
            Slot::Time => unreachable!(),
        })
    })?;
    assert_eq!(codec::pack_bits(Layout::Daily, &fields), Some(bytes));
    Ok(())
}

#[test]
fn shared_append_selection_keeps_duplicates_order_and_constant_threshold() -> TestResult {
    use eltdx::offline::bars::python_legacy::select_newer;
    let rows = select_newer(
        [7, 6, 7, 4].into_iter().map(Ok),
        |value| Ok::<_, eltdx::Error>(*value > 5),
        &mut || Ok(()),
    )?;
    assert_eq!(rows, [7, 6, 7]);
    let error = select_newer([Ok(1), Ok(2)], |_| Ok::<_, eltdx::Error>(true), &mut || {
        Err(eltdx::Error::new(
            ErrorKind::Cancelled,
            "stop before selection",
        ))
    })
    .unwrap_err();
    assert_eq!(error.message(), "stop before selection");
    Ok(())
}
static NEXT: AtomicU64 = AtomicU64::new(1);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> std::io::Result<Self> {
        let path = std::env::temp_dir().join(format!(
            "eltdx-offline-bars-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path)?;
        Ok(Self(path))
    }
    fn file(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn date(day: u8) -> eltdx::Result<TradingDate> {
    TradingDate::new(2026, 11, day)
}
fn key(day: u8) -> eltdx::Result<MinuteKey> {
    MinuteKey::new(date(day)?, TimeOfDay::new(9, 31, 0)?)
}
fn bar(day: u8, price: f64) -> eltdx::Result<DailyBar> {
    DailyBar::new(
        date(day)?,
        [price; 4],
        2.0,
        WireFloat::from_bits(0x80000000),
        0xfeedface,
    )
}
fn daily_bytes(day: u8) -> [u8; 32] {
    // Independently authored layout: YYYYMMDD, four integers, f32 bits, volume, flag.
    let slots = [
        20261100 + u32::from(day),
        1234,
        2345,
        987,
        1456,
        0x7fc12345,
        u32::MAX,
        0xdeadbeef,
    ];
    let mut bytes = [0; 32];
    for (index, value) in slots.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
fn minute_bytes(day: u8, integer: bool) -> [u8; 32] {
    let mut bytes = [0; 32];
    let packed: u16 = (2026 - 2004) * 2048 + 1100 + u16::from(day);
    bytes[..2].copy_from_slice(&packed.to_le_bytes());
    bytes[2..4].copy_from_slice(&571_u16.to_le_bytes());
    let slots = if integer {
        [1234, u32::MAX, 0, 999, 0x7fa12345, u32::MAX, 0xdeadbeef]
    } else {
        [
            0x80000000,
            0x7fc12345,
            0x3f9e0419,
            0xff800000,
            0x7fa12345,
            u32::MAX,
            0xdeadbeef,
        ]
    };
    for (index, value) in slots.into_iter().enumerate() {
        bytes[4 + index * 4..8 + index * 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}
fn ex_bytes(day: u8) -> [u8; 32] {
    let slots = [
        20261100 + u32::from(day),
        0x80000000,
        0x7fc12345,
        0x3f9e0419,
        0xff800000,
        0x4996b43c,
        u32::MAX,
        0x7fa54321,
    ];
    let mut bytes = [0; 32];
    for (index, value) in slots.into_iter().enumerate() {
        bytes[index * 4..index * 4 + 4].copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

#[test]
fn four_formats_roundtrip_raw_integer_float_reserved_and_hk_bits() -> TestResult {
    let scratch = Scratch::new()?;
    for raw in [
        RawPolicy::Omit,
        RawPolicy::RetainPayload,
        RawPolicy::RetainWire,
    ] {
        let options = ReadOptions::default().with_raw(raw);
        let path = scratch.file("sh600000.day");
        let original = daily_bytes(16);
        std::fs::write(&path, original)?;
        let result = daily::read(&path, None, &options)?;
        assert_eq!(
            daily::encode(&result.rows()[0], Coefficients::default())?,
            original
        );
        assert_eq!(result.rows()[0].adjust_flag(), 0xdeadbeef);
        assert_eq!(result.rows()[0].raw().is_some(), raw != RawPolicy::Omit);
        assert_eq!(
            result.rows()[0].source_integers().map(|v| v[4]),
            Some(u32::MAX)
        );
        for integer in [false, true] {
            let original = minute_bytes(16, integer);
            std::fs::write(&path, original)?;
            let result = if integer {
                minute::read_5(&path, &options)?
            } else {
                minute::read_lc(&path, &options)?
            };
            let row = &result.rows()[0];
            let encoded = if integer {
                minute::encode_5(row)?
            } else {
                minute::encode_lc(row)?
            };
            assert_eq!(encoded, original);
            assert_eq!(row.reserved(), 0xdeadbeef);
            assert_eq!(row.source_volume(), Some(u32::MAX));
            assert_eq!(row.raw().is_some(), raw != RawPolicy::Omit);
            if integer {
                assert_eq!(row.open(), 12.34);
            } else {
                assert_eq!(row.float_prices().map(|v| v[0].bits()), Some(0x80000000));
            }
        }
        let original = ex_bytes(16);
        std::fs::write(&path, original)?;
        let result = ex_daily::read(&path, &options)?;
        let row = &result.rows()[0];
        assert_eq!(row.amount(), 1234613308);
        assert_eq!(row.hk_stock_amount().as_f32(), 1234567.5);
        assert_eq!(ex_daily::encode(row)?, original);
        assert_eq!(row.raw().is_some(), raw != RawPolicy::Omit);
    }
    Ok(())
}

#[test]
fn coefficient_table_prefixes_unknown_and_custom_rounding_bounds() -> TestResult {
    let cases = [
        ("sh600000.day", SecurityType::ShA, 0.01, 0.01),
        ("SH900001.DAY", SecurityType::ShB, 0.001, 0.01),
        ("sh000001.day", SecurityType::ShIndex, 0.01, 1.0),
        ("sh588000.day", SecurityType::ShFund, 0.001, 1.0),
        ("sh113692.day", SecurityType::ShBond, 0.001, 1.0),
        ("sz300001.day", SecurityType::SzA, 0.01, 0.01),
        ("sz200001.day", SecurityType::SzB, 0.01, 0.01),
        ("sz399001.day", SecurityType::SzIndex, 0.01, 1.0),
        ("sz159001.day", SecurityType::SzFund, 0.001, 0.01),
        ("sz123001.day", SecurityType::SzBond, 0.001, 0.01),
        ("unknown.day", SecurityType::Unknown, 0.01, 0.01),
    ];
    for (name, security, price, volume) in cases {
        let decision = daily::security_coefficients(OsStr::new(name));
        assert_eq!(decision.security_type(), security);
        assert_eq!(decision.coefficients().price(), price);
        assert_eq!(decision.coefficients().volume(), volume);
        assert_eq!(decision.is_fallback(), security == SecurityType::Unknown);
    }
    for (price, volume) in [
        (0.0, 1.0),
        (1.0, -1.0),
        (f64::NAN, 1.0),
        (1.0, f64::INFINITY),
    ] {
        assert!(Coefficients::new(price, volume).is_err());
    }
    let coeff = Coefficients::new(1.0, 1.0)?;
    let half = DailyBar::new(
        date(16)?,
        [0.5, 1.5, 2.5, 3.5],
        4.5,
        WireFloat::from_bits(0),
        0,
    )?;
    let bytes = daily::encode(&half, coeff)?;
    let u32_at = |offset| {
        u32::from_le_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ])
    };
    assert_eq!(
        [u32_at(4), u32_at(8), u32_at(12), u32_at(16), u32_at(24)],
        [0, 2, 2, 4, 4]
    );
    assert!(daily::encode(&bar(16, u32::MAX as f64 + 1.0)?, coeff).is_err());
    assert!(daily::encode(&bar(16, 1.0)?, Coefficients::new(f64::MIN_POSITIVE, 1.0)?).is_err());
    assert!(TradingDate::new(2026, 2, 29).is_err());
    assert!(TradingDate::new(2000, 2, 29).is_ok());
    assert!(TradingDate::new(1900, 2, 29).is_err());
    let scratch = Scratch::new()?;
    let path = scratch.file("unknown.day");
    std::fs::write(&path, daily_bytes(16))?;
    assert!(!daily::read(&path, None, &ReadOptions::default())?
        .diagnostics()
        .is_empty());
    Ok(())
}

#[test]
fn tails_empty_append_rejection_last_keys_and_original_bytes_all_formats() -> TestResult {
    let scratch = Scratch::new()?;
    let path = scratch.file("sh600000.day");
    for format in 0..4 {
        let record = match format {
            0 => daily_bytes(16),
            1 => minute_bytes(16, false),
            2 => minute_bytes(16, true),
            _ => ex_bytes(16),
        };
        let mut original = record.to_vec();
        original.extend_from_slice(&[0xaa, 0xbb, 0xcc]);
        std::fs::write(&path, &original)?;
        let options = ReadOptions::default();
        let strict = options.clone().with_tail(TailPolicy::Reject);
        match format {
            0 => {
                assert_eq!(daily::read(&path, None, &options)?.ignored_tail_bytes(), 3);
                assert!(daily::read(&path, None, &strict).is_err());
                assert_eq!(daily::last_date(&path)?, Some(date(16)?));
                assert_eq!(
                    daily::append(
                        &path,
                        &[],
                        Coefficients::default(),
                        &AppendOptions::default()
                    )
                    .err()
                    .map(|e| e.kind()),
                    Some(ErrorKind::ResourceFormat)
                );
                assert!(daily::write(
                    &path,
                    &[bar(17, 1.0)?],
                    Coefficients::default(),
                    &AppendOptions::default()
                )
                .is_err());
            }
            1 | 2 => {
                let result = if format == 1 {
                    minute::read_lc(&path, &options)?
                } else {
                    minute::read_5(&path, &options)?
                };
                assert_eq!(result.rows().len(), 1);
                assert_eq!(result.ignored_tail_bytes(), 3);
                assert!(if format == 1 {
                    minute::read_lc(&path, &strict)
                } else {
                    minute::read_5(&path, &strict)
                }
                .is_err());
                assert_eq!(minute::last_lc_time(&path)?, Some(key(16)?));
                assert_eq!(minute::last_5_time(&path)?, Some(key(16)?));
                assert!(minute::append_lc(&path, &[], &AppendOptions::default()).is_err());
                assert!(minute::append_5(&path, &[], &AppendOptions::default()).is_err());
            }
            _ => {
                assert_eq!(ex_daily::read(&path, &options)?.ignored_tail_bytes(), 3);
                assert!(ex_daily::read(&path, &strict).is_err());
                assert_eq!(ex_daily::last_date(&path)?, Some(date(16)?));
                assert!(ex_daily::append(&path, &[], &AppendOptions::default()).is_err());
            }
        }
        assert_eq!(std::fs::read(&path)?, original);
    }
    for path in [scratch.file("missing"), scratch.0.clone(), {
        let path = scratch.file("short");
        std::fs::write(&path, [0; 31])?;
        path
    }] {
        assert_eq!(daily::last_date(&path)?, None);
        assert_eq!(ex_daily::last_date(&path)?, None);
        assert_eq!(minute::last_lc_time(&path)?, None);
        assert_eq!(minute::last_5_time(&path)?, None);
    }
    assert_eq!(
        daily::read(&scratch.file("missing"), None, &ReadOptions::default())
            .err()
            .map(|e| e.kind()),
        Some(ErrorKind::Io)
    );
    Ok(())
}

#[test]
fn append_uses_only_original_last_key_and_reports_actual_final_record() -> TestResult {
    let scratch = Scratch::new()?;
    let options = AppendOptions::default();
    let coeff = Coefficients::default();
    let path = scratch.file("sh600000.day");
    daily::write(&path, &[bar(10, 10.0)?], coeff, &options)?;
    let prefix = std::fs::read(&path)?;
    let result = daily::write(
        &path,
        &[
            bar(12, 12.0)?,
            bar(11, 11.0)?,
            bar(12, 13.0)?,
            bar(10, 10.0)?,
            bar(11, 14.0)?,
        ],
        coeff,
        &options,
    )?;
    assert_eq!(
        (result.accepted(), result.skipped(), result.bytes_written()),
        (4, 1, 128)
    );
    assert_eq!(result.final_key(), Some(RecordKey::Date(date(11)?)));
    assert_eq!(result.durability(), Durability::SyncAll);
    assert_eq!(&std::fs::read(&path)?[..32], &prefix);
    let rows = daily::read(&path, None, &ReadOptions::default())?.into_rows();
    assert_eq!(
        rows.iter().map(DailyBar::open).collect::<Vec<_>>(),
        [10.0, 12.0, 11.0, 13.0, 14.0]
    );
    for integer in [false, true] {
        let path = scratch.file(if integer { "minute.5" } else { "minute.lc1" });
        let make = |day, price| MinBar::new(key(day)?, [price; 4], 1.5, WireFloat::from_bits(0), 7);
        let initial = [make(10, 10.0)?];
        let input = [
            make(12, 12.0)?,
            make(11, 11.0)?,
            make(12, 13.0)?,
            make(10, 10.0)?,
            make(11, 14.0)?,
        ];
        let report = if integer {
            minute::write_5(&path, &initial, &options)?;
            minute::write_5(&path, &input, &options)?
        } else {
            minute::write_lc(&path, &initial, &options)?;
            minute::write_lc(&path, &input, &options)?
        };
        assert_eq!((report.accepted(), report.skipped()), (4, 1));
        assert_eq!(report.final_key(), Some(RecordKey::Minute(key(11)?)));
        assert_eq!(report.durability(), Durability::FormatDefault);
    }
    let path = scratch.file("ex.day");
    let make = |day| -> eltdx::Result<ExDailyBar> {
        Ok(ExDailyBar::new(
            date(day)?,
            [WireFloat::from_bits(0); 4],
            0x4996b43c,
            7,
            WireFloat::from_bits(0),
        ))
    };
    ex_daily::write(&path, &[make(10)?], &options)?;
    let report = ex_daily::write(
        &path,
        &[make(12)?, make(11)?, make(12)?, make(10)?, make(11)?],
        &options,
    )?;
    assert_eq!((report.accepted(), report.skipped()), (4, 1));
    assert_eq!(report.final_key(), Some(RecordKey::Date(date(11)?)));
    assert_eq!(report.durability(), Durability::FormatDefault);
    let absent = scratch.file("unsorted-new.day");
    let all = [
        bar(12, 12.0)?,
        bar(11, 11.0)?,
        bar(12, 13.0)?,
        bar(10, 10.0)?,
    ];
    let report = daily::append(&absent, &all, coeff, &options)?;
    assert_eq!(
        (report.accepted(), report.skipped(), report.bytes_written()),
        (4, 0, 128)
    );
    assert_eq!(report.final_key(), Some(RecordKey::Date(date(10)?)));
    let empty = scratch.file("empty.day");
    std::fs::write(&empty, [])?;
    assert_eq!(daily::append(&empty, &all, coeff, &options)?.accepted(), 4);
    Ok(())
}

#[test]
fn explicit_limits_cancellation_paths_and_batch_prevalidation() -> TestResult {
    let scratch = Scratch::new()?;
    assert!(ReadOptions::default().with_max_bytes(0).is_err());
    assert!(ReadOptions::default().with_max_bytes(u64::MAX).is_err());
    let path = scratch.file("sh600000.day");
    std::fs::write(&path, daily_bytes(16))?;
    assert_eq!(
        daily::read(&path, None, &ReadOptions::default().with_max_bytes(31)?)
            .err()
            .map(|e| e.kind()),
        Some(ErrorKind::LimitExceeded)
    );
    assert_eq!(
        daily::read(&path, None, &ReadOptions::default().with_max_bytes(32)?)?
            .rows()
            .len(),
        1
    );
    let token = CancellationToken::default();
    token.cancel();
    assert_eq!(
        daily::read(
            &path,
            None,
            &ReadOptions::default().with_cancel(Some(token.clone()))
        )
        .err()
        .map(|e| e.kind()),
        Some(ErrorKind::Cancelled)
    );
    assert!(daily::append(
        &path,
        &[bar(17, 1.0)?],
        Coefficients::default(),
        &AppendOptions::default().with_cancel(Some(token))
    )
    .is_err());
    assert_eq!(std::fs::read(&path)?, daily_bytes(16));
    let absent = scratch.file("absent/child.day");
    assert!(daily::write(
        &absent,
        &[bar(17, 1.0)?],
        Coefficients::default(),
        &AppendOptions::default()
    )
    .is_err());
    assert!(!scratch.file("absent").exists());
    daily::write(
        &absent,
        &[bar(17, 1.0)?],
        Coefficients::default(),
        &AppendOptions::default().with_create_parents(true),
    )?;
    let fresh = scratch.file("fresh.day");
    assert!(daily::write(
        &fresh,
        &[bar(17, 1.0)?, bar(18, u32::MAX as f64 + 1.0)?],
        Coefficients::new(1.0, 1.0)?,
        &AppendOptions::default()
    )
    .is_err());
    assert!(!fresh.exists());
    assert_eq!(
        paths::market_to_exchange(2).err().map(|e| e.kind()),
        Some(ErrorKind::InvalidArgument)
    );
    assert!(LocalCode::new("../123").is_err());
    let code = LocalCode::new("000001")?;
    assert_eq!(
        paths::find_lc1(0, &code, Some(&scratch.0))?,
        scratch.file(&format!("sz/minline/sz{}.lc1", code.as_str()))
    );
    assert_eq!(
        paths::find_lc5(0, &code, Some(&scratch.0))?,
        scratch.file(&format!("sz/fzline/sz{}.lc5", code.as_str()))
    );
    assert_eq!(
        paths::find_5(0, &code, Some(&scratch.0))?,
        scratch.file(&format!("sz/fzline/sz{}.5", code.as_str()))
    );
    assert!(TimeOfDay::new(24, 0, 0).is_err());
    assert!(MinuteKey::new(date(16)?, TimeOfDay::new(9, 31, 1)?).is_err());
    let invalid_year = MinBar::new(
        MinuteKey::new(TradingDate::new(2003, 12, 31)?, TimeOfDay::new(0, 0, 0)?)?,
        [1.0; 4],
        0.0,
        WireFloat::from_bits(0),
        0,
    )?;
    assert!(minute::encode_lc(&invalid_year).is_err());
    let invalid_year = MinBar::new(
        MinuteKey::new(TradingDate::new(2036, 1, 1)?, TimeOfDay::new(0, 0, 0)?)?,
        [1.0; 4],
        0.0,
        WireFloat::from_bits(0),
        0,
    )?;
    assert!(minute::encode_5(&invalid_year).is_err());
    Ok(())
}

#[test]
fn model_fields_share_order_without_calendar_coercion() -> TestResult {
    use eltdx::offline::bars::python_legacy;
    let mut fields = Vec::new();
    let date = python_legacy::date_integer_with(
        |field| {
            fields.push(field.to_owned());
            Ok::<_, &'static str>(match field {
                "year" => 2026_i64,
                "month" => 99,
                _ => -1,
            })
        },
        |left, right| Ok(left * i64::from(right)),
        |left, right| Ok(left + right),
    )?;
    assert_eq!(date, 20269899);
    assert_eq!(fields, ["year", "month", "day"]);
    let fields = python_legacy::datetime_fields(|field| Ok::<_, &'static str>(field.to_owned()))?;
    assert_eq!(fields, ["year", "month", "day", "hour", "minute"]);
    assert_eq!(key(16)?.datetime_key(), (2026, 11, 16, 9, 31));
    Ok(())
}
