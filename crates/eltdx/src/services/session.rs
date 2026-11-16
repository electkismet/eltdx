use std::time::Instant;
use eltdx_protocol::{CommandRequest, CommandResponse};
use eltdx_protocol::commands::session::{HandshakeRequest, HeartbeatRequest};
use eltdx_runtime::engine::Engine;
use crate::config::ClientConfig;
use crate::hq::executor::HqExecutor;
use crate::hq::decode::DecodedReply;
use crate::operation::{ChildPurpose, OperationContext, OperationOptions};
use crate::types::{EventTime, HandshakeInfo, HeartbeatAck, PingReply, TradingDate};
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use super::reply_meta;

fn fetch_session_reply(
    context: &OperationContext, request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<DecodedReply>,
) -> Result<DecodedReply> {
    let handshake = match &request {
        CommandRequest::Handshake(_) => true,
        CommandRequest::Heartbeat(_) => false,
        _ => return Err(Error::new(ErrorKind::InvalidArgument, "session service requires a session command")),
    };
    context.check_boundary(TimeoutPhase::Admission)?;
    let reply = fetch(request)?;
    if !matches!((handshake, &reply.response),
        (true, CommandResponse::Handshake(_)) | (false, CommandResponse::Heartbeat(_))) {
        return Err(unexpected());
    }
    context.check_boundary(TimeoutPhase::Decode)?;
    Ok(reply)
}

pub(crate) fn legacy_session_reply(
    context: &OperationContext, request: CommandRequest,
    fetch: impl FnOnce(CommandRequest) -> Result<DecodedReply>,
) -> Result<CommandResponse> {
    if !context.is_legacy() {
        return Err(Error::new(ErrorKind::InvalidArgument, "legacy session requires a compatibility operation"));
    }
    Ok(fetch_session_reply(context, request, fetch)?.response)
}

#[derive(Clone, Debug)]
pub struct SessionService { executor: HqExecutor, engine: Engine, config: ClientConfig }
impl SessionService {
    pub(crate) fn new(executor: HqExecutor, engine: Engine, config: ClientConfig) -> Self { Self { executor, engine, config } }
    /// Local diagnostic, available after close and without starting a connection.
    pub fn ping(&self) -> Result<PingReply> { Ok(PingReply) }

    pub fn handshake(&self, options: &OperationOptions) -> Result<HandshakeInfo> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "handshake", &self.config, options)?;
        let result = self.handshake_in_context(&context)?;
        context.finish()?; Ok(result)
    }

    pub(crate) fn handshake_in_context(&self, context: &OperationContext) -> Result<HandshakeInfo> {
        let reply = self.fetch(context, CommandRequest::Handshake(HandshakeRequest))?;
        let CommandResponse::Handshake(value) = &reply.response else { return Err(unexpected()); };
        let server_datetime = value.server_datetime.and_then(|value| date(value.date)
            .and_then(|date| EventTime::local(date, value.hour, value.minute, value.second).ok()));
        let mut meta = reply_meta(context, &self.config, &reply, &[], "session.handshake");
        if let Some(date) = server_datetime.and_then(|value| value.date()).or_else(|| value.server_date_1.and_then(date)) {
            meta = meta.with_trading_date(date);
        }
        let result = HandshakeInfo { server_datetime,
            server_date1: value.server_date_1.and_then(date), server_date2: value.server_date_2.and_then(date),
            session_minutes1: value.session_minutes_1.clone(), session_minutes2: value.session_minutes_2.clone(),
            server_name: value.server_name.clone(), product_tag: value.product_tag.clone(),
            unknown_time1: Some(value.unknown_time_1_raw), unknown_time2: Some(value.unknown_time_2_raw),
            flags: value.flags_raw.clone(), control_tail: value.tail_control_raw.clone(),
            meta };
        Ok(result)
    }

    pub fn heartbeat(&self, options: &OperationOptions) -> Result<HeartbeatAck> {
        let entry = Instant::now();
        let context = self.executor.begin_context_at(entry, "heartbeat", &self.config, options)?;
        let result = self.heartbeat_in_context(&context)?;
        context.finish()?; Ok(result)
    }

    pub(crate) fn heartbeat_in_context(&self, context: &OperationContext) -> Result<HeartbeatAck> {
        let reply = self.fetch(context, CommandRequest::Heartbeat(HeartbeatRequest))?;
        let CommandResponse::Heartbeat(value) = &reply.response else { return Err(unexpected()); };
        let server_date = value.server_date.and_then(date);
        let mut meta = reply_meta(context, &self.config, &reply, &[], "session.heartbeat");
        if let Some(date) = server_date { meta = meta.with_trading_date(date); }
        Ok(HeartbeatAck { server_date, server_date_raw: value.server_date_raw, reserved: value.reserved.clone(), meta })
    }

    fn fetch(&self, context: &OperationContext, request: CommandRequest) -> Result<DecodedReply> {
        fetch_session_reply(context, request, |request| {
            let raw = self.executor.execute_raw(&self.engine, context, request, ChildPurpose::Required, None)?;
            self.executor.decode_unpriced(context, raw)
        })
    }
}
fn date(value: eltdx_protocol::unit::DateParts) -> Option<TradingDate> {
    u16::try_from(value.year).ok().and_then(|year| TradingDate::new(year, value.month, value.day).ok())
}
fn unexpected() -> Error { Error::new(ErrorKind::Protocol, "unexpected HQ session response") }
