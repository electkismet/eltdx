use std::collections::VecDeque;
use std::sync::Arc;

use eltdx_protocol::frame::ResponseFrame;

use crate::error::RuntimeError;
use crate::slot::{EngineEpoch, GenerationId, SlotId};

pub const DEFAULT_PUSH_MAX_FRAMES: usize = 1_024;
pub const DEFAULT_PUSH_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PushFrame {
    pub engine_epoch: EngineEpoch,
    pub slot_id: SlotId,
    pub generation: GenerationId,
    pub connected_host: Arc<str>,
    pub response: ResponseFrame,
}

impl PushFrame {
    pub fn wire_size(&self) -> usize {
        if self.response.raw.is_empty() {
            16_usize.saturating_add(usize::from(self.response.zip_length))
        } else {
            self.response.raw.len()
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PushBufferSnapshot {
    pub owner_epoch: EngineEpoch,
    pub frame_count: usize,
    pub byte_count: usize,
    pub max_frames: usize,
    pub max_bytes: usize,
    pub max_frames_observed: usize,
    pub max_bytes_observed: usize,
    pub dropped_total: u64,
    pub gap_pending: bool,
    pub closed: bool,
}

#[derive(Debug)]
pub struct PushBuffer {
    owner_epoch: EngineEpoch,
    max_frames: usize,
    max_bytes: usize,
    frames: VecDeque<PushFrame>,
    bytes: usize,
    max_frames_observed: usize,
    max_bytes_observed: usize,
    dropped_total: u64,
    reported_dropped_total: u64,
    accepting: bool,
    closed: bool,
    fatal: Option<RuntimeError>,
}

impl PushBuffer {
    pub fn new(
        owner_epoch: EngineEpoch,
        max_frames: usize,
        max_bytes: usize,
    ) -> Result<Self, RuntimeError> {
        if max_frames == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "push_queue_size must be > 0",
            ));
        }
        if max_bytes == 0 {
            return Err(RuntimeError::invalid_argument(
                "ValueError",
                "push_queue_bytes must be > 0",
            ));
        }
        Ok(Self {
            owner_epoch,
            max_frames,
            max_bytes,
            frames: VecDeque::new(),
            bytes: 0,
            max_frames_observed: 0,
            max_bytes_observed: 0,
            dropped_total: 0,
            reported_dropped_total: 0,
            accepting: true,
            closed: false,
            fatal: None,
        })
    }

    pub fn offer(&mut self, frame: PushFrame) -> bool {
        if !self.accepting || self.closed || frame.engine_epoch != self.owner_epoch {
            return false;
        }
        let size = frame.wire_size();
        let mut dropped = 0_u64;
        while self.frames.front().is_some()
            && (self.frames.len() >= self.max_frames
                || self.bytes.saturating_add(size) > self.max_bytes)
        {
            if let Some(oldest) = self.frames.pop_front() {
                self.bytes = self.bytes.saturating_sub(oldest.wire_size());
                dropped = dropped.saturating_add(1);
            }
        }
        let accepted = if size > self.max_bytes {
            dropped = dropped.saturating_add(1);
            false
        } else {
            self.frames.push_back(frame);
            self.bytes = self.bytes.saturating_add(size);
            true
        };
        self.record_drop_count(dropped);
        self.max_frames_observed = self.max_frames_observed.max(self.frames.len());
        self.max_bytes_observed = self.max_bytes_observed.max(self.bytes);
        accepted
    }

    pub fn record_external_drop(&mut self, epoch: EngineEpoch, count: u64) -> bool {
        if !self.accepting || self.closed || epoch != self.owner_epoch {
            return false;
        }
        self.record_drop_count(count);
        true
    }

    pub fn poll(&mut self) -> Result<Option<PushFrame>, RuntimeError> {
        self.raise_fatal_or_gap()?;
        let Some(frame) = self.frames.pop_front() else {
            return Ok(None);
        };
        self.bytes = self.bytes.saturating_sub(frame.wire_size());
        Ok(Some(frame))
    }

