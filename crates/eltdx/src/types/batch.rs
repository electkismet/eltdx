use crate::types::SecurityId;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum MissingPolicy {
    #[default]
    Error,
    AllowPartial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum MissingReason {
    NotFound,
    Unavailable,
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum BatchItemState<T> {
    Present(T),
    ConfirmedMissing(MissingReason),
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BatchItem<T> {
    input_index: usize,
    security: SecurityId,
    state: BatchItemState<T>,
}

impl<T> BatchItem<T> {
    pub fn new(input_index: usize, security: SecurityId, state: BatchItemState<T>) -> Self {
        Self { input_index, security, state }
    }
    pub const fn input_index(&self) -> usize { self.input_index }
    pub fn security(&self) -> &SecurityId { &self.security }
    pub fn state(&self) -> &BatchItemState<T> { &self.state }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BatchOutcome<T> {
    items: Vec<BatchItem<T>>,
}

impl<T> BatchOutcome<T> {
    pub fn new(items: Vec<BatchItem<T>>) -> Self { Self { items } }
    pub fn items(&self) -> &[BatchItem<T>] { &self.items }
    pub fn into_items(self) -> Vec<BatchItem<T>> { self.items }
    pub fn is_empty(&self) -> bool { self.items.is_empty() }
    pub fn len(&self) -> usize { self.items.len() }
}

pub type SnapshotBatch<T> = BatchOutcome<T>;
