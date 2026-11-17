use super::{invalid, RecordKey};
use crate::error::Result;
use crate::types::Diagnostic;
use crate::{CancellationToken, RawPolicy};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TailPolicy {
    #[default]
    IgnoreWithDiagnostic,
    Reject,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Durability {
    /// Daily performs flush + sync_all; minute and EX use ordinary close.
    #[default]
    FormatDefault,
    Flush,
    SyncAll,
}

#[derive(Clone, Debug)]
pub struct ReadOptions {
    tail: TailPolicy,
    raw: RawPolicy,
    max_bytes: u64,
    cancel: Option<CancellationToken>,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            tail: TailPolicy::IgnoreWithDiagnostic,
            raw: RawPolicy::Omit,
            max_bytes: 256 * 1024 * 1024,
            cancel: None,
        }
    }
}

impl ReadOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn tail(&self) -> TailPolicy {
        self.tail
    }
    pub const fn raw(&self) -> RawPolicy {
        self.raw
    }
    pub const fn max_bytes(&self) -> u64 {
        self.max_bytes
    }
    pub fn cancel(&self) -> Option<&CancellationToken> {
        self.cancel.as_ref()
    }
    pub fn with_tail(mut self, value: TailPolicy) -> Self {
        self.tail = value;
        self
    }
    pub fn with_raw(mut self, value: RawPolicy) -> Self {
        self.raw = value;
        self
    }
    pub fn with_max_bytes(mut self, value: u64) -> Result<Self> {
        // The cap must leave room for the single-byte overflow probe and a Vec length.
        if value == 0 || value > (isize::MAX as u64).saturating_sub(1) {
            return Err(invalid(
                "max_bytes must be positive and fit a bounded allocation",
            ));
        }
        self.max_bytes = value;
        Ok(self)
    }
    pub fn with_cancel(mut self, value: Option<CancellationToken>) -> Self {
        self.cancel = value;
        self
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppendOptions {
    durability: Durability,
    create_parents: bool,
    cancel: Option<CancellationToken>,
}

impl AppendOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub const fn durability(&self) -> Durability {
        self.durability
    }
    pub const fn create_parents(&self) -> bool {
        self.create_parents
    }
    pub fn cancel(&self) -> Option<&CancellationToken> {
        self.cancel.as_ref()
    }
    pub fn with_durability(mut self, value: Durability) -> Self {
        self.durability = value;
        self
    }
    pub fn with_create_parents(mut self, value: bool) -> Self {
        self.create_parents = value;
        self
    }
    pub fn with_cancel(mut self, value: Option<CancellationToken>) -> Self {
        self.cancel = value;
        self
    }
}

#[derive(Clone, Debug)]
pub struct ReadResult<T> {
    rows: Vec<T>,
    ignored_tail_bytes: usize,
    diagnostics: Vec<Diagnostic>,
}

impl<T> ReadResult<T> {
    pub(crate) fn new(
        rows: Vec<T>,
        ignored_tail_bytes: usize,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            rows,
            ignored_tail_bytes,
            diagnostics,
        }
    }
    pub fn rows(&self) -> &[T] {
        &self.rows
    }
    pub const fn ignored_tail_bytes(&self) -> usize {
        self.ignored_tail_bytes
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn into_rows(self) -> Vec<T> {
        self.rows
    }
}

#[derive(Clone, Debug)]
pub struct WriteReport {
    accepted: usize,
    skipped: usize,
    bytes_written: u64,
    final_key: Option<RecordKey>,
    durability: Durability,
}

impl WriteReport {
    pub(crate) fn new(
        accepted: usize,
        skipped: usize,
        bytes_written: u64,
        final_key: Option<RecordKey>,
        durability: Durability,
    ) -> Self {
        Self {
            accepted,
            skipped,
            bytes_written,
            final_key,
            durability,
        }
    }
    pub const fn accepted(&self) -> usize {
        self.accepted
    }
    pub const fn skipped(&self) -> usize {
        self.skipped
    }
    pub const fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
    pub const fn final_key(&self) -> Option<RecordKey> {
        self.final_key
    }
    /// Actual completed policy. An empty append returns FormatDefault (no sync occurred).
    pub const fn durability(&self) -> Durability {
        self.durability
    }
}
