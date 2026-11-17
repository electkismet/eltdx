use super::{allocation_error, reply_meta};
use crate::config::ClientConfig;
use crate::encoding::{scan_gbk, InvalidBytePolicy};
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions};
use crate::types::{
    CompanySection, CompanySectionQuery, CompanyText, Diagnostic, EnrichmentStatus, SecurityId,
    Table,
};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use bytes::Bytes;
use eltdx_protocol::commands::corporate::{
    company_info_content_bytes, CompanyInfoCategoryRequest, CompanyInfoContentRequest,
    COMPANY_INFO_RECORD_SIZE,
};
use eltdx_protocol::unit::Market;
use eltdx_protocol::{CommandRequest, CommandResponse, RecordRange};
use eltdx_runtime::engine::Engine;
use std::time::Instant;

/// The shared domain boundary keeps the original business bytes available for
/// Rust's diagnostic text projection even when its raw policy omits buffers.
fn fetch_company_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<(DecodedReply, Bytes)>,
) -> Result<(DecodedReply, Bytes)> {
    let category = match &request {
        CommandRequest::CompanyInfoCategory(_) => true,
        CommandRequest::CompanyInfoContent(_) => false,
        _ => {
            return Err(Error::new(
                ErrorKind::InvalidArgument,
                "company info requires a company-domain command",
            ))
        }
    };
    context.check_boundary(TimeoutPhase::Admission)?;
    let (reply, business) = fetch(request)?;
    if !matches!(
        (category, &reply.response),
        (true, CommandResponse::CompanyInfoCategory(_))
            | (false, CommandResponse::CompanyInfoContent(_))
    ) {
        return Err(Error::new(
            ErrorKind::Protocol,
            "unexpected HQ company info response",
        ));
    }
    context.check_boundary(TimeoutPhase::Decode)?;
    Ok((reply, business))
}

pub(crate) fn legacy_company_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<(DecodedReply, Bytes)>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "legacy company info requires a compatibility operation",
        ));
    }
    // Preserve the original decoder's ignore/NUL/trim policy. New Rust's lossy
    // diagnostics, range-overflow and requested-length checks are separate.
    Ok(fetch_company_reply(context, request, fetch)?.0.response)
}

