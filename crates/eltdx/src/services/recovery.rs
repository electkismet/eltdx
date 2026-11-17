//! One ordered binary isolation algorithm for snapshot and finance batches.
//! Domain adapters decide what constitutes a recoverable structure or absence;
//! authorization/budget accounting remains in the existing operation executor.
use crate::operation::OperationContext;
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use eltdx_protocol::unit::NormalizedCode;

pub(crate) enum RecoveryStep<T> {
    Complete(T),
    Split,
}

pub(crate) fn ordered_tree<T>(
    context: &OperationContext,
    codes: &[NormalizedCode],
    recovery: bool,
    query: &mut impl FnMut(&[NormalizedCode], bool) -> Result<RecoveryStep<T>>,
    merge: &impl Fn(T, T) -> Result<T>,
) -> Result<T> {
    ordered_tree_with(
        codes,
        recovery,
        &mut |part, recovery| {
            context.check_boundary(TimeoutPhase::Admission)?;
            query(part, recovery)
        },
        merge,
        &|| {
            Error::new(
                ErrorKind::Protocol,
                "cannot isolate a malformed batch singleton",
            )
        },
    )
}

/// The traversal is independent of the execution profile. Python adapters keep
/// their original exceptions and callback objects; SDK queries keep operation
/// admission and authorization in the callback above.
pub(crate) fn ordered_tree_with<C, T, E>(
    codes: &[C],
    recovery: bool,
    query: &mut impl FnMut(&[C], bool) -> std::result::Result<RecoveryStep<T>, E>,
    merge: &impl Fn(T, T) -> std::result::Result<T, E>,
    invalid_split: &impl Fn() -> E,
) -> std::result::Result<T, E> {
    match query(codes, recovery)? {
        RecoveryStep::Complete(value) => Ok(value),
        RecoveryStep::Split => {
            if codes.len() <= 1 {
                return Err(invalid_split());
            }
            let middle = codes.len() / 2;
            let first = ordered_tree_with(&codes[..middle], true, query, merge, invalid_split)?;
            let second = ordered_tree_with(&codes[middle..], true, query, merge, invalid_split)?;
            merge(first, second)
        }
    }
}
