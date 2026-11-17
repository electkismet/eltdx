use super::{
    aggregate_meta, allocation_error, endpoint_engine, identity_from_market, result_meta,
    verify_cursor, ServiceSource,
};
use crate::config::ClientConfig;
use crate::hq::decode::DecodedReply;
use crate::hq::executor::HqExecutor;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions, WorkLimits};
use crate::types::{
    Collected, DataPage, EndEvidence, NumericValue, PageCursor, Price, SpecialLimit,
    SpecialLimitQuery, ValueProvenance,
};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use eltdx_protocol::commands::limits::SpecialLimitsRequest;
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_runtime::engine::{Engine, PinHandle};
use std::hash::{Hash, Hasher};
use std::time::Instant;

fn fetch_limit_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<DecodedReply>,
) -> Result<DecodedReply> {
    context.check_boundary(TimeoutPhase::Admission)?;
    if !matches!(&request, CommandRequest::SpecialLimits(_)) {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "special-limit service requires its domain request",
        ));
    }
    let reply = fetch(request)?;
    if !matches!(&reply.response, CommandResponse::SpecialLimits(_)) {
        return Err(Error::new(
            ErrorKind::Protocol,
            "invalid special-limit response kind",
        ));
    }
    Ok(reply)
}

pub(crate) fn legacy_limit_reply(
    context: &OperationContext,
    request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<DecodedReply>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            "legacy special-limit request requires a Python-profile operation",
        ));
    }
    Ok(fetch_limit_reply(context, request, fetch)?.response)
}

fn advance_special_offset(offset: u64, count: u64) -> Option<u64> {
    offset.checked_add(count)
}