#[derive(Clone, Debug)]
pub struct CompanyInfoService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
}
impl CompanyInfoService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig) -> Self {
        Self {
            executor,
            engine,
            config,
        }
    }
    pub fn category(
        &self,
        id: &SecurityId,
        options: &OperationOptions,
    ) -> Result<Table<CompanySection>> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(
            entry,
            "company_info_category",
            &self.config,
            options,
        )?;
        let request = CommandRequest::CompanyInfoCategory(
            CompanyInfoCategoryRequest::new(market(id)?, id.number())
                .map_err(Error::from_protocol)?,
        );
        let (reply, business) = self.fetch(&context, request)?;
        // The shared protocol decoder has validated the count and full fixed
        // record extent. Decode names here with the shared strict GBK scanner
        // so invalid bytes remain visible to Rust callers even under Omit.
        let count = usize::from(u16::from_le_bytes([business[0], business[1]]));
        let mut rows = Vec::new();
        let mut ranges = Vec::new();
        let mut lossy = false;
        rows.try_reserve_exact(count).map_err(allocation_error)?;
        ranges.try_reserve_exact(count).map_err(allocation_error)?;
        for index in 0..count {
            context.check_boundary(TimeoutPhase::Decode)?;
            let start = 2 + index * COMPANY_INFO_RECORD_SIZE;
            let record = &business[start..start + COMPANY_INFO_RECORD_SIZE];
            let (name, name_loss) = text(&record[..64], true, &context)?;
            let (filename, filename_loss) = text(&record[64..144], true, &context)?;
            lossy |= name_loss || filename_loss;
            let offset = u32::from_le_bytes([record[144], record[145], record[146], record[147]]);
            let length = u32::from_le_bytes([record[148], record[149], record[150], record[151]]);
            if offset.checked_add(length).is_none() {
                return Err(Error::new(
                    ErrorKind::Protocol,
                    "company catalog byte range overflows u32",
                ));
            }
            rows.push(CompanySection::new(name, filename, offset, length));
            ranges.push(range(start, COMPANY_INFO_RECORD_SIZE));
        }
        context.record_work(count, 0)?;
        let mut meta = reply_meta(
            &context,
            &self.config,
            &reply,
            &ranges,
            "company_info.category",
        );
        if lossy {
            meta = meta.with_diagnostic(loss_diagnostic());
        }
        context.finish()?;
        Ok(Table::new(rows, meta))
    }

    pub fn content(
        &self,
        id: &SecurityId,
        query: &CompanySectionQuery,
        options: &OperationOptions,
    ) -> Result<CompanyText> {
        let entry = Instant::now();
        let context =
            self.executor
                .begin_context_at(entry, "company_info_content", &self.config, options)?;
        query.validate()?;
        let filename = crate::encoding::encode_gbk_bounded(query.filename(), 80)?;
        let request = CommandRequest::CompanyInfoContent(
            CompanyInfoContentRequest::from_encoded_filename(
                market(id)?,
                id.number(),
                filename,
                query.offset(),
                query.length(),
            )
            .map_err(Error::from_protocol)?,
        );
        let (reply, business) = self.fetch(&context, request)?;
        let bytes = company_info_content_bytes(&business).map_err(Error::from_protocol)?;
        if bytes.len() as u64 > u64::from(query.length()) {
            return Err(Error::new(
                ErrorKind::Protocol,
                "company content exceeds requested byte range",
            ));
        }
        let (text, decoding_had_errors) = text(bytes, false, &context)?;
        let mut meta = reply_meta(
            &context,
            &self.config,
            &reply,
            &[range(12, bytes.len())],
            "company_info.content",
        );
        if decoding_had_errors {
            meta = meta.with_diagnostic(loss_diagnostic());
        }
        let result = CompanyText {
            id: id.clone(),
            filename: query.filename().to_owned(),
            offset: query.offset(),
            requested_length: query.length(),
            text,
            decoding_had_errors,
            meta,
        };
        context.finish()?;
        Ok(result)
    }

    fn fetch(
        &self,
        context: &OperationContext,
        request: CommandRequest,
    ) -> Result<(DecodedReply, Bytes)> {
        fetch_company_reply(context, request, |request| {
            let raw = self.executor.execute_raw(
                &self.engine,
                context,
                request,
                ChildPurpose::Required,
                None,
            )?;
            let business = raw.response.data.clone();
            Ok((self.executor.decode_unpriced(context, raw)?, business))
        })
    }
}

fn market(id: &SecurityId) -> Result<Market> {
    Market::normalize(id.market().code()).map_err(Error::from_protocol)
}
fn range(offset: usize, len: usize) -> RecordRange {
    RecordRange {
        offset,
        len,
        encoding: eltdx_protocol::context::RecordEncoding::Payload,
    }
}
fn text(bytes: &[u8], nul_terminated: bool, context: &OperationContext) -> Result<(String, bool)> {
    let bytes = if nul_terminated {
        &bytes[..bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len())]
    } else {
        bytes
    };
    let mut text = String::new();
    text.try_reserve_exact(bytes.len().checked_mul(3).ok_or_else(|| {
        Error::new(
            ErrorKind::LimitExceeded,
            "company text allocation overflows",
        )
    })?)
    .map_err(allocation_error)?;
    let lossy = scan_gbk(
        bytes,
        InvalidBytePolicy::Replace,
        |scalar| text.push(scalar),
        &mut || context.check_boundary(TimeoutPhase::Decode),
    )?;
    Ok((
        if nul_terminated {
            text.trim().to_owned()
        } else {
            text
        },
        lossy,
    ))
}
fn loss_diagnostic() -> Diagnostic {
    Diagnostic::new(
        "company_info",
        "gbk",
        EnrichmentStatus::Unavailable,
        Some(ErrorKind::Protocol),
        Some("invalid source GBK bytes were replaced; offsets remain byte based".to_owned()),
    )
}
