//! Explicit numeric endpoint example; only --features ex is required.
//! Source only until the all-implementation gate opens.
use eltdx::ex::hosts::Endpoint;
use eltdx::ex::{ExClient, InstrumentsQuery};
use eltdx::{OperationOptions, Result};

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let argument = std::env::args()
        .nth(1)
        .ok_or("usage: ex_query <numeric-ip:port>")?;
    let endpoint: Endpoint = argument.parse()?;
    let client = ExClient::builder().with_endpoints(vec![endpoint]).build()?;
    let options = OperationOptions::default();
    let operation: Result<()> = (|| {
        client.connect(&options)?;
        let markets = client.markets(&options)?;
        println!("markets={}", markets.len());
        let page = client.instruments(&InstrumentsQuery::new(0).with_count(1)?, &options)?;
        for instrument in page.rows() {
            println!(
                "market={} code={} name={}",
                instrument.market().get(),
                instrument.code(),
                instrument.name()
            );
        }
        println!(
            "page={:?}, next={:?}",
            page.completion(),
            page.next_offset()
        );
        Ok(())
    })();
    let closed = client.close();
    match (operation, closed) {
        (Err(primary), Err(cleanup)) => {
            eprintln!("close also failed: {cleanup}");
            Err(primary.into())
        }
        (Err(primary), _) => Err(primary.into()),
        (Ok(()), Err(cleanup)) => Err(cleanup.into()),
        (Ok(()), Ok(())) => Ok(()),
    }
}