pub fn legacy_special_scan<E>(
    start: u64,
    maximum: u64,
    mut query: impl FnMut(u64) -> std::result::Result<Option<u64>, E>,
    overflow: impl Fn() -> E,
) -> std::result::Result<(), E> {
    let end = start.checked_add(maximum).ok_or_else(&overflow)?;
    let mut index = start;
    while index < end {
        let Some(count) = query(index)? else {
            break;
        };
        index = advance_special_offset(index, count).ok_or_else(&overflow)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct LimitService {
    executor: HqExecutor,
    engine: Engine,
    config: ClientConfig,
}
impl LimitService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig) -> Self {
        Self {
            executor,
            engine,
            config,
        }
    }
    pub fn special(
        &self,
        query: &SpecialLimitQuery,
        options: &OperationOptions,
    ) -> Result<DataPage<SpecialLimit>> {
        let entry = Instant::now();
        let mut context =
            self.executor
                .begin_context_at(entry, "special_limits", &self.config, options)?;
        query.validate()?;
        verify_cursor(
            query.cursor(),
            fingerprint(query),
            query.pages().page_size(),
            &context,
            None,
        )?;
        context.set_work_limits(WorkLimits {
            pages: None,
            records: Some(query.pages().max_records()),
            bytes: None,
        })?;
        let engine = endpoint_engine(
            &self.executor,
            &self.config,
            &self.engine,
            query.cursor().and_then(PageCursor::endpoint),
        )?;
        let result = self.page_in_context(
            &context,
            &engine,
            None,
            query,
            query.start_index(),
            ChildPurpose::Required,
        )?;
        context.finish()?;
        Ok(result)
    }
    pub fn scan_special(
        &self,
        query: &SpecialLimitQuery,
        options: &OperationOptions,
    ) -> Result<Collected<SpecialLimit>> {
        let entry = Instant::now();
        let mut context =
            self.executor
                .begin_context_at(entry, "scan_special_limits", &self.config, options)?;
        query.validate()?;
        verify_cursor(
            query.cursor(),
            fingerprint(query),
            query.pages().page_size(),
            &context,
            None,
        )?;
        context.set_work_limits(WorkLimits {
            pages: std::num::NonZeroUsize::new(query.pages().max_pages().saturating_sub(1)),
            records: Some(query.pages().max_records()),
            bytes: None,
        })?;
        let engine = endpoint_engine(
            &self.executor,
            &self.config,
            &self.engine,
            query.cursor().and_then(PageCursor::endpoint),
        )?;
        let mut pin = self.executor.acquire_pin(&engine, &context)?;
        let result = (|| {
            let mut items = Vec::new();
            let mut metas = Vec::new();
            let mut offset = query.start_index();
            for page_index in 0..query.pages().max_pages() {
                context.check_boundary(TimeoutPhase::Admission)?;
                let page = self.page_in_context(
                    &context,
                    &engine,
                    Some(pin.handle()?),
                    query,
                    offset,
                    if page_index == 0 {
                        ChildPurpose::Required
                    } else {
                        ChildPurpose::Page
                    },
                )?;
                metas.push(page.meta().clone());
                if page.items().is_empty() {
                    return Ok(Collected::new(
                        items,
                        EndEvidence::EmptyPage,
                        page_index + 1,
                        aggregate_meta(&context, &metas, None)?,
                    ));
                }
                let total = items
                    .len()
                    .checked_add(page.items().len())
                    .ok_or_else(cap)?;
                if total > query.pages().max_records() {
                    return Err(cap());
                }
                let next = advance_special_offset(
                    u64::from(offset),
                    u64::try_from(page.items().len()).map_err(|_| cap())?,
                )
                .ok_or_else(cap)?;
                offset = u16::try_from(next).map_err(|_| cap())?;
                items
                    .try_reserve(page.items().len())
                    .map_err(allocation_error)?;
                items.extend(page.into_items());
                if items.len() == query.pages().max_records() {
                    return Err(cap());
                }
            }
            Err(cap())
        })();
        match result {
            Ok(result) => {
                self.executor.release_pin(&context, &mut pin, None)?;
                context.finish()?;
                Ok(result)
            }
            Err(root) => match self.executor.release_pin(&context, &mut pin, Some(root)) {
                Err(error) => Err(error),
                Ok(()) => Err(Error::new(
                    ErrorKind::Internal,
                    "special-limit cleanup lost its root failure",
                )),
            },
        }
    }
    fn page_in_context(
        &self,
        context: &OperationContext,
        engine: &Engine,
        pin: Option<&PinHandle>,
        query: &SpecialLimitQuery,
        offset: u16,
        purpose: ChildPurpose,
    ) -> Result<DataPage<SpecialLimit>> {
        let request = CommandRequest::SpecialLimits(SpecialLimitsRequest::new(offset));
        let reply = fetch_limit_reply(context, request, |request| {
            let raw = match pin {
                Some(pin) => self
                    .executor
                    .execute_pinned_raw(engine, pin, context, request, purpose, None)?,
                None => self
                    .executor
                    .execute_raw(engine, context, request, purpose, None)?,
            };
            self.executor.decode_received(context, raw)
        })?;
        let CommandResponse::SpecialLimits(page) = &reply.response else {
            return Err(Error::new(
                ErrorKind::Protocol,
                "invalid special-limit response kind",
            ));
        };
        if page.records.len() > query.pages().max_records() {
            return Err(cap());
        }
        let source = ServiceSource::from_reply(&reply);
        let make_meta = |ranges: &[eltdx_protocol::RecordRange]| {
            result_meta(
                context,
                &self.config,
                None,
                &source,
                Some(reply.payload.clone()),
                reply.wire.as_ref().map(|wire| wire.raw.clone()),
                ranges,
                "limits.special",
            )
        };
        let mut items = Vec::new();
        items
            .try_reserve(page.records.len())
            .map_err(allocation_error)?;
        for row in &page.records {
            if !row.upper_price_raw_f32.is_finite() || !row.lower_price_raw_f32.is_finite() {
                return Err(
                    Error::new(ErrorKind::Protocol, "non-finite special-limit price")
                        .with_context("command", "0x0452"),
                );
            }
            let id = row
                .market
                .and_then(|market| identity_from_market(market, &row.code).ok());
            let price = |value| {
                Price::with_value_and_provenance(
                    NumericValue::F32(value),
                    ValueProvenance::WireRule,
                )
            };
            items.push(SpecialLimit::new(
                id,
                row.market_id,
                row.code_num,
                price(row.upper_price_raw_f32),
                price(row.lower_price_raw_f32),
                make_meta(&[row.record_range]),
            ));
        }
        context.record_work(items.len(), 0)?;
        let meta = make_meta(
            &page
                .records
                .iter()
                .map(|row| row.record_range)
                .collect::<Vec<_>>(),
        );
        let next = u32::from(offset)
            .checked_add(u32::try_from(items.len()).map_err(|_| cap())?)
            .ok_or_else(cap)?;
        let (end, cursor) = if items.is_empty() {
            (EndEvidence::EmptyPage, None)
        } else if next > u32::from(u16::MAX) {
            (EndEvidence::RequestedWindow, None)
        } else {
            (
                EndEvidence::Unknown,
                Some(PageCursor::new(
                    next,
                    query.pages().page_size(),
                    fingerprint(query),
                    context.session_id,
                    Some(source.host.clone()),
                    None,
                )),
            )
        };
        Ok(DataPage::new(
            items,
            u64::from(offset),
            query.pages().page_size(),
            cursor,
            end,
            meta,
        ))
    }
}
fn fingerprint(query: &SpecialLimitQuery) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    "hq:0452".hash(&mut hash);
    query.pages().page_size().hash(&mut hash);
    hash.finish()
}
fn cap() -> Error {
    Error::new(
        ErrorKind::LimitExceeded,
        "special-limit collection reached its row/page/offset cap before EOF",
    )
}

#[cfg(test)]
mod legacy_tests {
    use super::*;
    #[test]
    fn legacy_scan_accepts_over_target_page_and_preserves_next_offset() {
        let mut calls = Vec::new();
        legacy_special_scan(
            4,
            3,
            |index| {
                calls.push(index);
                Ok::<_, &str>(Some(2))
            },
            || "overflow",
        )
        .unwrap();
        assert_eq!(calls, [4, 6]);
    }
    #[test]
    fn legacy_scan_empty_page_and_zero_maximum_stop_without_extra_query() {
        let mut calls = 0;
        legacy_special_scan(
            0,
            10,
            |_| {
                calls += 1;
                Ok::<_, &str>(None)
            },
            || "overflow",
        )
        .unwrap();
        assert_eq!(calls, 1);
        legacy_special_scan(
            0,
            0,
            |_| {
                calls += 1;
                Ok::<_, &str>(None)
            },
            || "overflow",
        )
        .unwrap();
        assert_eq!(calls, 1);
    }
}
