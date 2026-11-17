//! Per-connection MAC setup on the one shared EX frame/worker implementation.
use super::handshake::{self, BoundaryFrame, LoginReplyRules, MacSetupPolicy, SetupReply};
use crate::ex::control::Control;
use crate::ex::frame::FrameLimits;
use crate::ex::profile::{SetupFuture, SetupProfile};
use crate::ex::transport::{read_frame, write};
use crate::{Result, TimeoutPhase};
use tokio::net::TcpStream;

// Exact original MAC transport constants. No EX92-byte setup is sent here.
pub(crate) const SETUP_1: &[u8] = &[0x0c, 0x02, 0x18, 0x93, 0, 1, 3, 0, 3, 0, 0x0d, 0, 1];
pub(crate) const SETUP_2: &[u8] = &[0x0c, 0x02, 0x18, 0x94, 0, 1, 3, 0, 3, 0, 0x0d, 0, 2];
pub(crate) const SETUP_3: &[u8] = &[
    0x0c, 0x03, 0x18, 0x99, 0, 1, 0x20, 0, 0x20, 0, 0xdb, 0x0f, 0xd5, 0xd0, 0xc9, 0xcc, 0xd6, 0xa4,
    0xa8, 0xaf, 0, 0, 0, 0x8f, 0xc2, 0x25, 0x40, 0x13, 0, 0, 0xd5, 0, 0xc9, 0xcc, 0xbd, 0xf0, 0xd7,
    0xea, 0, 0, 0, 2,
];
pub(crate) const SETUP_PACKETS: [&[u8]; 3] = [SETUP_1, SETUP_2, SETUP_3];

#[derive(Debug)]
pub(crate) struct MacProfile {
    pub policy: MacSetupPolicy,
}
impl SetupProfile for MacProfile {
    fn name(&self) -> &'static str {
        "mac"
    }
    fn negotiate<'a>(
        &'a self,
        stream: &'a mut TcpStream,
        limits: FrameLimits,
        control: &'a Control,
    ) -> SetupFuture<'a> {
        Box::pin(async move {
            let steps = match &self.policy {
                MacSetupPolicy::RequireAllReplies => [SetupReply::Required; 3],
                MacSetupPolicy::OptionalWithFence(boundary) => *boundary.steps(),
            };
            for (index, packet) in SETUP_PACKETS.iter().enumerate() {
                write(stream, packet, TimeoutPhase::Handshake, control).await?;
                if steps[index] == SetupReply::Required {
                    // Read and decompress the entire reply. Partial or quiet
                    // timeout failure retires this socket in the shared worker.
                    read_frame(stream, limits, TimeoutPhase::Handshake, control).await?;
                }
            }
            if let MacSetupPolicy::OptionalWithFence(boundary) = &self.policy {
                let fence = super::commands::server_info_request()?;
                write(stream, &fence.wire, TimeoutPhase::Handshake, control).await?;
                let mut tracker = boundary.tracker();
                // At most one reply per setup plus one mandatory fence.
                for _ in 0..=3 {
                    let frame =
                        read_frame(stream, limits, TimeoutPhase::Handshake, control).await?;
                    if tracker.accept(frame.header())? == BoundaryFrame::Fence {
                        let server =
                            super::decode::server_info(frame.payload()).map_err(|error| {
                                crate::ex::control::phase_error(error, TimeoutPhase::Handshake)
                            })?;
                        if server.today().parsed().is_none() {
                            return Err(handshake::protocol(
                                "MAC setup fence lacks a valid server date",
                            ));
                        }
                        control.check(TimeoutPhase::Handshake)?;
                        return Ok(());
                    }
                }
                return Err(handshake::protocol(
                    "MAC optional setup exhausted its frame allowance before the fence",
                ));
            }
            control.check(TimeoutPhase::Handshake)
        })
    }
}
#[derive(Debug)]
pub(crate) struct MacExProfile {
    pub login: Option<LoginReplyRules>,
}
impl SetupProfile for MacExProfile {
    fn name(&self) -> &'static str {
        "mac-ex"
    }
    fn negotiate<'a>(
        &'a self,
        stream: &'a mut TcpStream,
        limits: FrameLimits,
        control: &'a Control,
    ) -> SetupFuture<'a> {
        Box::pin(async move {
            // This evidence gap is explicit, not a len>=2 authentication rule.
            let rules = self.login.as_ref().ok_or_else(|| {
                handshake::protocol("MAC-EX production login response evidence is not configured")
            })?;
            let login = super::commands::mac_ex_login_request()?;
            write(stream, &login.wire, TimeoutPhase::Handshake, control).await?;
            let frame = read_frame(stream, limits, TimeoutPhase::Handshake, control).await?;
            handshake::require_login(Some(rules), frame.payload())?;
            control.check(TimeoutPhase::Handshake)
        })
    }
}
