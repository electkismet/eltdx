//! A structural upper bound for the three supported event shapes. Runtime
//! publication still charges actual canonical allocations, not this bound.
use std::mem::size_of;
use crate::error::{Error, ErrorKind, Result};
use crate::types::{BatchItem, Diagnostic, PushEvent, QuoteLevel, RawPolicy, Snapshot, SourceStamp};
use crate::types::memory::{MemoryCharge, BUFFER_CONTROL_BYTES};

pub(super) fn maximum_event_bytes(rows: usize, host_bytes: usize, owner_bytes: usize, raw: RawPolicy) -> Result<usize> {
    // Each initial/resync group requests <=100 snapshots. Recovery has the
    // existing operation-wide SNAPSHOT_RECOVERY limit, exported by the service.
    let pages = rows.div_ceil(100).checked_add(crate::services::SNAPSHOT_RECOVERY_REQUESTS).ok_or_else(overflow)?;
    let mut row = MemoryCharge::default();
    row.add(6)?; // Snapshot identity; SecurityId is always six ASCII digits.
    row.array::<QuoteLevel>(10)?; // At most five bids and five asks.
    metadata(&mut row, 1, 1, host_bytes, owner_bytes, raw)?;
    let mut batch = MemoryCharge::default();
    batch.add(size_of::<PushEvent>())?;
    batch.array::<BatchItem<Snapshot>>(rows)?;
    batch.add(rows.checked_mul(row.total().checked_add(6).ok_or_else(overflow)?).ok_or_else(overflow)?)?;
    metadata(&mut batch, pages, rows, host_bytes, owner_bytes, raw)?;
    buffers(&mut batch, pages, raw)?;
    let mut update = MemoryCharge::default();
    update.add(size_of::<PushEvent>())?;
    update.add(6)?; // QuoteUpdate identity, separate from Snapshot identity.
    update.add(row.total())?;
    buffers(&mut update, 1, raw)?;
    Ok(batch.total().max(update.total()))
}

fn metadata(charge: &mut MemoryCharge, pages: usize, rows: usize, host: usize, owner: usize, raw: RawPolicy) -> Result<()> {
    charge.add(owner)?; // primary SourceStamp session string
    charge.array::<SourceStamp>(pages)?;
    charge.add(pages.checked_mul(owner).ok_or_else(overflow)?)?;
    charge.array::<Diagnostic>(pages)?;
    // These are the longest literal source/stage labels in the shared quote
    // projections. Endpoint text comes from this Engine's configured host list.
    let diagnostic = "quotes.subscription".len().checked_add("response".len())
        .and_then(|n| n.checked_add("endpoint=".len())).and_then(|n| n.checked_add(host)).ok_or_else(overflow)?;
    charge.add(pages.checked_mul(diagnostic).ok_or_else(overflow)?)?;
    if raw != RawPolicy::Omit {
        charge.array::<bytes::Bytes>(pages)?;
        if raw == RawPolicy::RetainWire { charge.array::<bytes::Bytes>(pages)?; }
        charge.array::<(usize, usize, usize)>(rows)?;
        charge.array::<bool>(rows)?;
    }
    Ok(())
}

fn buffers(charge: &mut MemoryCharge, pages: usize, raw: RawPolicy) -> Result<()> {
    if raw == RawPolicy::Omit { return Ok(()); }
    let payload = eltdx_protocol::limits::MAX_RESPONSE_PAYLOAD_SIZE.checked_add(BUFFER_CONTROL_BYTES).ok_or_else(overflow)?;
    let one = if raw == RawPolicy::RetainWire {
        payload.checked_add(eltdx_protocol::limits::MAX_RESPONSE_BUFFER_SIZE)
            .and_then(|n| n.checked_add(BUFFER_CONTROL_BYTES)).ok_or_else(overflow)?
    } else { payload };
    charge.add(pages.checked_mul(one).ok_or_else(overflow)?)
}

fn overflow() -> Error { Error::new(ErrorKind::InvalidArgument, "supported subscription event size overflows") }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BatchItemState, EnrichmentStatus, RawData, ResultMeta, SnapshotBatch};

    #[test]
    fn maximum_shape_covers_canonical_snapshots_updates_and_shared_raw_storage() {
        let host = "127.0.0.1:7709"; let owner = "owner-1234567890abcdef";
        for raw in [RawPolicy::Omit, RawPolicy::RetainPayload, RawPolicy::RetainWire] {
            let payload = bytes::Bytes::from(vec![1; eltdx_protocol::limits::MAX_RESPONSE_PAYLOAD_SIZE]);
            let wire = bytes::Bytes::from(vec![2; eltdx_protocol::limits::MAX_RESPONSE_BUFFER_SIZE]);
            let source = SourceStamp::new(Some(0x054c), Some(host.parse().unwrap()), owner, Some(1), None);
            let diagnostic = Diagnostic::new("quotes.subscription", "response", EnrichmentStatus::Succeeded,
                None, Some(format!("endpoint={host}")));
            let retained = || {
                if raw == RawPolicy::Omit { return None; }
                let mut payloads = Vec::with_capacity(1024); payloads.push(payload.clone());
                let frames = (raw == RawPolicy::RetainWire).then(|| {
                    let mut frames = Vec::with_capacity(1024); frames.push(wire.clone()); frames
                });
                Some(RawData::new(payloads, frames, vec![(0, 0, 10)]))
            };
            let base = crate::subscription::source_cases::update("sh600000", 93000);
            let row = base.snapshot();
            let level = QuoteLevel::new(row.last(), row.total());
            let mut bids = Vec::with_capacity(256); bids.resize(5, level.clone());
            let mut asks = Vec::with_capacity(256); asks.resize(5, level);
            let snapshot = Snapshot::new(row.id().clone(), row.active(), row.last(), row.pre_close(), row.open(),
                row.high(), row.low(), row.event_time(), row.server_time_raw(), row.total(), row.current(),
                row.inside(), row.outside(), row.amount(), row.open_amount(), bids, asks,
                ResultMeta::new(source.clone(), vec![diagnostic.clone()], retained()));
            let update_snapshot = snapshot.clone();
            let batch = SnapshotBatch::with_meta(vec![BatchItem::new(0, row.id().clone(),
                BatchItemState::Present(snapshot))], ResultMeta::new(source, vec![diagnostic], retained()));
            let mut initial = PushEvent::Initial(batch);
            let before = initial.retained_bytes().unwrap();
            initial.canonical_heap();
            let after = initial.retained_bytes().unwrap();
            assert!(after < before, "canonical publication discards spare collection capacity");
            let maximum = maximum_event_bytes(1, host.len(), owner.len(), raw).unwrap();
            assert!(after <= maximum);
            let PushEvent::Initial(batch) = &initial else { unreachable!() };
            let mut restored = PushEvent::Resynchronized(batch.clone()); restored.canonical_heap();
            assert!(restored.retained_bytes().unwrap() <= maximum);
            let mut update = PushEvent::Update(crate::types::QuoteUpdate::new(row.id().clone(), 93000, 1, update_snapshot, 0));
            update.canonical_heap();
            assert!(update.retained_bytes().unwrap() <= maximum);
        }
    }
}
