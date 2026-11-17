//! Download only when run with an explicit endpoint and remote archive leaf.
use eltdx::financial::{
    open_calc_client, CalcClientConfig, FinancialArchiveName, FinancialRecordsQuery,
};
use eltdx::OperationOptions;
use std::io;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let usage = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: financial_download <IP:port> <gpcwYYYYMMDD.zip>",
        )
    };
    let endpoint = arguments.next().ok_or_else(usage)?.parse()?;
    let archive = FinancialArchiveName::new(arguments.next().ok_or_else(usage)?)?;
    let client = open_calc_client(&CalcClientConfig::new().with_endpoint(endpoint))?;
    let result = client.financial().download_records_detailed(
        &FinancialRecordsQuery::new(archive),
        &OperationOptions::default(),
    );
    let cleanup = client.close();
    match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            eprintln!("close also failed: {cleanup}");
            Err(primary.into())
        }
        (Err(error), _) => Err(error.into()),
        (Ok(_), Err(error)) => Err(error.into()),
        (Ok(records), Ok(())) => {
            println!(
                "records={} date_source={:?}",
                records.rows().len(),
                records.date_source()
            );
            Ok(())
        }
    }
}
