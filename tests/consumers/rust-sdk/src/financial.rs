//! A single-dependency consumer: hand-authored ZIP and DAT, std plus eltdx.
use std::io;
use eltdx::financial::{open_calc_client, CalcClientConfig, DownloadOptions, FinancialArchiveName};
use eltdx::offline::financial::{parse_zip_detailed, FinancialCompletion, FinancialReadOptions, FinancialRecordsQuery, ReportDateSource};
use eltdx::{DownloadCompletion, ErrorKind, OperationOptions, RawPolicy, TdxClient, TradingDate};

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 { crc = (crc >> 1) ^ (0xedb88320 & (0_u32.wrapping_sub(crc & 1))); }
    }
    !crc
}
fn zip(data: &[u8]) -> Vec<u8> {
    let name = b"report.dat";
    let length = data.len() as u32;
    let checksum = crc32(data);
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PK\x03\x04");
    bytes.extend_from_slice(&20_u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]); // flags, Store method, time/date
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes.extend_from_slice(&length.to_le_bytes()); bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes()); bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(name); bytes.extend_from_slice(data);
    let central_offset = bytes.len() as u32;
    bytes.extend_from_slice(b"PK\x01\x02");
    bytes.extend_from_slice(&20_u16.to_le_bytes()); bytes.extend_from_slice(&20_u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes.extend_from_slice(&length.to_le_bytes()); bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&[0; 16]); // extra/comment/disk/internal/external/local-offset
    bytes.extend_from_slice(name);
    let central_size = bytes.len() as u32 - central_offset;
    bytes.extend_from_slice(b"PK\x05\x06"); bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&1_u16.to_le_bytes()); bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&central_size.to_le_bytes()); bytes.extend_from_slice(&central_offset.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes
}
fn require(condition: bool, message: &str) -> io::Result<()> {
    if condition { Ok(()) } else { Err(io::Error::other(message)) }
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut data = Vec::new();
    data.extend_from_slice(&0_i16.to_le_bytes()); data.extend_from_slice(&20240331_u32.to_le_bytes());
    data.extend_from_slice(&1_u16.to_le_bytes()); data.extend_from_slice(&0_u32.to_le_bytes());
    data.extend_from_slice(&4_u32.to_le_bytes()); data.extend_from_slice(&0_u32.to_le_bytes());
    data.extend_from_slice(b"600000"); data.push(1); data.extend_from_slice(&31_u32.to_le_bytes());
    data.extend_from_slice(&0x80000000_u32.to_le_bytes());
    let archive = zip(&data);
    let options = FinancialReadOptions::new().with_parse(
        eltdx::offline::financial::FinancialParseOptions::new().with_raw(RawPolicy::RetainPayload));
    let records = parse_zip_detailed(&archive, &options)?;
    require(records.rows().len() == 1, "financial row missing")?;
    require(records.rows()[0].fields()[0].bits() == 0x80000000, "negative zero bits changed")?;
    require(records.archive().is_some_and(|source| source.entry_index() == 0
        && source.entry_name() == b"report.dat"), "archive identity missing")?;
    require(records.archive_raw() == Some(archive.as_slice()), "compressed raw missing")?;
    let client = open_calc_client(&CalcClientConfig::new())?;
    let service = client.financial();
    let filename = FinancialArchiveName::new("gpcw20240630.zip")?;
    let no_io = DownloadOptions::new().with_max_bytes(0)?;
    let result = service.download_archive(&filename, &no_io, &OperationOptions::default())?;
    require(result.completion == DownloadCompletion::LimitReached && result.data.is_empty(), "zero cap changed")?;
    client.close()?;
    require(service.list_archive_files(&OperationOptions::default()).is_err_and(|error|
        error.kind() == ErrorKind::Closed), "financial owner reopened")?;
    let fixture = super::server::Fixture::start_with_files(vec![
        ("tdxfin/gpcw.txt".to_owned(), b"gpcw20240630.zip,unverified-hash,1\n".to_vec()),
        ("tdxfin/gpcw20240630.zip".to_owned(), archive.clone()),
    ])?;
    let config = CalcClientConfig::new().with_endpoint(fixture.endpoint.parse()?)
        .with_builder(TdxClient::builder().heartbeat_interval(None).pool_size(1));
    let client = open_calc_client(&config)?;
    let operation = (|| -> Result<(), Box<dyn std::error::Error>> {
        let query = FinancialRecordsQuery::new(filename);
        let result = client.financial().download_records_detailed(&query,
            &OperationOptions::default().with_raw(RawPolicy::RetainPayload))?;
        require(result.completion() == FinancialCompletion::Complete && result.rows().len() == 1, "HQ archive did not produce complete records")?;
        require(result.date_source() == ReportDateSource::ArchiveFilename
            && result.rows()[0].report_date() == Some(TradingDate::new(2024, 6, 30)?), "financial report date source changed")?;
        require(result.rows()[0].fields()[0].bits() == 0x80000000, "HQ financial float bits changed")?;
        require(result.archive_raw() == Some(archive.as_slice()), "HQ archive raw changed")?;
        require(result.source_meta().ok_or("missing financial provenance")?.sources().len() == 2,
            "financial list/archive source provenance missing")?;
        Ok(())
    })();
    let cleanup = client.close(); let observed = fixture.finish();
    operation?; cleanup?; let observed = observed?;
    require(observed.connections == 1 && observed.resources.len() == 2, "financial consumer did not use one HQ connection and two downloads")?;
    require(observed.resources[0].0 == "tdxfin/gpcw.txt" && observed.resources[1].0 == "tdxfin/gpcw20240630.zip",
        "professional financial download did not use HQ resource paths")?;
    Ok(())
}
