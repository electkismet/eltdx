use std::time::Duration;
use bytes::Bytes;
use super::{EventTime, ResultMeta, TradingDate};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PingReply;
impl PingReply {
    pub fn message(&self) -> &str { "pong" }
    pub const fn remote_verified(&self) -> bool { false }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HandshakeInfo {
    pub(crate) server_datetime: Option<EventTime>, pub(crate) server_date1: Option<TradingDate>,
    pub(crate) server_date2: Option<TradingDate>, pub(crate) session_minutes1: Vec<String>,
    pub(crate) session_minutes2: Vec<String>, pub(crate) server_name: String, pub(crate) product_tag: String,
    pub(crate) unknown_time1: Option<u32>, pub(crate) unknown_time2: Option<u32>,
    pub(crate) flags: Bytes, pub(crate) control_tail: Bytes, pub(crate) meta: ResultMeta,
}
impl HandshakeInfo {
    pub const fn server_datetime(&self) -> Option<EventTime> { self.server_datetime }
    pub const fn server_date1(&self) -> Option<TradingDate> { self.server_date1 }
    pub const fn server_date2(&self) -> Option<TradingDate> { self.server_date2 }
    pub fn session_minutes1(&self) -> &[String] { &self.session_minutes1 }
    pub fn session_minutes2(&self) -> &[String] { &self.session_minutes2 }
    pub fn server_name(&self) -> &str { &self.server_name }
    pub fn product_tag(&self) -> &str { &self.product_tag }
    pub const fn unknown_time1(&self) -> Option<u32> { self.unknown_time1 }
    pub const fn unknown_time2(&self) -> Option<u32> { self.unknown_time2 }
    pub fn flags(&self) -> &Bytes { &self.flags }
    pub fn control_tail(&self) -> &Bytes { &self.control_tail }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeartbeatAck {
    pub(crate) server_date: Option<TradingDate>, pub(crate) server_date_raw: u32,
    pub(crate) reserved: Bytes, pub(crate) meta: ResultMeta,
}
impl HeartbeatAck {
    pub const fn server_date(&self) -> Option<TradingDate> { self.server_date }
    pub const fn server_date_raw(&self) -> u32 { self.server_date_raw }
    pub fn reserved(&self) -> &Bytes { &self.reserved }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthReport {
    pub(crate) heartbeat: HeartbeatAck, pub(crate) elapsed: Duration,
    pub(crate) endpoint: std::net::SocketAddr, pub(crate) meta: ResultMeta,
}
impl HealthReport {
    pub fn heartbeat(&self) -> &HeartbeatAck { &self.heartbeat }
    pub const fn elapsed(&self) -> Duration { self.elapsed }
    /// The actual heartbeat endpoint, also available in the source metadata.
    pub const fn endpoint(&self) -> std::net::SocketAddr { self.endpoint }
    pub fn meta(&self) -> &ResultMeta { &self.meta }
}
