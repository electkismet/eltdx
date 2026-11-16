//! Historical financial archives over the existing HQ resource transport.
//! This module needs `offline` only; no Python, F10 HTTP or extended session.
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use crate::operation::OperationContext;
use crate::services::ResourceService;
use crate::types::ResultMeta;
use crate::{ClientBuilder, DownloadCompletion, DownloadResult, Error, ErrorKind,
    OperationOptions, Result, TdxClient, TimeoutPhase};

pub use crate::types::resources::DownloadOptions;
pub use crate::offline::financial::{DatSelector, FinancialArchiveName, FinancialCompletion,
    FinancialDatResult, FinancialFileInfo, FinancialParseOptions, FinancialReadOptions,
    FinancialRecord, FinancialRecordsQuery};
use crate::offline::{self, financial as parser};

/// The builder remains lazy. Explicit endpoints suppress host discovery/probing.
#[derive(Clone, Debug)]
pub struct CalcClientConfig { builder: ClientBuilder, endpoint: SocketAddr }
impl Default for CalcClientConfig {
    fn default() -> Self {
        Self { builder: ClientBuilder::new(), endpoint: SocketAddr::from(([120, 76, 152, 87], 7709)) }
    }
}
impl CalcClientConfig {
    pub fn new() -> Self { Self::default() }
    pub fn endpoint(&self) -> SocketAddr { self.endpoint }
    pub fn probe_hosts(&self) -> bool { false }
    pub fn builder(&self) -> &ClientBuilder { &self.builder }
    pub fn with_endpoint(mut self, endpoint: SocketAddr) -> Self { self.endpoint = endpoint; self }
    pub fn with_builder(mut self, builder: ClientBuilder) -> Self { self.builder = builder; self }
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.builder = self.builder.request_timeout(timeout); self
    }
}

/// Construct without connecting; ordinary client close/clone/owner rules apply.
pub fn open_calc_client(config: &CalcClientConfig) -> Result<TdxClient> {
    config.builder.clone().endpoints(vec![config.endpoint]).build()
}

#[derive(Clone, Debug)]
pub struct FinancialFileList { files: Vec<FinancialFileInfo>, meta: ResultMeta }
impl FinancialFileList {
    pub fn files(&self) -> &[FinancialFileInfo] { &self.files }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
    pub fn into_files(self) -> Vec<FinancialFileInfo> { self.files }
}

/// Owned service shares the original client's owner and shutdown state.
#[derive(Clone, Debug)]
pub struct FinancialService { resources: ResourceService }
impl FinancialService {
    pub(crate) fn new(resources: ResourceService) -> Self { Self { resources } }

    pub fn list_archive_files(&self, options: &OperationOptions) -> Result<Vec<FinancialFileInfo>> {
        Ok(self.list_archive_files_detailed(options)?.into_files())
    }

    pub fn list_archive_files_detailed(&self, options: &OperationOptions) -> Result<FinancialFileList> {
        let context = self.resources.begin_context(Instant::now(), "financial.list_archive_files", options)?;
        let result = self.list_in_context(&context)?;
        context.finish()?;
        Ok(result)
    }

    fn list_in_context(&self, context: &OperationContext) -> Result<FinancialFileList> {
        let download = self.resources.download_configured_in_context(context, parser::FINANCIAL_LIST_PATH,
            &DownloadOptions::new(), true)?;
        require_complete(&download.result)?;
        let files = parser::parse_file_list_with_boundary(&download.result.data, false,
            &mut || context.check_boundary(TimeoutPhase::Decode))?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        Ok(FinancialFileList { files, meta: download.meta })
    }

    /// Bounded download; LimitReached is inspectable and is never passed to ZIP.
    pub fn download_archive(&self, filename: &FinancialArchiveName, download: &DownloadOptions,
        options: &OperationOptions) -> Result<DownloadResult> {
        let context = self.resources.begin_context(Instant::now(), "financial.download_archive", options)?;
        archive_options(download)?;
        let result = self.resources.download_configured_in_context(&context, &filename.remote_path(), download, false)?.result;
        context.finish()?;
        Ok(result)
    }

    /// Complete record composition using one operation through list, download,
    /// ZIP validation/selection and DAT parsing. Hash text is never authenticated.
    pub fn download_records(&self, query: &FinancialRecordsQuery, options: &OperationOptions)
        -> Result<offline::ReadResult<FinancialRecord>> {
        Ok(self.download_records_detailed(query, options)?.into_records())
    }

    pub fn download_records_detailed(&self, query: &FinancialRecordsQuery, options: &OperationOptions)
        -> Result<FinancialDatResult> {
        self.download_records_with_options(query, &FinancialReadOptions::default(), &DownloadOptions::default(), options)
    }

    /// Explicit resource limits may narrow the defaults. Query selector/date
    /// remain authoritative; raw policy comes from the containing operation.
    pub fn download_records_with_options(&self, query: &FinancialRecordsQuery, read: &FinancialReadOptions,
        download: &DownloadOptions, options: &OperationOptions) -> Result<FinancialDatResult> {
        let context = self.resources.begin_context(Instant::now(), "financial.download_records", options)?;
        archive_options(download)?;
        // Validate the query/read combination before the first network request.
        let read = read.clone().with_selector(query.selector().clone())?
            .with_report_date(query.report_date()).with_parse(read.parse().clone().with_raw(context.raw()));
        offline::cancelled(read.parse().cancel())?;
        let download = download.clone().with_max_bytes(download.max_bytes().min(read.max_archive_bytes()))?;
        if download.max_bytes() == 0 { return Err(incomplete()); }
        let listing = self.list_in_context(&context)?;
        offline::cancelled(read.parse().cancel())?;
        // A listing entry is useful metadata, not proof of EOF or integrity.
        let hinted = listing.files.iter().find(|file| file.filename() == query.filename().as_str())
            .and_then(FinancialFileInfo::filesize);
        let download = if download.expected_size().is_none() {
            download.with_expected_size(hinted.filter(|size| *size <= u64::from(u32::MAX)))?
        } else { download };
        let archive = self.resources.download_configured_in_context(&context,
            &query.filename().remote_path(), &download, true)?;
        require_complete(&archive.result)?;
        let mut result = parser::parse_zip_with_boundary(&archive.result.data, Some(query.filename().as_str()),
            &read, &mut || context.check_boundary(TimeoutPhase::Decode))?;
        if result.completion() != FinancialCompletion::Complete {
            return Err(Error::new(ErrorKind::ResourceFormat, "financial DAT is incomplete")
                .with_context("omitted_indexes", result.omitted_indexes().to_string()));
        }
        let meta = crate::services::aggregate_meta(&context, &[listing.meta, archive.meta], None)?;
        result = result.with_source_meta(meta);
        context.finish()?;
        Ok(result)
    }
}

fn archive_options(download: &DownloadOptions) -> Result<()> {
    download.validate_hq()?;
    if download.start_offset() != 0 {
        return Err(Error::new(ErrorKind::InvalidArgument, "financial archive download must start at offset zero"));
    }
    if download.max_bytes() > parser::MAX_ARCHIVE_BYTES {
        return Err(Error::new(ErrorKind::InvalidArgument, "financial archive byte limit exceeds 64 MiB"));
    }
    Ok(())
}
fn require_complete(result: &DownloadResult) -> Result<()> {
    if result.completion != DownloadCompletion::Complete { return Err(incomplete()); }
    Ok(())
}
fn incomplete() -> Error {
    Error::new(ErrorKind::LimitExceeded, "financial resource limit reached without complete input")
}
