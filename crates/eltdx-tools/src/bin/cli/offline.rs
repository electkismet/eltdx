use std::io::ErrorKind;
use std::path::PathBuf;

use eltdx::offline::{self, LocalCode, ReadOptions};
use eltdx::RawPolicy;

use super::args::{Offline, OfflineAction};
use super::output::{object, Value};
use super::{codes, hex, invalid, single_code, Result};

fn target(args: &Offline) -> Result<PathBuf> {
    if let Some(path) = args
        .file
        .as_ref()
        .filter(|path| !path.as_os_str().is_empty())
    {
        return Ok(path.clone());
    }
    let (Some(market), Some(code)) = (args.market, &args.code) else {
        return Err(invalid("需要 --file，或同时提供 --market 与 --code"));
    };
    let market = u8::try_from(market)?;
    let code = LocalCode::new(single_code(&codes(std::slice::from_ref(code)))?)?;
    let root = args.vipdoc.as_deref();
    Ok(match args.action {
        OfflineAction::Day => offline::paths::find_daily(market, &code, root)?,
        OfflineAction::Min => offline::paths::find_lc1(market, &code, root)?,
        OfflineAction::FiveMin => offline::paths::find_5(market, &code, root)?,
        _ => return Err(invalid("该命令需要 --file")),
    })
}

fn missing_file(error: &(dyn std::error::Error + 'static)) -> bool {
    if error
        .downcast_ref::<std::io::Error>()
        .is_some_and(|error| error.kind() == ErrorKind::NotFound)
    {
        return true;
    }
    error.source().is_some_and(missing_file)
}

pub(super) fn run(args: Offline) -> Result<Value> {
    let result = target(&args).and_then(|path| read(&args, &path));
    result.map_err(|error| {
        if missing_file(error.as_ref()) {
            format!("文件不存在: {error}").into()
        } else {
            error
        }
    })
}

fn read(args: &Offline, path: &std::path::Path) -> Result<Value> {
    let options = ReadOptions::new().with_raw(RawPolicy::RetainPayload);
    let bars = match args.action {
        OfflineAction::Day => offline::daily::read(path, None, &options)?.rows().iter().map(|bar| {
            let date = bar.date();
            object! {
                "year" => date.year(), "month" => date.month(), "day" => date.day(),
                "open" => bar.open(), "high" => bar.high(), "low" => bar.low(), "close" => bar.close(),
                "volume" => bar.volume(), "amount" => bar.amount().as_f64(), "adjust_flag" => bar.adjust_flag(),
                "raw" => bar.raw().map(|raw| hex(raw)).unwrap_or_default(),
            }
        }).collect::<Vec<_>>(),
        OfflineAction::Min | OfflineAction::FiveMin => {
            let result = match args.action {
                OfflineAction::FiveMin if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("5")) => offline::minute::read_5(path, &options)?,
                _ => offline::minute::read_lc(path, &options)?,
            };
            result.rows().iter().map(|bar| {
                let (date, time) = (bar.key().date(), bar.key().time());
                object! {
                    "year" => date.year(), "month" => date.month(), "day" => date.day(),
                    "hour" => time.hour(), "minute" => time.minute(),
                    "open" => bar.open(), "high" => bar.high(), "low" => bar.low(), "close" => bar.close(),
                    "volume" => bar.volume(), "amount" => bar.amount().as_f64(), "reserved" => bar.reserved(),
                    "raw" => bar.raw().map(|raw| hex(raw)).unwrap_or_default(),
                }
            }).collect()
        }
        OfflineAction::ExDay => offline::ex_daily::read(path, &options)?.rows().iter().map(|bar| {
            let date = bar.date();
            object! {
                "year" => date.year(), "month" => date.month(), "day" => date.day(),
                "open" => bar.open().as_f64(), "high" => bar.high().as_f64(),
                "low" => bar.low().as_f64(), "close" => bar.close().as_f64(),
                "amount" => bar.amount(), "volume" => bar.volume(), "settlement" => bar.settlement().as_f64(),
                "hk_stock_amount" => bar.hk_stock_amount().as_f64(),
                "raw" => bar.raw().map(|raw| hex(raw)).unwrap_or_default(),
            }
        }).collect(),
        OfflineAction::Block => return Ok(Value::Array(offline::block::read(path, &options)?.rows().iter().map(|row| {
            object! { "name" => row.name(), "category" => row.category(), "count" => row.count(), "codes" => row.codes() }
        }).collect())),
        OfflineAction::Industry => return Ok(Value::Object(offline::industry::read(path, &options)?.entries().iter().map(|(code, row)| {
            (code.clone(), Value::Array(vec![row.tdx().into(), row.sw().into()]))
        }).collect())),
        OfflineAction::Gbbq => return Ok(Value::Array(offline::gbbq::read(path, &options)?.rows().iter().map(|row| {
            object! {
                "market" => row.market(), "code" => row.code(), "datetime" => row.date_raw(), "category" => row.category(),
                "hongli_panqianliutong" => row.hongli_panqianliutong().as_f64(),
                "peigujia_qianzongguben" => row.peigujia_qianzongguben().as_f64(),
                "songgu_qianzongguben" => row.songgu_qianzongguben().as_f64(),
                "peigu_houzongguben" => row.peigu_houzongguben().as_f64(),
            }
        }).collect())),
        OfflineAction::Financial => return Ok(Value::Array(offline::financial::read(path, &Default::default())?.rows().iter().map(|row| {
            object! {
                "code" => row.code(), "market_id" => row.market_id(), "report_date" => row.report_date_raw(),
                "fields" => row.fields().iter().map(|field| field.as_f64()).collect::<Vec<_>>(),
            }
        }).collect())),
    };
    if args.last {
        return Ok(bars.into_iter().last().unwrap_or(Value::Null));
    }
    let count = usize::try_from(args.count.unsigned_abs()).unwrap_or(usize::MAX);
    let start = match args.count.cmp(&0) {
        std::cmp::Ordering::Less => count,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => bars.len().saturating_sub(count),
    };
    Ok(Value::Array(bars.into_iter().skip(start).collect()))
}
