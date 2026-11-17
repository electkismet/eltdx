//! Explicit evidence for setup response boundaries and login decisions.
//!
//! The source contains setup/login requests, but no independently verified
//! response association or positive/negative login payloads. These values let
//! a verified profile supply those facts without inventing status bytes or
//! treating a quiet socket as a frame boundary. They perform no I/O.
use bytes::Bytes;
use crate::{Error, ErrorKind, Result};
use crate::ex::frame::{Association, FrameHeader};

/// Exact observed response-header fields. Constructing one is an explicit
/// caller assertion of protocol evidence, not SDK verification of a capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResponseIdentity { magic: u32, sequence: u32, method: u32 }
impl ResponseIdentity {
    pub const fn new(magic: u32, sequence: u32, method: u32) -> Self { Self { magic, sequence, method } }
    pub const fn magic(self) -> u32 { self.magic }
    pub const fn sequence(self) -> u32 { self.sequence }
    pub const fn method(self) -> u32 { self.method }
    pub const fn association(self) -> Association { Association::new(Some(self.magic),Some(self.sequence),Some(self.method)) }
    pub fn matches(self, header: FrameHeader) -> bool {
        self.magic == header.magic() && self.sequence == header.sequence() && self.method == header.method()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SetupReply {
    /// Consume one complete response before sending the next setup packet.
    Required,
    /// May be absent; consumption is deferred to a verified ordered fence.
    Optional(ResponseIdentity),
}
/// A fence is valid only for a profile whose setup replies are guaranteed to
/// precede this mandatory fence response. Timing measurements cannot establish
/// that guarantee. The constructor records the caller-supplied evidence label.
#[derive(Clone, Debug)]
pub struct OptionalSetupBoundary { steps: [SetupReply;3], fence: ResponseIdentity, evidence: String }
impl OptionalSetupBoundary {
    pub fn new(steps: [SetupReply;3], fence: ResponseIdentity, evidence: impl Into<String>) -> Result<Self> {
        let evidence = evidence.into();
        evidence_label(&evidence)?;
        let identities: Vec<_> = steps.iter().filter_map(|step| match step { SetupReply::Required => None, SetupReply::Optional(id) => Some(*id) }).collect();
        if identities.is_empty() { return Err(invalid("optional MAC boundary requires at least one optional step")); }
        for (index,id) in identities.iter().enumerate() {
            if *id == fence || identities[..index].contains(id) { return Err(invalid("MAC setup identities and fence must be disjoint")); }
        }
        // A required response after a deferred optional response cannot be
        // consumed independently without knowing which frame it belongs to.
        // Defer the complete suffix so one classifier owns that stream region.
        let mut deferred = false;
        for step in steps {
            match step {
                SetupReply::Optional(_) => deferred = true,
                SetupReply::Required if deferred => return Err(invalid("MAC optional setup steps must form a suffix before the fence")),
                SetupReply::Required => {},
            }
        }
        Ok(Self { steps, fence, evidence })
    }
    pub const fn steps(&self) -> &[SetupReply;3] { &self.steps }
    pub const fn fence(&self) -> ResponseIdentity { self.fence }
    pub fn evidence(&self) -> &str { &self.evidence }
    pub(crate) fn tracker(&self) -> BoundaryTracker { BoundaryTracker { boundary: self.clone(), seen: [false;3], fenced: false, failed: false } }
}
#[derive(Clone, Debug, Default)]
pub enum MacSetupPolicy {
    /// Conservative source-only default. Silence fails at Handshake; it does
    /// not authorize skipping to a business response on this socket.
    #[default]
    RequireAllReplies,
    OptionalWithFence(OptionalSetupBoundary),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BoundaryFrame { SetupReply(usize), Fence }
pub(crate) struct BoundaryTracker { boundary: OptionalSetupBoundary, seen: [bool;3], fenced: bool, failed: bool }
impl BoundaryTracker {
    pub(crate) fn accept(&mut self, header: FrameHeader) -> Result<BoundaryFrame> {
        if self.failed || self.fenced { return Err(protocol("MAC setup boundary is already terminal")); }
        if self.boundary.fence.matches(header) { self.fenced = true; return Ok(BoundaryFrame::Fence); }
        for (index,step) in self.boundary.steps.iter().enumerate() {
            if let SetupReply::Optional(identity) = step {
                if identity.matches(header) {
                    if self.seen[index] { self.failed = true; return Err(protocol("duplicate MAC optional setup response")); }
                    self.seen[index] = true;
                    return Ok(BoundaryFrame::SetupReply(index));
                }
            }
        }
        self.failed = true;
        Err(protocol("MAC response matches neither optional setup nor its verified fence"))
    }
}

/// Exact complete payload evidence, with an audit label. No u16 field or
/// assumed ACK is extracted from a production login response.
#[derive(Clone)]
pub struct LoginReplyRules { accepted: Vec<Bytes>, rejected: Vec<Bytes>, evidence: String }
impl std::fmt::Debug for LoginReplyRules {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("LoginReplyRules").field("accepted_patterns",&self.accepted.len())
            .field("rejected_patterns",&self.rejected.len()).field("evidence",&self.evidence).finish()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginDecision { Accepted, Rejected, Unrecognized }
impl LoginReplyRules {
    pub fn new(accepted: Vec<Bytes>, rejected: Vec<Bytes>, evidence: impl Into<String>) -> Result<Self> {
        let evidence = evidence.into(); evidence_label(&evidence)?;
        if accepted.is_empty() || accepted.len().checked_add(rejected.len()).is_none_or(|count| count > 64) {
            return Err(invalid("MAC-EX login rules require positive evidence and at most 64 complete reply patterns"));
        }
        let mut total = 0_usize;
        for reply in accepted.iter().chain(&rejected) {
            if reply.len() > usize::from(u16::MAX) { return Err(invalid("MAC-EX login pattern exceeds wire length")); }
            total = total.checked_add(reply.len()).ok_or_else(|| invalid("MAC-EX login evidence byte count overflow"))?;
        }
        if total > 2 * 1024 * 1024 || accepted.iter().any(|reply| reply.len() < 2) {
            return Err(invalid("MAC-EX accepted replies must be complete source-compatible payloads within 2 MiB"));
        }
        if accepted.iter().any(|reply| rejected.contains(reply)) { return Err(invalid("MAC-EX login acceptance and rejection evidence overlaps")); }
        Ok(Self { accepted, rejected, evidence })
    }
    pub fn evidence(&self) -> &str { &self.evidence }
    pub fn classify(&self, payload: &[u8]) -> LoginDecision {
        if self.rejected.iter().any(|reply| reply.as_ref() == payload) { LoginDecision::Rejected }
        else if self.accepted.iter().any(|reply| reply.as_ref() == payload) { LoginDecision::Accepted }
        else { LoginDecision::Unrecognized }
    }
    pub(crate) fn require_accepted(&self, payload: &[u8]) -> Result<()> {
        match self.classify(payload) {
            LoginDecision::Accepted => Ok(()),
            LoginDecision::Rejected => Err(protocol("MAC-EX login was explicitly rejected")),
            LoginDecision::Unrecognized => Err(protocol("MAC-EX login reply has no matching positive evidence")),
        }
    }
}
/// A source-only default cannot promote an unrecognized production response
/// to authentication success. The final evidence pass must supply a verified
/// production classifier; synthetic rules are used only by authored fixtures.
pub(crate) fn require_login(rules: Option<&LoginReplyRules>, payload: &[u8]) -> Result<()> {
    match rules {
        Some(rules) => rules.require_accepted(payload),
        None => Err(protocol("MAC-EX login response evidence is not configured")),
    }
}
fn evidence_label(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > 512 || value.contains('\0') {
        Err(invalid("MAC handshake evidence requires a nonempty label within 512 bytes, without NUL"))
    } else { Ok(()) }
}
fn invalid(message: &'static str) -> Error { Error::new(ErrorKind::InvalidArgument,message).with_context("domain","mac") }
pub(crate) fn protocol(message: &'static str) -> Error {
    Error::new(ErrorKind::Protocol,message).with_context("domain","mac").with_context("phase","handshake")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(method: u32) -> ResponseIdentity { ResponseIdentity::new(7_654_321,0,method) }
    fn header(method: u32) -> FrameHeader { FrameHeader::new(7_654_321,0,method,0,0) }
    fn boundary() -> OptionalSetupBoundary {
        OptionalSetupBoundary::new([SetupReply::Required,SetupReply::Optional(id(2)),SetupReply::Optional(id(3))],id(4),"synthetic ordered-fence fixture only").unwrap()
    }
    #[test]
    fn optional_absence_is_accepted_only_by_the_exact_mandatory_fence() {
        let mut tracker = boundary().tracker();
        assert_eq!(tracker.accept(header(3)).unwrap(),BoundaryFrame::SetupReply(2));
        assert_eq!(tracker.accept(header(4)).unwrap(),BoundaryFrame::Fence);
        assert!(tracker.accept(header(2)).is_err());
    }
    #[test]
    fn unknown_or_duplicate_setup_frame_retires_the_boundary() {
        let mut tracker = boundary().tracker();
        assert!(tracker.accept(header(99)).is_err());
        assert!(tracker.accept(header(4)).is_err());
        let mut tracker = boundary().tracker();
        tracker.accept(header(2)).unwrap();
        assert!(tracker.accept(header(2)).is_err());
        assert!(tracker.accept(header(4)).is_err());
    }
    #[test]
    fn overlapping_and_interleaved_optional_rules_are_rejected_before_io() {
        assert!(OptionalSetupBoundary::new([SetupReply::Required,SetupReply::Optional(id(4)),SetupReply::Optional(id(3))],id(4),"fixture").is_err());
        assert!(OptionalSetupBoundary::new([SetupReply::Optional(id(2)),SetupReply::Required,SetupReply::Optional(id(3))],id(4),"fixture").is_err());
        assert!(OptionalSetupBoundary::new([SetupReply::Required;3],id(4),"fixture").is_err());
        assert!(OptionalSetupBoundary::new([SetupReply::Required,SetupReply::Optional(id(2)),SetupReply::Optional(id(2))],id(4),"fixture").is_err());
    }
    #[test]
    fn two_bytes_alone_never_establish_login_success() {
        let rules = LoginReplyRules::new(vec![Bytes::from_static(b"fixture-positive")],vec![Bytes::from_static(b"fixture-negative")],"synthetic only; not production bytes").unwrap();
        assert_eq!(rules.classify(b"fixture-positive"),LoginDecision::Accepted);
        assert_eq!(rules.classify(b"fixture-negative"),LoginDecision::Rejected);
        assert_eq!(rules.classify(&[1,0]),LoginDecision::Unrecognized);
        assert_eq!(rules.classify(&[]),LoginDecision::Unrecognized);
        assert_eq!(rules.require_accepted(b"fixture-negative").unwrap_err().kind(),ErrorKind::Protocol);
        assert!(rules.require_accepted(&[1,0]).is_err());
    }
    #[test]
    fn login_rules_require_positive_disjoint_complete_evidence() {
        assert!(LoginReplyRules::new(vec![],vec![],"fixture").is_err());
        assert!(LoginReplyRules::new(vec![Bytes::from_static(&[1])],vec![],"fixture").is_err());
        assert!(LoginReplyRules::new(vec![Bytes::from_static(b"yes")],vec![Bytes::from_static(b"yes")],"fixture").is_err());
        assert!(LoginReplyRules::new(vec![Bytes::from_static(b"yes")],vec![]," ").is_err());
        assert!(require_login(None,b"yes").is_err());
    }
}
