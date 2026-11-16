//! Independent public-API consumer. Only std and eltdx are imported.
//! Synthetic wire bytes are authored here, without using a production parser
//! or encoder as the expected-value oracle. Execution is deferred until T30.
mod server;
#[cfg(any(feature = "sdk-offline", feature = "sdk-full"))]
mod financial;
#[cfg(any(feature = "sdk-offline", feature = "sdk-full"))]
mod offline;
#[cfg(any(feature = "sdk-f10", feature = "sdk-full"))]
mod f10;
use std::io;
use std::time::Duration;
use eltdx::{BatchItemState, CodeScanQuery, EndEvidence, ErrorKind, HqMarket, OperationOptions,
    PageOptions, RawPolicy, SecurityId, TdxClient};

fn require(condition: bool, message: &str) -> io::Result<()> {
    if condition { Ok(()) } else { Err(io::Error::other(message)) }
}

fn lazy() -> Result<(), Box<dyn std::error::Error>> {
    let client = TdxClient::builder().build()?;
    let handles = (client.quotes(), client.klines(), client.trades(), client.calendar(), client.helpers());
    drop(handles); client.close()?; client.close()?; Ok(())
}

fn loopback() -> Result<(), Box<dyn std::error::Error>> {
    let fixture = server::Fixture::start()?;
    let client = TdxClient::builder().endpoints(vec![fixture.endpoint.parse()?]).pool_size(1)
        .heartbeat_interval(None).request_timeout(Duration::from_secs(10)).build()?;
    let operation = (|| -> Result<(), Box<dyn std::error::Error>> {
        let options = OperationOptions::default().with_raw(RawPolicy::RetainPayload);
        let query = CodeScanQuery::new().with_pages(PageOptions::new(1, 8, 8)?)?;
        let codes = client.codes().all(HqMarket::Shanghai, &query, &options)?;
        require(codes.end() == EndEvidence::EmptyPage && codes.pages() == 3, "directory must continue to the terminal empty page")?;
        require(codes.items().iter().map(|row| row.id().number()).collect::<Vec<_>>() == ["600001", "600000"], "directory source order changed")?;
        let ids = ["sh600000", "sh600001", "sh600000"].iter()
            .map(|value| value.parse()).collect::<eltdx::Result<Vec<SecurityId>>>()?;
        let batch = client.quotes().snapshots(&ids, &options)?;
        require(batch.len() == 3, "snapshot occurrence count changed")?;
        for (index, item) in batch.items().iter().enumerate() {
            require(item.input_index() == index && item.security() == &ids[index], "snapshot occurrence association changed")?;
            match item.state() {
                BatchItemState::Present(quote) => {
                    require(quote.id() == &ids[index], "reversed wire response attached to wrong identity")?;
                    let expected = if ids[index].number() == "600000" { 10.0 } else { 20.0 };
                    require(quote.last().as_f64() == expected, "quote precision/value changed")?;
                    require(quote.meta().sources().iter().any(|source| source.command() == Some(0x054c)), "snapshot source command lost")?;
                }
                BatchItemState::ConfirmedMissing(_) => return Err(io::Error::other("fixture security unexpectedly missing").into()),
                _ => return Err(io::Error::other("unrecognized snapshot occurrence state").into()),
            }
        }
        require(batch.meta().raw().is_some(), "explicit raw retention was ignored")?;
        Ok(())
    })();
    let cleanup = client.close();
    // Stop/join fixture after attempting the client's own cleanup, on every path.
    let fixture_result = fixture.finish();
    if let Err(primary) = operation {
        if let Err(cleanup) = cleanup { eprintln!("close also failed: {cleanup}"); }
        if let Err(cleanup) = fixture_result { eprintln!("fixture also failed: {cleanup}"); }
        return Err(primary);
    }
    cleanup?; let observed = fixture_result?;
    require(observed.connections == 1, "one-client loopback unexpectedly opened more than one connection")?;
    require(observed.directory_offsets.windows(3).any(|offsets| offsets == [0, 1, 2]), "public collector did not issue 0/1/2 directory pages")?;
    require(observed.snapshots > 0, "consumer did not issue an actual nonempty snapshot request")?;
    client.close()?;
    let closed = client.quotes().snapshots(&[], &OperationOptions::default());
    require(matches!(closed, Err(error) if error.kind() == ErrorKind::Closed), "closed Rust owner admitted a service operation")?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match std::env::args().nth(1).as_deref() {
        None | Some("loopback") => loopback()?,
        Some("lazy") => lazy()?,
        #[cfg(any(feature = "sdk-offline", feature = "sdk-full"))]
        Some("financial") => financial::run()?,
        #[cfg(any(feature = "sdk-offline", feature = "sdk-full"))]
        Some("offline") => offline::run()?,
        #[cfg(any(feature = "sdk-f10", feature = "sdk-full"))]
        Some("f10") => f10::run()?,
        Some(_) => return Err(io::Error::new(io::ErrorKind::InvalidInput,
            "use lazy or loopback; financial/offline need sdk-offline, f10 needs sdk-f10").into()),
    }
    println!("independent Rust consumer completed"); Ok(())
}
