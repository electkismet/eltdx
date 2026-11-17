//! Measurement worker, not a public-server probe. Requires `offline`.
//! Input fixture and endpoint are supplied by rust_sdk_performance.py.
//! Core DAT parsing and loopback timings are separate measurements. Values in
//! the synthetic DAT are IEEE754 bits; directory rows contain decimal metadata.
use std::env;
use std::hint::black_box;
use std::io::{self, Write};
use std::time::{Duration, Instant};
use eltdx::{CancellationToken, CodeQuery, ErrorKind, HqMarket, OperationOptions, TdxClient};
use eltdx::offline::financial::{parse_dat, FinancialParseOptions};

type Outcome<T = ()> = Result<T, Box<dyn std::error::Error>>;
fn invalid(message: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidInput, message) }
fn number(arguments: &mut impl Iterator<Item = String>) -> Outcome<usize> {
    let number = arguments.next().ok_or_else(|| invalid("missing iteration count"))?.parse()?;
    if !(1..=100_000).contains(&number) { return Err(invalid("iteration count outside1..=100000").into()); }
    Ok(number)
}
fn client(endpoint: std::net::SocketAddr) -> eltdx::Result<TdxClient> {
    TdxClient::builder().endpoints(vec![endpoint]).pool_size(1).heartbeat_interval(None)
        .request_timeout(Duration::from_secs(5)).build()
}
fn query(client: &TdxClient, index: usize, external: bool) -> Outcome {
    let query = CodeQuery::new().with_start(u32::try_from(index)?)?.with_limit(1)?;
    let rows = client.codes().list(HqMarket::Shanghai, &query, &OperationOptions::default())?;
    if rows.items().is_empty() || (!external && (rows.items().len() != 1 || rows.items()[0].id().number() != format!("{:06}", 600_000 + index))) {
        return Err(invalid("loopback response identity mismatch").into());
    }
    black_box(rows); Ok(())
}
fn main() -> Outcome {
    if env::var("ELTDX_IMPLEMENTATION_COMPLETE").as_deref() != Ok("yes") {
        return Err(invalid("complete all implementation sources before measurements").into());
    }
    let mut arguments = env::args().skip(1);
    let mode = arguments.next().ok_or_else(|| invalid("mode required"))?;
    if !matches!(mode.as_str(), "core" | "loopback" | "close" | "cancel" | "external") {
        return Err(invalid("mode must be core/loopback/close/cancel/external").into());
    }
    let source = arguments.next().ok_or_else(|| invalid("DAT path or endpoint required"))?;
    let samples = number(&mut arguments)?; let warmup = number(&mut arguments)?;
    if arguments.next().is_some() { return Err(invalid("unexpected measurement argument").into()); }
    let input = if mode == "core" { Some(std::fs::read(&source)?) } else { None };
    let options = FinancialParseOptions::new();
    let endpoint = if matches!(mode.as_str(), "loopback" | "close" | "external") { Some(source.parse()?) } else { None };
    let shared = if matches!(mode.as_str(), "loopback" | "external") { Some(client(endpoint.ok_or_else(|| invalid("missing endpoint"))?)?) } else { None };
    let mut stdout = io::BufWriter::new(io::stdout().lock());
    writeln!(stdout, "{{\"kind\":\"start\",\"mode\":\"{mode}\",\"samples\":{samples},\"warmup\":{warmup},\"concurrency\":1}}")?;
    let work = (|| -> Outcome {
        for iteration in 0..warmup + samples {
            let index = iteration % 1000;
            let prepared = if mode == "close" {
                let client = client(endpoint.ok_or_else(|| invalid("missing close endpoint"))?)?;
                if let Err(error) = query(&client, index, false) {
                    if let Err(cleanup) = client.close() { eprintln!("close preparation cleanup failed: {cleanup}"); }
                    return Err(error);
                }
                Some(client)
            } else { None };
            let started = Instant::now();
            match mode.as_str() {
                "core" => {
                    let result = parse_dat(input.as_deref().ok_or_else(|| invalid("missing DAT"))?, None, &options)?;
                    let elapsed = started.elapsed();
                    if result.rows().len() != 256 || result.rows().iter().any(|row| row.fields().len() != 64) {
                        return Err(invalid("unexpected fixed DAT dimensions").into());
                    }
                    black_box(result);
                    if iteration >= warmup { writeln!(stdout, "{{\"kind\":\"sample\",\"id\":{},\"latency_ns\":{}}}", iteration - warmup, elapsed.as_nanos())?; }
                    continue;
                }
                "loopback" | "external" => query(shared.as_ref().ok_or_else(|| invalid("missing shared owner"))?, index, mode == "external")?,
                "close" => prepared.as_ref().ok_or_else(|| invalid("missing close owner"))?.close()?,
                "cancel" => {
                    // This separately named microcase measures pre-admission
                    // cancellation only. Admitted cancellation comes from the
                    // actual inflight stress log, never this zero-wire path.
                    let client = TdxClient::builder().build()?;
                    let token = CancellationToken::default(); token.cancel();
                    let result = client.codes().list(HqMarket::Shanghai, &CodeQuery::new(),
                        &OperationOptions::default().with_cancellation(token));
                    if !matches!(result, Err(error) if error.kind() == ErrorKind::Cancelled) {
                        return Err(invalid("pre-admission cancellation changed").into());
                    }
                    client.close()?;
                }
                _ => return Err(invalid("mode must be core/loopback/close/cancel/external").into()),
            }
            if iteration >= warmup { writeln!(stdout, "{{\"kind\":\"sample\",\"id\":{},\"latency_ns\":{}}}", iteration - warmup, started.elapsed().as_nanos())?; }
        }
        Ok(())
    })();
    let cleanup = shared.as_ref().map(TdxClient::close).transpose();
    if let Err(error) = &cleanup { eprintln!("measurement close failed: {error}"); }
    work?; cleanup?;
    writeln!(stdout, "{{\"kind\":\"finish\",\"completed\":{samples}}}")?;
    stdout.flush()?; Ok(())
}
