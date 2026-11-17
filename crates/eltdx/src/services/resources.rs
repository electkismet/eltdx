use super::resource_legacy::DownloadCursor;
use super::{aggregate_meta, allocation_error, reply_meta};
use crate::config::ClientConfig;
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions};
use crate::types::resources::DownloadOptions;
use crate::types::{
    BlockMeta, DownloadCompletion, DownloadResult, ResultMeta, StatsQuery, StatsResource,
};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use bytes::Bytes;
use eltdx_protocol::commands::resources::{BlockMetaRequest, FileContentRequest};
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::{Engine, PinHandle};
use std::time::Instant;

fn fetch_resource_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<DecodedReply>,
) -> Result<DecodedReply> {
    context.check_boundary(TimeoutPhase::Admission)?;
    let file = match &request {
        CommandRequest::FileContent(_) => true,
        CommandRequest::BlockMeta(_) => false,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "resource service requires file or block metadata request",
            ))
        }
    };
    let reply = fetch(request)?;
    if !matches!(
        (&reply.response, file),
        (CommandResponse::FileContent(_), true) | (CommandResponse::BlockMeta(_), false)
    ) {
        return Err(protocol("unexpected resource response"));
    }
    Ok(reply)
}

pub(crate) fn legacy_resource_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<DecodedReply>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "legacy resource adapter requires a Python-profile operation",
        ));
    }
    Ok(fetch_resource_reply(context, request, fetch)?.response)
}

#[derive(Clone, Debug)]
pub struct ResourceService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
}
pub(crate) struct ResourceDownload {
    pub result: DownloadResult,
    pub meta: ResultMeta,
}

