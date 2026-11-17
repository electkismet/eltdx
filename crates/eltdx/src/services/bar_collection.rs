//! Shared bar collection mechanics. Compatibility changes admission and error
//! projection; it does not introduce a second pagination or ordering loop.

use std::collections::HashSet;

use bytes::Bytes;

use crate::error::{Error, ErrorKind, Result, TimeoutPhase};
use crate::operation::{ChildPurpose, OperationContext};

use super::allocation_error;
use super::pagination::{advance_offset, CollectionLimits, PageProgress};

#[derive(Clone, Copy, Debug)]
pub(crate) enum BarCollectionPolicy {
    Strict {
        limits: CollectionLimits,
        repeated_page: &'static str,
        offset_error: &'static str,
    },
    Legacy {
        maximum_pages: Option<usize>,
        api: &'static str,
    },
}

/// Both protocols select the newest page first. Only the final assembly differs:
/// ordinary bars reverse pages; delisted bars use a stable chronological sort.
#[derive(Clone, Copy, Debug)]
pub(crate) enum BarOrder {
    ReversePages,
    Chronological,
}

pub(crate) fn walk_bar_pages<T>(
    context: &OperationContext,
    start: u32,
    page_size: usize,
    policy: BarCollectionPolicy,
    mut fetch: impl FnMut(u32, usize, ChildPurpose) -> Result<(T, usize, Bytes)>,
    mut consume: impl FnMut(T) -> Result<()>,
) -> Result<usize> {
    let mut progress = PageProgress::default();
    let mut offset = start;
    let mut seen = HashSet::new();
    loop {
        context.check_boundary(TimeoutPhase::Admission)?;
        let requested = match policy {
            BarCollectionPolicy::Strict { limits, .. } => {
                progress.before_request(limits)?;
                let remaining = limits.records.map_or(page_size, |maximum| {
                    maximum.saturating_sub(progress.records()).max(1)
                });
                page_size.min(remaining)
            }
            BarCollectionPolicy::Legacy { .. } => page_size,
        };
        if offset > u32::from(u16::MAX) {
            return Err(match policy {
                BarCollectionPolicy::Strict { offset_error, .. } => {
                    Error::new(ErrorKind::LimitExceeded, offset_error)
                }
                // The former facade constructed the next request and failed its
                // u16 extraction. Preserve that public exception and message.
                BarCollectionPolicy::Legacy { .. } => Error::new(
                    ErrorKind::InvalidArgument,
                    "start must be between 0 and 65535",
                )
                .with_context("name", "start"),
            });
        }
        let purpose = if progress.pages() == 0 {
            ChildPurpose::Required
        } else {
            ChildPurpose::Page
        };
        let (page, count, evidence) = fetch(offset, requested, purpose)?;
        match policy {
            BarCollectionPolicy::Strict {
                limits,
                repeated_page,
                ..
            } => {
                progress.accept(count, requested, limits)?;
                if count != 0 {
                    seen.try_reserve(1).map_err(allocation_error)?;
                    if !seen.insert(evidence) {
                        return Err(Error::new(ErrorKind::Protocol, repeated_page));
                    }
                }
            }
            BarCollectionPolicy::Legacy { .. } => progress.accept_legacy(count)?,
        }
        context.record_work(count, 0)?;
        consume(page)?;
        context.check_boundary(TimeoutPhase::Aggregate)?;
        // A short page is not EOF. The empty terminal page is consumed too, so
        // first-page metadata and raw survive even an entirely empty history.
        if count == 0 {
            return Ok(progress.pages());
        }
        if let BarCollectionPolicy::Legacy {
            maximum_pages: Some(maximum),
            api,
        } = policy
        {
            if progress.pages() >= maximum {
                return Err(Error::new(
                    ErrorKind::LimitExceeded,
                    format!("{api}.get reached max_pages before the server returned an empty page"),
                )
                .with_context("python_kind", "RuntimeError"));
            }
        }
        offset = advance_offset(offset, count)?;
        if let BarCollectionPolicy::Strict { offset_error, .. } = policy {
            if offset > u32::from(u16::MAX) {
                return Err(Error::new(ErrorKind::LimitExceeded, offset_error));
            }
        }
    }
}

/// Returns both the public row order and the permutation of received row
/// positions. Rust raw provenance and the Python tuple ABI use the same order.
pub(crate) fn order_bar_pages<T, K: Ord>(
    pages: Vec<Vec<T>>,
    order: BarOrder,
    mut time: impl FnMut(&T) -> K,
) -> Result<(Vec<T>, Vec<usize>)> {
    let total = pages
        .iter()
        .try_fold(0usize, |sum, page| sum.checked_add(page.len()))
        .ok_or_else(|| Error::new(ErrorKind::LimitExceeded, "collected K-line count overflows"))?;
    let mut indexed = Vec::new();
    indexed.try_reserve(total).map_err(allocation_error)?;
    match order {
        BarOrder::ReversePages => {
            let mut base = total;
            for page in pages.into_iter().rev() {
                base = base.checked_sub(page.len()).ok_or_else(|| {
                    Error::new(ErrorKind::Internal, "K-line page row mapping underflows")
                })?;
                indexed.extend(
                    page.into_iter()
                        .enumerate()
                        .map(|(index, row)| (base + index, row)),
                );
            }
        }
        BarOrder::Chronological => {
            for page in pages {
                for row in page {
                    indexed.push((indexed.len(), row));
                }
            }
            indexed.sort_by_key(|(_, row)| time(row));
        }
    }
    let mut rows = Vec::new();
    let mut permutation = Vec::new();
    rows.try_reserve(total).map_err(allocation_error)?;
    permutation.try_reserve(total).map_err(allocation_error)?;
    for (index, row) in indexed {
        rows.push(row);
        permutation.push(index);
    }
    Ok((rows, permutation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_pages_preserve_rows_and_reverse_only_pages() -> Result<()> {
        let (rows, permutation) = order_bar_pages(
            vec![vec![3, 4], vec![1, 2], vec![]],
            BarOrder::ReversePages,
            |row| *row,
        )?;
        assert_eq!(rows, [1, 2, 3, 4]);
        assert_eq!(permutation, [2, 3, 0, 1]);
        Ok(())
    }

    #[test]
    fn delisted_equal_times_keep_their_received_order() -> Result<()> {
        let (rows, permutation) = order_bar_pages(
            vec![vec![(2, 'a'), (1, 'b')], vec![(1, 'c')], vec![]],
            BarOrder::Chronological,
            |row| row.0,
        )?;
        assert_eq!(rows, [(1, 'b'), (1, 'c'), (2, 'a')]);
        assert_eq!(permutation, [1, 2, 0]);
        Ok(())
    }
}
