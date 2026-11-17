//! Whole-file download on one existing owner/lease and exact source generation.
//! A reported size is a hint: only the observed short/empty chunk proves EOF.
use bytes::Bytes;
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use crate::operation::{ChildPurpose, OperationContext};
use crate::types::{Diagnostic, DownloadCompletion, DownloadResult, EnrichmentStatus, ResultMeta};
use crate::services::resource_legacy::DownloadCursor;
use super::{commands, decode, DownloadFileQuery};
use super::session::{self, MacOwner};

pub(crate) fn file(owner: &MacOwner, context: &OperationContext, query: &DownloadFileQuery) -> Result<DownloadResult> {
    context.check_boundary(TimeoutPhase::Admission)?;
    let policy = query.options(); let start = policy.start_offset();
    if policy.expected_size().is_some_and(|expected| start > expected) {
        return Err(super::query::invalid("MAC download starting offset exceeds its explicit expected file size"));
    }
    let maximum = usize::try_from(policy.max_bytes()).map_err(|_| limit("MAC file byte cap exceeds platform capacity"))?;
    if maximum == 0 {
        let meta = crate::services::aggregate_meta(context,&[],None)?.with_diagnostic(limited_diagnostic("max_bytes",start));
        owner.session.record_meta(meta)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        return Ok(limited(Bytes::new(),start));
    }
    // Holding the existing lease prevents another operation changing source or
    // profile between metadata and chunks; workers retain this same gate slot.
    let lease = owner.session.lease(context)?;
    let mut expected = policy.expected_size(); let mut metas = Vec::new(); let mut source = None;
    if expected.is_none() {
        let request = commands::file_meta_request(query.filename().as_str(),0)?;
        let reply = owner.exchange_leased(context,&lease,owner.mac_work(request)?,0x1215,|body,_| {
            session::decoded(decode::file_meta(&body)?,None,Vec::new())
        })?;
        expected = Some(u64::from(reply.value.size()));
        source = Some(identity(owner,context,&reply.meta)?);
        let meta = reply.meta.with_diagnostic(Diagnostic::new("mac-download","FileSizeHint",EnrichmentStatus::Succeeded,None,
            Some(format!("size={}; hash algorithm and metadata offset/flag association are unverified; size is not EOF evidence",reply.value.size()))));
        metas.push(meta);
        if expected.is_some_and(|size| start > size) { return mismatch(owner,context,&metas,expected.unwrap_or(0),start,false); }
    }
    let chunk_size = usize::try_from(policy.chunk_size().get()).map_err(|_| limit("MAC chunk exceeds platform capacity"))?;
    let mut cursor = DownloadCursor::new(chunk_size,Some(maximum)); let mut data = Vec::new(); let mut chunks = 0_u32;
    let mut completion = DownloadCompletion::LimitReached; let mut exhausted = "max_bytes";
    while let Some((relative,requested)) = cursor.request() {
        context.check_boundary(TimeoutPhase::Admission)?;
        if chunks == policy.max_chunks().get() { exhausted = "max_chunks"; break; }
        let offset = start.checked_add(u64::try_from(relative).map_err(|_| limit("MAC relative offset exceeds u64"))?)
            .ok_or_else(|| limit("MAC absolute file offset overflows"))?;
        let offset = u32::try_from(offset).map_err(|_| limit("MAC absolute file offset exceeds u32"))?;
        let requested_wire = u32::try_from(requested).map_err(|_| limit("MAC chunk request exceeds u32"))?;
        let index = chunks.checked_add(1).ok_or_else(|| limit("MAC chunk index overflows"))?;
        // Reserve accepted output capacity before requesting/reading this chunk.
        // The transport additionally charges its own incoming/retained buffers.
        context.record_work(0,requested)?;
        data.try_reserve_exact(requested).map_err(crate::services::allocation_error)?;
        let request = commands::download_chunk_request(query.filename().as_str(),index,offset,requested_wire)?;
        let work = match source { Some(source) => owner.pinned_mac_work(request,source)?,None => owner.mac_work(request)? };
        let purpose = if chunks == 0 { ChildPurpose::Required } else { ChildPurpose::Page };
        let sponsor = context.sponsor();
        let reply = owner.exchange_leased_with_purpose(context,&lease,work,0x1217,purpose,move |body,_| {
            let chunk = decode::download_chunk_with(&body,requested_wire,|length| sponsor.record_bytes(length))?;
            let ranges = if chunk.bytes().is_empty() { Vec::new() } else { vec![(8,8+chunk.bytes().len())] };
            session::decoded(chunk,None,ranges)
        }).map_err(|error| {
            // Preserve every accepted observation even if a later pinned read
            // fails. The error kind/cause is the original transport/decoder's.
            if let Ok(meta) = crate::services::aggregate_meta(context,&metas,None) {
                let meta = meta.with_diagnostic(Diagnostic::new("mac-download","ChunkFailed",EnrichmentStatus::Failed,
                    Some(error.kind()),Some(format!("index={index}; offset={offset}; accepted_bytes={}; pinned_source={source:?}; no EOF was observed",data.len()))));
                let _ = owner.session.record_meta(meta);
            }
            error
        })?;
        let observed = identity(owner,context,&reply.meta)?;
        if source.is_some_and(|expected| expected != observed) {
            return Err(Error::new(ErrorKind::Protocol,"MAC file source changed during its pinned download").with_context("reason","SourceChanged"));
        }
        source.get_or_insert(observed);
        let chunk = reply.value.into_bytes(); let count = chunk.len();
        context.check_boundary(TimeoutPhase::Aggregate)?;
        data.extend_from_slice(&chunk);
        cursor.advance(count).ok_or_else(|| limit("MAC file download cursor overflows"))?;
        chunks = index; metas.try_reserve(1).map_err(crate::services::allocation_error)?; metas.push(reply.meta);
        if count < requested {
            let observed_size = start.checked_add(u64::try_from(cursor.offset()).map_err(|_| limit("MAC file size exceeds u64"))?)
                .ok_or_else(|| limit("MAC observed file size overflows"))?;
            if let Some(expected) = expected {
                if observed_size != expected { return mismatch(owner,context,&metas,expected,observed_size,true); }
            }
            completion = DownloadCompletion::Complete; break;
        }
    }
    let next = start.checked_add(u64::try_from(cursor.offset()).map_err(|_| limit("MAC file continuation exceeds u64"))?)
        .ok_or_else(|| limit("MAC file continuation overflows"))?;
    let mut meta = crate::services::aggregate_meta(context,&metas,None)?;
    if completion == DownloadCompletion::LimitReached { meta = meta.with_diagnostic(limited_diagnostic(exhausted,next)); }
    else { meta = meta.with_diagnostic(Diagnostic::new("mac-download","ObservedEof",EnrichmentStatus::Succeeded,None,
        Some(format!("observed_size={next}; chunks={chunks}; short/empty response proves EOF; no digest verification is asserted")))); }
    owner.session.record_meta(meta)?;
    context.check_boundary(TimeoutPhase::Aggregate)?;
    lease.accept();
    Ok(DownloadResult { data: Bytes::from(data),completion,next_offset: (completion == DownloadCompletion::LimitReached).then_some(next) })
}
fn identity(owner: &MacOwner, context: &OperationContext, meta: &ResultMeta) -> Result<(crate::ex::hosts::Endpoint,u64)> {
    let source = owner.session.connection_identity()?.ok_or_else(|| Error::new(ErrorKind::Protocol,
        "MAC download has no ready source after its full response").with_context("reason","SourceChanged"))?;
    if meta.source().endpoint() != Some(source.0) || meta.source().session_id() != format!("{}:{}",context.session_id,source.1) {
        return Err(Error::new(ErrorKind::Protocol,"MAC download response does not match its ready source").with_context("reason","SourceChanged"));
    }
    Ok(source)
}
fn mismatch<T>(owner: &MacOwner, context: &OperationContext, metas: &[ResultMeta], expected: u64, observed: u64, eof_observed: bool) -> Result<T> {
    let meta = crate::services::aggregate_meta(context,metas,None)?.with_diagnostic(Diagnostic::new("mac-download",
        "ExpectedSizeMismatch",EnrichmentStatus::Failed,Some(ErrorKind::Protocol),Some(format!("expected_size={expected}; observed_position={observed}; eof_observed={eof_observed}"))));
    owner.session.record_meta(meta)?;
    Err(Error::new(ErrorKind::Protocol,"MAC observed file position differs from its expected size")
        .with_context("reason","ExpectedSizeMismatch").with_context("expected_size",format!("{expected}"))
        .with_context("observed_size",format!("{observed}")).with_context("eof_observed",format!("{eof_observed}")))
}
fn limited(data: Bytes, next: u64) -> DownloadResult { DownloadResult { data,completion: DownloadCompletion::LimitReached,next_offset: Some(next) } }
fn limited_diagnostic(limit: &'static str, next: u64) -> Diagnostic {
    Diagnostic::new("mac-download","LimitReached",EnrichmentStatus::Unavailable,Some(ErrorKind::LimitExceeded),Some(format!("limit={limit}; next_offset={next}; no EOF was observed")))
}
fn limit(message: &'static str) -> Error { Error::new(ErrorKind::LimitExceeded,message).with_context("domain","mac-download") }