    pub fn drain(&mut self) -> Result<Vec<PushFrame>, RuntimeError> {
        self.raise_fatal_or_gap()?;
        let frames = self.frames.drain(..).collect();
        self.bytes = 0;
        Ok(frames)
    }

    pub fn close(&mut self, epoch: EngineEpoch, fatal: Option<RuntimeError>) -> bool {
        if epoch != self.owner_epoch {
            return false;
        }
        self.accepting = false;
        if self.fatal.is_none() {
            self.fatal = fatal;
        }
        self.frames.clear();
        self.bytes = 0;
        self.reported_dropped_total = self.dropped_total;
        self.closed = true;
        true
    }

    pub fn snapshot(&self) -> PushBufferSnapshot {
        PushBufferSnapshot {
            owner_epoch: self.owner_epoch,
            frame_count: self.frames.len(),
            byte_count: self.bytes,
            max_frames: self.max_frames,
            max_bytes: self.max_bytes,
            max_frames_observed: self.max_frames_observed,
            max_bytes_observed: self.max_bytes_observed,
            dropped_total: self.dropped_total,
            gap_pending: self.dropped_total > self.reported_dropped_total,
            closed: self.closed,
        }
    }

    pub fn check_invariants(&self) -> Result<(), RuntimeError> {
        if self.frames.len() > self.max_frames
            || self.bytes > self.max_bytes
            || self.max_frames_observed > self.max_frames
            || self.max_bytes_observed > self.max_bytes
        {
            return Err(RuntimeError::internal(
                "push buffer capacity invariant failed",
            ));
        }
        let actual_bytes = self
            .frames
            .iter()
            .try_fold(0_usize, |total, frame| total.checked_add(frame.wire_size()))
            .ok_or_else(|| RuntimeError::internal("push buffer byte accounting overflow"))?;
        if actual_bytes != self.bytes {
            return Err(RuntimeError::internal(
                "push buffer byte count does not match resident frames",
            ));
        }
        if self.reported_dropped_total > self.dropped_total {
            return Err(RuntimeError::internal(
                "reported push drops exceed total drops",
            ));
        }
        if self.closed && (self.accepting || !self.frames.is_empty() || self.bytes != 0) {
            return Err(RuntimeError::internal(
                "closed push buffer retained publication ownership",
            ));
        }
        Ok(())
    }

    fn raise_fatal_or_gap(&mut self) -> Result<(), RuntimeError> {
        if let Some(error) = &self.fatal {
            return Err(error.clone());
        }
        if self.dropped_total > self.reported_dropped_total {
            self.reported_dropped_total = self.dropped_total;
            return Err(RuntimeError::PushOverflow {
                dropped_total: self.dropped_total,
                context: Vec::new(),
            });
        }
        Ok(())
    }

