//! EX framing without I/O. Caller must serialize the full request/reply and
//! retire the connection after any framing failure before another operation.
use std::sync::atomic::{AtomicU8, Ordering};
use bytes::Bytes;
use flate2::{Decompress, FlushDecompress, Status};
use crate::{Error, ErrorKind, Result};
use super::{invalid, protocol};

pub const HEADER_SIZE: usize = 16;
pub const WIRE_BODY_LIMIT: usize = u16::MAX as usize;
/// Python's frame encoder constant. Production response acceptance is explicitly
/// profile-selected; no response association capture is bundled in Stage A.
pub const ENCODED_MAGIC: u32 = 7_654_321;
pub const SETUP_REQUEST: [u8; 92] = [
    0x01,0x01,0x48,0x65,0x00,0x01,0x52,0x00,0x52,0x00,0x54,0x24,
    0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,
    0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,
    0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,
    0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,0x1f,0x32,0xc6,0xe5,0xd5,0x3d,0xfb,0x41,
    0xcc,0xe1,0x6d,0xff,0xd5,0xba,0x3f,0xb8,0xcb,0xc5,0x7a,0x05,0x4f,0x77,0x48,0xea,
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameHeader { magic: u32, sequence: u32, method: u32, compressed_len: u16, decoded_len: u16 }
impl FrameHeader {
    pub const fn new(magic: u32, sequence: u32, method: u32, compressed_len: u16, decoded_len: u16) -> Self {
        Self { magic, sequence, method, compressed_len, decoded_len }
    }
    pub fn parse(raw: &[u8]) -> Result<Self> {
        if raw.len() != HEADER_SIZE { return Err(protocol("EX header must contain exactly 16 bytes")); }
        Ok(Self {
            magic: u32::from_le_bytes([raw[0],raw[1],raw[2],raw[3]]),
            sequence: u32::from_le_bytes([raw[4],raw[5],raw[6],raw[7]]),
            method: u32::from_le_bytes([raw[8],raw[9],raw[10],raw[11]]),
            compressed_len: u16::from_le_bytes([raw[12],raw[13]]), decoded_len: u16::from_le_bytes([raw[14],raw[15]]),
        })
    }
    pub fn encode(self) -> [u8; HEADER_SIZE] {
        let mut output = [0; HEADER_SIZE];
        output[0..4].copy_from_slice(&self.magic.to_le_bytes());
        output[4..8].copy_from_slice(&self.sequence.to_le_bytes());
        output[8..12].copy_from_slice(&self.method.to_le_bytes());
        output[12..14].copy_from_slice(&self.compressed_len.to_le_bytes());
        output[14..16].copy_from_slice(&self.decoded_len.to_le_bytes()); output
    }
    pub const fn magic(self) -> u32 { self.magic }
    pub const fn sequence(self) -> u32 { self.sequence }
    pub const fn method(self) -> u32 { self.method }
    pub const fn compressed_len(self) -> u16 { self.compressed_len }
    pub const fn decoded_len(self) -> u16 { self.decoded_len }
    pub fn frame_len(self) -> usize { HEADER_SIZE + usize::from(self.compressed_len) }
}
#[derive(Clone, Copy, Debug)]
pub struct FrameLimits { max_encoded: u16, max_decoded: u16 }
impl Default for FrameLimits { fn default() -> Self { Self { max_encoded: u16::MAX, max_decoded: u16::MAX } } }
impl FrameLimits {
    pub fn new(max_encoded: usize, max_decoded: usize) -> Result<Self> {
        if max_encoded == 0 || max_decoded == 0 { return Err(invalid("EX frame limits must be positive")); }
        Ok(Self { max_encoded: u16::try_from(max_encoded).map_err(|_| invalid("EX encoded cap exceeds wire limit"))?,
            max_decoded: u16::try_from(max_decoded).map_err(|_| invalid("EX decoded cap exceeds wire limit"))? })
    }
    pub const fn max_encoded(self) -> u16 { self.max_encoded }
    pub const fn max_decoded(self) -> u16 { self.max_decoded }
    pub fn validate(self, header: FrameHeader) -> Result<()> {
        if header.compressed_len > self.max_encoded || header.decoded_len > self.max_decoded {
            return Err(Error::new(ErrorKind::LimitExceeded, "EX frame exceeds configured cap").with_context("domain", "ex"));
        }
        Ok(())
    }
}
/// Caller-selected, evidence-backed association expectations. None records an
/// unverified rule; it is never silently advertised as verified response matching.
#[derive(Clone, Copy, Debug, Default)]
pub struct Association { magic: Option<u32>, sequence: Option<u32>, method: Option<u32> }
impl Association {
    pub const fn new(magic: Option<u32>, sequence: Option<u32>, method: Option<u32>) -> Self { Self { magic, sequence, method } }
    pub const fn magic(self) -> Option<u32> { self.magic }
    pub const fn sequence(self) -> Option<u32> { self.sequence }
    pub const fn method(self) -> Option<u32> { self.method }
    pub const fn is_fully_verified(self) -> bool { self.magic.is_some() && self.sequence.is_some() && self.method.is_some() }
    pub fn validate(self, header: FrameHeader) -> Result<()> {
        if self.magic.is_some_and(|value| value != header.magic)
            || self.sequence.is_some_and(|value| value != header.sequence)
            || self.method.is_some_and(|value| value != header.method) {
            return Err(protocol("EX response association mismatch"));
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Frame { header: FrameHeader, payload: Bytes, wire: Bytes, association_verified: bool }
impl Frame {
    pub const fn header(&self) -> FrameHeader { self.header }
    pub fn payload(&self) -> &Bytes { &self.payload }
    pub fn wire(&self) -> &Bytes { &self.wire }
    pub const fn association_verified(&self) -> bool { self.association_verified }
    pub fn decode(wire: Bytes, limits: FrameLimits, association: Association) -> Result<Self> {
        if wire.len() < HEADER_SIZE { return Err(protocol("truncated EX header")); }
        let header = FrameHeader::parse(&wire[..HEADER_SIZE])?;
        limits.validate(header)?; association.validate(header)?;
        if wire.len() != header.frame_len() { return Err(protocol("EX frame body length mismatch")); }
        let payload = decode_body(header, wire.slice(HEADER_SIZE..), limits)?;
        Ok(Self { header, payload, wire, association_verified: association.is_fully_verified() })
    }
}
pub fn decode_body(header: FrameHeader, encoded: Bytes, limits: FrameLimits) -> Result<Bytes> {
    limits.validate(header)?;
    if encoded.len() != usize::from(header.compressed_len) { return Err(protocol("EX encoded body length mismatch")); }
    let size = usize::from(header.decoded_len);
    if header.compressed_len == header.decoded_len { return Ok(encoded); }
    // One spare byte detects a stream that produces more than its declared size.
    // Allocation is bounded by 65536, including that detection byte.
    let mut output = vec![0; size + 1];
    let mut decoder = Decompress::new(true);
    let status = decoder.decompress(&encoded, &mut output, FlushDecompress::Finish)
        .map_err(|error| protocol("invalid EX zlib body").with_source(error))?;
    if status != Status::StreamEnd || decoder.total_in() != encoded.len() as u64 || decoder.total_out() != size as u64 {
        return Err(protocol("EX zlib stream EOF, trailing data or declared length mismatch"));
    }
    output.truncate(size); Ok(Bytes::from(output))
}
/// Incremental one-frame accumulator. push consumes only bytes belonging to this
/// frame and reports that count, so coalesced next-frame bytes stay with caller.
#[derive(Debug)]
pub struct FrameAccumulator { raw: Vec<u8>, header: Option<FrameHeader>, limits: FrameLimits, association: Association, failed: bool }
impl FrameAccumulator {
    pub fn new(limits: FrameLimits, association: Association) -> Self {
        Self { raw: Vec::with_capacity(HEADER_SIZE), header: None, limits, association, failed: false }
    }
    pub fn push(&mut self, input: &[u8]) -> Result<usize> {
        if self.failed { return Err(protocol("EX accumulator is retired after failure")); }
        let result = self.push_inner(input);
        if result.is_err() { self.failed = true; }
        result
    }
    fn push_inner(&mut self, input: &[u8]) -> Result<usize> {
        let mut used = 0;
        if self.header.is_none() {
            used = input.len().min(HEADER_SIZE - self.raw.len());
            self.raw.extend_from_slice(&input[..used]);
            if self.raw.len() < HEADER_SIZE { return Ok(used); }
            let header = FrameHeader::parse(&self.raw)?;
            self.limits.validate(header)?; self.association.validate(header)?;
            self.header = Some(header);
        }
        if let Some(header) = self.header {
            let count = (input.len() - used).min(header.frame_len() - self.raw.len());
            self.raw.extend_from_slice(&input[used..used + count]); used += count;
        }
        Ok(used)
    }
    pub fn remaining(&self) -> usize {
        self.header.map_or(HEADER_SIZE, FrameHeader::frame_len).saturating_sub(self.raw.len())
    }
    /// EOF before completion is a protocol failure. A successful finish validates
    /// compressed setup responses exactly like business responses.
    pub fn finish(self) -> Result<Frame> {
        if self.failed { return Err(protocol("EX accumulator is retired after failure")); }
        Frame::decode(Bytes::from(self.raw), self.limits, self.association)
    }
}
/// One atomic counter per owner session. Holding a lease prevents a second range
/// request from allocating a sequence while one is in flight; transport still
/// owns the single full-execution gate for all commands.
#[derive(Debug)]
pub struct RangeSequence { next: AtomicU8, in_flight: std::sync::atomic::AtomicBool }
impl Default for RangeSequence {
    fn default() -> Self { Self { next: AtomicU8::new(1), in_flight: std::sync::atomic::AtomicBool::new(false) } }
}
impl RangeSequence {
    pub fn acquire(&self) -> Result<SequenceLease<'_>> {
        self.in_flight.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Error::new(ErrorKind::PoolBusy, "EX range sequence already in flight").with_context("domain", "ex"))?;
        let sequence = self.next.load(Ordering::Relaxed);
        self.next.store(if sequence == 255 { 1 } else { sequence + 1 }, Ordering::Relaxed);
        Ok(SequenceLease { owner: self, sequence })
    }
}
#[derive(Debug)]
pub struct SequenceLease<'a> { owner: &'a RangeSequence, sequence: u8 }
impl SequenceLease<'_> { pub const fn sequence(&self) -> u8 { self.sequence } }
impl Drop for SequenceLease<'_> { fn drop(&mut self) { self.owner.in_flight.store(false, Ordering::Release); } }

/// A protocol choice is supplied by the owner, never inferred from endpoint
/// port 7727. MAC-EX owns its login/body rules in T25; it may reuse EX framing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WireProfile { Ex, MacEx }
impl WireProfile {
    pub const fn setup_request(self) -> Option<&'static [u8]> {
        match self { Self::Ex => Some(&SETUP_REQUEST), Self::MacEx => None }
    }
}
