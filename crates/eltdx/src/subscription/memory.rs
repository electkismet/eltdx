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