    fn record_drop_count(&mut self, count: u64) {
        self.dropped_total = self.dropped_total.saturating_add(count);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytes::Bytes;
    use eltdx_protocol::frame::{ResponseFrame, ResponseHeader, RESPONSE_PREFIX};

    use super::{PushBuffer, PushFrame};
    use crate::error::RuntimeError;
    use crate::slot::{EngineEpoch, GenerationId, SlotId};

    fn frame(
        epoch: EngineEpoch,
        message: u32,
        payload_size: usize,
    ) -> Result<PushFrame, RuntimeError> {
        let length = u16::try_from(payload_size)
            .map_err(|_| RuntimeError::internal("test payload is too large"))?;
        let header = ResponseHeader {
            control: 0,
            msg_id: message,
            reserved: 0,
            msg_type: 0x0547,
            zip_length: length,
            length,
        };
        let mut raw = vec![0_u8; 16 + payload_size];
        raw[..4].copy_from_slice(&RESPONSE_PREFIX);
        raw[5..9].copy_from_slice(&message.to_le_bytes());
        raw[10..12].copy_from_slice(&0x0547_u16.to_le_bytes());
        raw[12..14].copy_from_slice(&length.to_le_bytes());
        raw[14..16].copy_from_slice(&length.to_le_bytes());
        Ok(PushFrame {
            engine_epoch: epoch,
            slot_id: SlotId::new(0),
            generation: GenerationId::new(1)?,
            connected_host: Arc::from("127.0.0.1:7709"),
            response: ResponseFrame::from_decoded(
                header,
                Bytes::from(vec![1_u8; payload_size]),
                Bytes::from(raw),
            )?,
        })
    }

    #[test]
    fn frame_and_byte_limits_drop_oldest_and_report_one_gap() -> Result<(), RuntimeError> {
        let epoch = EngineEpoch::new(1)?;
        let mut buffer = PushBuffer::new(epoch, 2, 36)?;
        assert!(buffer.offer(frame(epoch, 1, 1)?));
        assert!(buffer.offer(frame(epoch, 2, 1)?));
        assert!(buffer.offer(frame(epoch, 3, 1)?));

        assert!(matches!(
            buffer.poll(),
            Err(RuntimeError::PushOverflow {
                dropped_total: 1,
                ..
            })
        ));
        assert_eq!(buffer.poll()?.map(|item| item.response.msg_id), Some(2));
        assert_eq!(buffer.poll()?.map(|item| item.response.msg_id), Some(3));
        assert_eq!(buffer.poll()?, None);
        buffer.check_invariants()?;
        Ok(())
    }

    #[test]
    fn wrong_epoch_and_close_never_republish_frames() -> Result<(), RuntimeError> {
        let epoch = EngineEpoch::new(2)?;
        let stale = EngineEpoch::new(1)?;
        let mut buffer = PushBuffer::new(epoch, 2, 20)?;
        assert!(!buffer.offer(frame(stale, 1, 1)?));
        assert!(!buffer.offer(frame(epoch, 2, 5)?));
        assert!(matches!(
            buffer.drain(),
            Err(RuntimeError::PushOverflow { .. })
        ));
        assert!(buffer.close(epoch, None));
        assert!(!buffer.offer(frame(epoch, 3, 1)?));
        assert_eq!(buffer.poll()?, None);
        assert_eq!(buffer.snapshot().frame_count, 0);
        buffer.check_invariants()?;
        Ok(())
    }

    #[test]
    fn fatal_has_priority_over_gap_and_frame() -> Result<(), RuntimeError> {
        let epoch = EngineEpoch::new(3)?;
        let mut buffer = PushBuffer::new(epoch, 1, 64)?;
        assert!(buffer.offer(frame(epoch, 1, 1)?));
        assert!(buffer.offer(frame(epoch, 2, 1)?));
        let fatal = RuntimeError::connection_closed("runtime fatal");
        assert!(buffer.close(epoch, Some(fatal.clone())));

        assert_eq!(buffer.poll(), Err(fatal.clone()));
        assert_eq!(buffer.drain(), Err(fatal));
        assert_eq!(buffer.snapshot().frame_count, 0);
        buffer.check_invariants()?;
        Ok(())
    }

    #[test]
    fn later_external_drop_cannot_be_cleared_by_an_older_gap_report() -> Result<(), RuntimeError> {
        let epoch = EngineEpoch::new(4)?;
        let mut buffer = PushBuffer::new(epoch, 2, 64)?;
        assert!(buffer.record_external_drop(epoch, 1));
        assert!(matches!(
            buffer.poll(),
            Err(RuntimeError::PushOverflow {
                dropped_total: 1,
                ..
            })
        ));
        assert!(buffer.record_external_drop(epoch, 1));
        assert!(matches!(
            buffer.poll(),
            Err(RuntimeError::PushOverflow {
                dropped_total: 2,
                ..
            })
        ));
        buffer.check_invariants()?;
        Ok(())
    }
}
