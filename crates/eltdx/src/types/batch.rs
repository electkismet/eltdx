use crate::types::provenance::ResultMeta;
use crate::types::quotes::Snapshot;
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
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BatchItem<T> {
    input_index: usize,
    security: SecurityId,
    state: BatchItemState<T>,
}

impl<T> BatchItem<T> {
    pub fn new(input_index: usize, security: SecurityId, state: BatchItemState<T>) -> Self {
        Self {
            input_index,
            security,
            state,
        }
    }
    pub const fn input_index(&self) -> usize {
        self.input_index
    }
    pub fn security(&self) -> &SecurityId {
        &self.security
    }
    pub fn state(&self) -> &BatchItemState<T> {
        &self.state
    }
    pub(crate) fn into_parts(self) -> (usize, SecurityId, BatchItemState<T>) {
        (self.input_index, self.security, self.state)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BatchOutcome<T> {
    items: Vec<BatchItem<T>>,
    meta: ResultMeta,
}

impl<T> BatchOutcome<T> {
    pub fn new(items: Vec<BatchItem<T>>) -> Self {
        Self {
            items,
            meta: ResultMeta::default(),
        }
    }
    pub fn with_meta(items: Vec<BatchItem<T>>, meta: ResultMeta) -> Self {
        Self { items, meta }
    }
    pub fn items(&self) -> &[BatchItem<T>] {
        &self.items
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
    pub fn into_items(self) -> Vec<BatchItem<T>> {
        self.items
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

pub type SnapshotBatch = BatchOutcome<Snapshot>;

impl BatchOutcome<Snapshot> {
    pub(crate) fn canonical_heap(&mut self) {
        super::memory::canonical_vec(&mut self.items);
        for item in &mut self.items {
            item.security.canonical_heap();
            match &mut item.state {
                BatchItemState::Present(snapshot) => snapshot.canonical_heap(),
                BatchItemState::ConfirmedMissing(MissingReason::Other(reason)) => {
                    super::memory::canonical_string(reason)
                }
                BatchItemState::ConfirmedMissing(_) => {}
            }
        }
        self.meta.canonical_heap();
    }
    pub(crate) fn into_parts(self) -> (Vec<BatchItem<Snapshot>>, ResultMeta) {
        (self.items, self.meta)
    }
    pub(crate) fn charge_heap(
        &self,
        charge: &mut super::memory::MemoryCharge,
    ) -> crate::Result<()> {
        charge.array::<BatchItem<Snapshot>>(self.items.capacity())?;
        for item in &self.items {
            item.security.charge_heap(charge)?;
            match &item.state {
                BatchItemState::Present(snapshot) => snapshot.charge_heap(charge)?,
                BatchItemState::ConfirmedMissing(MissingReason::Other(reason)) => {
                    charge.string(reason)?
                }
                BatchItemState::ConfirmedMissing(_) => {}
            }
        }
        self.meta.charge_heap(charge)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EndEvidence {
    Unknown,
    EmptyPage,
    VerifiedShortPage,
    ExplicitTotal(u64),
    RequestedWindow,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageCursor {
    version: u16,
    next_offset: u32,
    page_size: usize,
    query_fingerprint: u64,
    owner_id: usize,
    endpoint: Option<String>,
    metadata_version: Option<u64>,
}

impl PageCursor {
    pub(crate) const fn new(
        next_offset: u32,
        page_size: usize,
        query_fingerprint: u64,
        owner_id: usize,
        endpoint: Option<String>,
        metadata_version: Option<u64>,
    ) -> Self {
        Self {
            version: 1,
            next_offset,
            page_size,
            query_fingerprint,
            owner_id,
            endpoint,
            metadata_version,
        }
    }

    pub(crate) const fn next_offset(&self) -> u32 {
        self.next_offset
    }
    pub(crate) const fn page_size(&self) -> usize {
        self.page_size
    }
    pub(crate) const fn query_fingerprint(&self) -> u64 {
        self.query_fingerprint
    }
    pub(crate) const fn owner_id(&self) -> usize {
        self.owner_id
    }
    pub(crate) fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }
    pub(crate) const fn metadata_version(&self) -> Option<u64> {
        self.metadata_version
    }
    pub(crate) const fn version(&self) -> u16 {
        self.version
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DataPage<T> {
    items: Vec<T>,
    start: u64,
    requested: usize,
    next_cursor: Option<PageCursor>,
    end: EndEvidence,
    meta: ResultMeta,
}

impl<T> DataPage<T> {
    pub fn new(
        items: Vec<T>,
        start: u64,
        requested: usize,
        next_cursor: Option<PageCursor>,
        end: EndEvidence,
        meta: ResultMeta,
    ) -> Self {
        Self {
            items,
            start,
            requested,
            next_cursor,
            end,
            meta,
        }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }
    pub fn into_items(self) -> Vec<T> {
        self.items
    }
    pub const fn start(&self) -> u64 {
        self.start
    }
    pub const fn requested(&self) -> usize {
        self.requested
    }
    pub fn next_cursor(&self) -> Option<&PageCursor> {
        self.next_cursor.as_ref()
    }
    pub const fn end(&self) -> EndEvidence {
        self.end
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Collected<T> {
    items: Vec<T>,
    end: EndEvidence,
    pages: usize,
    meta: ResultMeta,
}

impl<T> Collected<T> {
    pub fn new(items: Vec<T>, end: EndEvidence, pages: usize, meta: ResultMeta) -> Self {
        Self {
            items,
            end,
            pages,
            meta,
        }
    }

    pub fn items(&self) -> &[T] {
        &self.items
    }
    pub fn into_items(self) -> Vec<T> {
        self.items
    }
    pub const fn end(&self) -> EndEvidence {
        self.end
    }
    pub const fn pages(&self) -> usize {
        self.pages
    }
    pub fn meta(&self) -> &ResultMeta {
        &self.meta
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageOptions {
    page_size: usize,
    max_pages: usize,
    max_records: usize,
}

impl PageOptions {
    pub fn new(
        page_size: usize,
        max_pages: usize,
        max_records: usize,
    ) -> crate::error::Result<Self> {
        if page_size == 0 || max_pages == 0 || max_records == 0 {
            return Err(crate::error::Error::new(
                crate::error::ErrorKind::InvalidArgument,
                "page size and collection limits must be positive",
            ));
        }
        Ok(Self {
            page_size,
            max_pages,
            max_records,
        })
    }

    pub fn with_page_size(mut self, value: usize) -> crate::error::Result<Self> {
        if value == 0 {
            return Err(page_options_error("page_size"));
        }
        self.page_size = value;
        Ok(self)
    }

    pub fn with_max_pages(mut self, value: usize) -> crate::error::Result<Self> {
        if value == 0 {
            return Err(page_options_error("max_pages"));
        }
        self.max_pages = value;
        Ok(self)
    }

    pub fn with_max_records(mut self, value: usize) -> crate::error::Result<Self> {
        if value == 0 {
            return Err(page_options_error("max_records"));
        }
        self.max_records = value;
        Ok(self)
    }

    pub const fn page_size(&self) -> usize {
        self.page_size
    }
    pub const fn max_pages(&self) -> usize {
        self.max_pages
    }
    pub const fn max_records(&self) -> usize {
        self.max_records
    }
}

fn page_options_error(field: &'static str) -> crate::error::Error {
    crate::error::Error::new(
        crate::error::ErrorKind::InvalidArgument,
        "page options must be positive",
    )
    .with_context("field", field)
}