impl ResourceService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig) -> Self {
        Self {
            executor,
            engine,
            config,
        }
    }

    pub(crate) fn begin_context(
        &self,
        entry: Instant,
        name: &'static str,
        options: &OperationOptions,
    ) -> Result<OperationContext> {
        self.executor
            .begin_context_at(entry, name, &self.config, options)
    }

    pub fn read(
        &self,
        path: &str,
        offset: u64,
        size: usize,
        options: &OperationOptions,
    ) -> Result<DownloadResult> {
        let entry = Instant::now();
        let context =
            self.executor
                .begin_context_at(entry, "resource_read", &self.config, options)?;
        let path = crate::types::resources::normalize_path(path)?;
        validate_range(offset, size)?;
        let result = if size == 0 {
            limited(Bytes::new(), offset)
        } else {
            self.chunk(&context, None, &path, offset, size, ChildPurpose::Required)?
                .0
        };
        context.finish()?;
        Ok(result)
    }

    pub fn download_file(
        &self,
        path: &str,
        max_bytes: usize,
        options: &OperationOptions,
    ) -> Result<DownloadResult> {
        self.download_entry(path, max_bytes, options, false)
    }

    pub fn download_complete(
        &self,
        path: &str,
        max_bytes: usize,
        options: &OperationOptions,
    ) -> Result<Bytes> {
        Ok(self.download_entry(path, max_bytes, options, true)?.data)
    }

    fn download_entry(
        &self,
        path: &str,
        max_bytes: usize,
        options: &OperationOptions,
        strict: bool,
    ) -> Result<DownloadResult> {
        let entry = Instant::now();
        let context =
            self.executor
                .begin_context_at(entry, "resource_download", &self.config, options)?;
        let result = self
            .download_in_context(&context, path, max_bytes, 30_000, false)?
            .result;
        if strict && result.completion != DownloadCompletion::Complete {
            return Err(incomplete(max_bytes));
        }
        context.finish()?;
        Ok(result)
    }

    /// Shared seam for financial resources and the later legacy adapter. The
    /// caller retains its operation profile; this function never starts another
    /// operation or changes its deadline. Python cap-zero pin observation and
    /// optional unlimited caps must be selected explicitly by its later bridge.
    pub(crate) fn download_in_context(
        &self,
        context: &OperationContext,
        path: &str,
        max_bytes: usize,
        chunk_size: usize,
        retain_meta: bool,
    ) -> Result<ResourceDownload> {
        self.download_bounded_in_context(context, path, max_bytes, chunk_size, 0, None, retain_meta)
    }

    pub(crate) fn download_configured_in_context(
        &self,
        context: &OperationContext,
        path: &str,
        download: &DownloadOptions,
        retain_meta: bool,
    ) -> Result<ResourceDownload> {
        download.validate_hq()?;
        self.download_bounded_in_context(
            context,
            path,
            usize::try_from(download.max_bytes()).map_err(|_| incomplete(usize::MAX))?,
            download.chunk_size().get() as usize,
            download.start_offset(),
            Some(download.max_chunks().get() as usize),
            retain_meta,
        )
    }

    fn download_bounded_in_context(
        &self,
        context: &OperationContext,
        path: &str,
        max_bytes: usize,
        chunk_size: usize,
        start_offset: u64,
        max_chunks: Option<usize>,
        retain_meta: bool,
    ) -> Result<ResourceDownload> {
        context.check_boundary(TimeoutPhase::Admission)?;
        let path = crate::types::resources::normalize_path(path)?;
        if u32::try_from(max_bytes).is_err()
            || !(1..=60_000).contains(&chunk_size)
            || start_offset
                .checked_add(max_bytes as u64)
                .is_none_or(|end| end > u64::from(u32::MAX))
        {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "download cap must fit u32 and chunk_size must be 1..60000",
            ));
        }
        if max_bytes == 0 {
            return Ok(ResourceDownload {
                result: limited(Bytes::new(), start_offset),
                meta: aggregate_meta(context, &[], None)?,
            });
        }
        let mut pin = self.executor.acquire_pin(&self.engine, context)?;
        let result = (|| {
            let mut output = Vec::new();
            let mut pages = Vec::new();
            let mut source = None;
            let mut retained_bytes = 0_usize;
            let mut cursor = DownloadCursor::new(chunk_size, Some(max_bytes));
            let mut page_index = 0_usize;
            while let Some((offset, size)) = cursor.request() {
                context.check_boundary(TimeoutPhase::Admission)?;
                if max_chunks.is_some_and(|maximum| page_index >= maximum) {
                    break;
                }
                if retain_meta && page_index == 65_536 {
                    return Err(Error::new(
                        ErrorKind::LimitExceeded,
                        "resource provenance exceeds 65536 response pages",
                    ));
                }
                output.try_reserve_exact(size).map_err(allocation_error)?;
                let wire_offset = start_offset
                    .checked_add(offset as u64)
                    .ok_or_else(|| incomplete(max_bytes))?;
                let (chunk, meta, identity) = self.chunk(
                    context,
                    Some(pin.handle()?),
                    &path,
                    wire_offset,
                    size,
                    if page_index == 0 {
                        ChildPurpose::Required
                    } else {
                        ChildPurpose::Page
                    },
                )?;
                if source
                    .as_ref()
                    .is_some_and(|expected| expected != &identity)
                {
                    return Err(Error::new(
                        ErrorKind::Protocol,
                        "resource source changed during a pinned download",
                    ));
                }
                source.get_or_insert(identity);
                let next_len = output
                    .len()
                    .checked_add(chunk.data.len())
                    .ok_or_else(|| incomplete(max_bytes))?;
                if next_len > max_bytes {
                    return Err(incomplete(max_bytes));
                }
                context.check_boundary(TimeoutPhase::Aggregate)?;
                output.extend_from_slice(&chunk.data);
                cursor
                    .advance(chunk.data.len())
                    .ok_or_else(|| incomplete(max_bytes))?;
                if retain_meta {
                    if let Some(raw) = meta.raw() {
                        for bytes in raw
                            .payloads()
                            .iter()
                            .chain(raw.wire_frames().unwrap_or(&[]))
                        {
                            retained_bytes = retained_bytes
                                .checked_add(bytes.len())
                                .ok_or_else(provenance_limit)?;
                            if retained_bytes > 128 * 1024 * 1024 {
                                return Err(provenance_limit());
                            }
                        }
                    }
                    pages.try_reserve(1).map_err(allocation_error)?;
                    pages.push(meta);
                }
                page_index = page_index
                    .checked_add(1)
                    .ok_or_else(|| incomplete(max_bytes))?;
                if chunk.completion == DownloadCompletion::Complete {
                    return Ok(ResourceDownload {
                        result: DownloadResult {
                            data: Bytes::from(output),
                            completion: DownloadCompletion::Complete,
                            next_offset: None,
                        },
                        meta: aggregate_meta(context, &pages, None)?,
                    });
                }
            }
            Ok(ResourceDownload {
                result: limited(Bytes::from(output), start_offset + cursor.offset() as u64),
                meta: aggregate_meta(context, &pages, None)?,
            })
        })();
        match result {
            Ok(value) => {
                self.executor.release_pin(context, &mut pin, None)?;
                Ok(value)
            }
            Err(root) => match self.executor.release_pin(context, &mut pin, Some(root)) {
                Err(error) => Err(error),
                Ok(()) => Err(Error::new(
                    ErrorKind::Internal,
                    "resource cleanup lost its root cause",
                )),
            },
        }
    }

    fn chunk(
        &self,
        context: &OperationContext,
        pin: Option<&PinHandle>,
        path: &str,
        offset: u64,
        size: usize,
        purpose: ChildPurpose,
    ) -> Result<(DownloadResult, ResultMeta, String)> {
        validate_range(offset, size)?;
        let request = CommandRequest::FileContent(
            FileContentRequest::new(path, offset as u32, size as u32)
                .map_err(Error::from_protocol)?,
        );
        let reply = fetch_resource_reply(context, request, |request| {
            let raw = match pin {
                Some(pin) => self.executor.execute_pinned_raw(
                    &self.engine,
                    pin,
                    context,
                    request,
                    purpose,
                    None,
                )?,
                None => self
                    .executor
                    .execute_raw(&self.engine, context, request, purpose, None)?,
            };
            self.executor.decode_unpriced(context, raw)
        })?;
        let CommandResponse::FileContent(chunk) = &reply.response else {
            return Err(protocol("unexpected file content response"));
        };
        if chunk.chunk_len as usize != chunk.content.len() || chunk.content.len() > size {
            return Err(protocol(
                "file content declared length does not match requested/actual bytes",
            ));
        }
        let next = offset
            .checked_add(u64::from(chunk.chunk_len))
            .ok_or_else(|| protocol("file offset overflow"))?;
        let complete = chunk.content.len() < size;
        let meta = reply_meta(
            context,
            &self.config,
            &reply,
            &[chunk.content_range],
            "resources.read",
        );
        let identity = format!(
            "{}:{:?}:{:?}:{:?}",
            reply.source.host,
            reply.source.engine_epoch,
            reply.source.slot_id,
            reply.source.generation
        );
        Ok((
            DownloadResult {
                data: chunk.content.clone(),
                completion: if complete {
                    DownloadCompletion::Complete
                } else {
                    DownloadCompletion::LimitReached
                },
                next_offset: (!complete).then_some(next),
            },
            meta,
            identity,
        ))
    }

    pub fn block_meta(&self, filename: &str, options: &OperationOptions) -> Result<BlockMeta> {
        let entry = Instant::now();
        let context = self
            .executor
            .begin_context_at(entry, "block_meta", &self.config, options)?;
        let request = CommandRequest::BlockMeta(
            BlockMetaRequest::new(filename).map_err(Error::from_protocol)?,
        );
        let reply = fetch_resource_reply(&context, request, |request| {
            let raw = self.executor.execute_raw(
                &self.engine,
                &context,
                request,
                ChildPurpose::Required,
                None,
            )?;
            self.executor.decode_unpriced(&context, raw)
        })?;
        let CommandResponse::BlockMeta(value) = &reply.response else {
            return Err(protocol("unexpected block metadata response"));
        };
        let result = BlockMeta::new(
            value.size,
            value.md5.clone(),
            reply_meta(&context, &self.config, &reply, &[], "resources.block_meta"),
        );
        context.finish()?;
        Ok(result)
    }

    pub fn read_stats(
        &self,
        query: &StatsQuery,
        options: &OperationOptions,
    ) -> Result<StatsResource> {
        let entry = Instant::now();
        let context = self
            .executor
            .begin_context_at(entry, "read_stats", &self.config, options)?;
        let result = self.read_stats_in_context(&context, query)?;
        context.finish()?;
        Ok(result)
    }

    /// Required resource work belongs to the composition's existing deadline.
    pub(crate) fn read_stats_in_context(
        &self,
        context: &OperationContext,
        query: &StatsQuery,
    ) -> Result<StatsResource> {
        context.check_boundary(TimeoutPhase::Admission)?;
        query.validate()?;
        let cap = crate::stats::MAX_STATS_ZIP_BYTES;
        let download = self.download_in_context(
            context,
            query.path(),
            cap.checked_add(1).ok_or_else(|| incomplete(cap))?,
            query.chunk_size(),
            true,
        )?;
        if download.result.data.len() > cap {
            return Err(incomplete(cap));
        }
        if download.result.completion != DownloadCompletion::Complete {
            return Err(incomplete(cap));
        }
        let parsed = crate::stats::parse_stats_archive(
            &download.result.data,
            &format!("tdx://{}", query.path()),
            &mut || context.check_boundary(TimeoutPhase::Decode),
        )?;
        super::stats_projection::project(parsed, download.meta, context)
    }
}

fn validate_range(offset: u64, size: usize) -> Result<()> {
    if size > 60_000
        || offset > u64::from(u32::MAX)
        || offset
            .checked_add(size as u64)
            .is_none_or(|end| end > u64::from(u32::MAX))
    {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "file range must fit wire u32; size must be 0..60000",
        ));
    }
    Ok(())
}
fn limited(data: Bytes, offset: u64) -> DownloadResult {
    DownloadResult {
        data,
        completion: DownloadCompletion::LimitReached,
        next_offset: Some(offset),
    }
}
fn incomplete(cap: usize) -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "resource byte cap reached without complete input",
    )
    .with_context("limit", format!("{cap}"))
}
fn protocol(message: &str) -> Error {
    Error::new(ErrorKind::Protocol, message)
}
fn provenance_limit() -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "resource retained source buffers exceed 128 MiB",
    )
}
