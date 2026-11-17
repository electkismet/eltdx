//! Read an explicitly supplied local DAT/ZIP without a Python installation.
use eltdx::offline::financial::{read_detailed, FinancialReadOptions};
use std::io;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(std::env::args_os().nth(1).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "usage: financial_read <local.dat|local.zip>",
        )
    })?);
    let result = read_detailed(&path, &FinancialReadOptions::new())?;
    println!(
        "records={} completion={:?} date_source={:?}",
        result.rows().len(),
        result.completion(),
        result.date_source()
    );
    for record in result.rows().iter().take(5) {
        println!(
            "{} market={} date={:?} fields={}",
            record.code(),
            record.market_id(),
            record.report_date(),
            record.fields().len()
        );
    }
    for diagnostic in result.diagnostics() {
        eprintln!(
            "{}: {}",
            diagnostic.stage(),
            diagnostic.message().unwrap_or("")
        );
    }
    Ok(())
}
