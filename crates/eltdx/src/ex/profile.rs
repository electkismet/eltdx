//! Explicit internal protocol selection. The profile is immutable for a
//! Session and is never inferred from an endpoint's port or market number.
use std::future::Future;
use std::pin::Pin;
use std::fmt::Debug;
use tokio::net::TcpStream;
use crate::{Result, TimeoutPhase};
use super::control::Control;
use super::frame::{FrameLimits, SETUP_REQUEST};

pub(crate) type SetupFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
/// MAC and MAC-EX supply their own complete negotiation here. In particular a
/// MAC-EX implementation can never accidentally inherit the EX92-byte setup.
pub(crate) trait SetupProfile: Debug + Send + Sync {
    fn name(&self) -> &'static str;
    fn negotiate<'a>(&'a self, stream: &'a mut TcpStream, limits: FrameLimits, control: &'a Control) -> SetupFuture<'a>;
}
#[derive(Debug)]
pub(crate) struct ExProfile;
impl SetupProfile for ExProfile {
    fn name(&self) -> &'static str { "ex" }
    fn negotiate<'a>(&'a self, stream: &'a mut TcpStream, limits: FrameLimits, control: &'a Control) -> SetupFuture<'a> {
        Box::pin(async move {
            super::transport::write(stream, &SETUP_REQUEST, TimeoutPhase::Handshake, control).await?;
            super::transport::read_frame(stream, limits, TimeoutPhase::Handshake, control).await?;
            Ok(())
        })
    }
}
