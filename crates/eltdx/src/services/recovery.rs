//! One ordered binary isolation algorithm for snapshot and finance batches.
//! Domain adapters decide what constitutes a recoverable structure or absence;
//! authorization/budget accounting remains in the existing operation executor.
use crate::{Error, ErrorKind, Result, TimeoutPhase};
use crate::operation::OperationContext;
use eltdx_protocol::unit::NormalizedCode;

pub(crate) enum RecoveryStep<T> { Complete(T), Split }

pub(crate) fn ordered_tree<T>(context: &OperationContext, codes: &[NormalizedCode], recovery: bool,
    query: &mut impl FnMut(&[NormalizedCode], bool) -> Result<RecoveryStep<T>>,
    merge: &impl Fn(T, T) -> Result<T>) -> Result<T> {
    context.check_boundary(TimeoutPhase::Admission)?;
    match query(codes, recovery)? {
        RecoveryStep::Complete(value) => Ok(value),
        RecoveryStep::Split => {
            if codes.len() <= 1 { return Err(Error::new(ErrorKind::Protocol, "cannot isolate a malformed batch singleton")); }
            let middle = codes.len() / 2;
            let first = ordered_tree(context, &codes[..middle], true, query, merge)?;
            let second = ordered_tree(context, &codes[middle..], true, query, merge)?;
            merge(first, second)
        }
    }
}
