//! Default-HQ synchronous example. No optional feature or Python dependency.
//! Explicitly connects only when this example is run with a server address.
use eltdx::{BatchItemState, OperationOptions, TdxClient};
use std::env;
use std::io;
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = env::args().skip(1);
    let endpoint = arguments
        .next()
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "usage: hq_query <IP:port> <security> [security ...]",
            )
        })?
        .parse()?;
    let ids = arguments
        .map(|value| value.parse())
        .collect::<eltdx::Result<Vec<_>>>()?;
    if ids.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "supply at least one security identifier",
        )
        .into());
    }
    let client = TdxClient::builder()
        .endpoints(vec![endpoint])
        .pool_size(1)
        .heartbeat_interval(None)
        .build()?;
    // One total operation budget covers metadata, snapshots and optional depth.
    let options = OperationOptions::default().with_timeout(Duration::from_secs(15));
    let query = (|| -> eltdx::Result<()> {
        let batch = client.helpers().full_quotes(&ids, &options)?;
        for item in batch.items() {
            match item.state() {
                BatchItemState::Present(full) => {
                    let quote = full.snapshot();
                    println!(
                        "{} last={} volume={} {:?} amount={} {:?} depth={:?}",
                        item.security(),
                        quote.last().as_f64(),
                        quote.total().value().as_f64(),
                        quote.total().unit(),
                        quote.amount().value().as_f64(),
                        quote.amount().unit(),
                        full.depth_status()
                    );
                    // Time-of-day, unknown raw clocks and source dates remain
                    // distinct in the DTO. No local date is invented here.
                    println!(
                        "  source={:?} diagnostics={:?}",
                        quote.meta().sources(),
                        full.diagnostics()
                    );
                }
                BatchItemState::ConfirmedMissing(reason) => {
                    println!("{} missing: {reason:?}", item.security())
                }
                _ => {
                    return Err(eltdx::Error::new(
                        eltdx::ErrorKind::Unsupported,
                        "unrecognized batch occurrence state",
                    ))
                }
            }
        }
        Ok(())
    })();
    // Close even after a business failure; keep the original failure primary.
    let cleanup = client.close();
    match (query, cleanup) {
        (Err(primary), Err(cleanup)) => {
            eprintln!("close also failed: {cleanup}");
            Err(primary.into())
        }
        (Err(primary), _) => Err(primary.into()),
        (Ok(()), Err(cleanup)) => Err(cleanup.into()),
        (Ok(()), Ok(())) => Ok(()),
    }
}
