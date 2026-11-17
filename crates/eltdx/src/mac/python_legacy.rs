//! Shared paging arithmetic with caller-defined numeric/limit policy.
//! This module contains no Python types or runtime dependency.

/// Input bit, legacy quote field, and aggregate name. Both public surfaces
/// use this registry; their missing-value and iteration policies differ.
#[doc(hidden)]
pub const BOARD_MONEY_AGGREGATES: [(u8,&str,&str);4] = [
    (7,"amount","amount"),(56,"main_net_amount","main_net_amount"),
    (111,"main_net_3d_amount","main_net_3d"),(112,"main_net_5d_amount","main_net_5d"),
];

#[doc(hidden)]
pub fn remaining_page_with<T,E>(target: &T, consumed: &T,
    subtract: impl FnOnce(&T,&T) -> std::result::Result<T,E>,
    maximum: impl FnOnce() -> std::result::Result<T,E>,
    minimum: impl FnOnce(T,T) -> std::result::Result<T,E>) -> std::result::Result<T,E>
{
    let remaining = subtract(target,consumed)?;
    minimum(remaining,maximum()?)
}

#[doc(hidden)]
pub fn repair_needed_with<E>(requested: impl FnOnce() -> std::result::Result<bool,E>,
    nonempty: impl FnOnce() -> std::result::Result<bool,E>,
    invalid: impl FnOnce() -> std::result::Result<bool,E>) -> std::result::Result<bool,E>
{
    Ok(requested()? && nonempty()? && invalid()?)
}

#[cfg(test)]
mod source_cases {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn page_policy_keeps_subtraction_then_live_maximum_then_minimum() {
        let order = RefCell::new(Vec::new());
        let result = remaining_page_with::<_,()>(&100_i64,&80,
            |target,used| { order.borrow_mut().push("subtract"); Ok(target-used) },
            || { order.borrow_mut().push("maximum"); Ok(80) },
            |remaining,maximum| { order.borrow_mut().push("minimum"); Ok(remaining.min(maximum)) }).unwrap();
        assert_eq!(result,20);
        assert_eq!(*order.borrow(),["subtract","maximum","minimum"]);
        // Numeric validation is the strict caller's job. Legacy custom
        // initial requests keep signed values and impose no extra page cap.
        assert_eq!(remaining_page_with::<_,()>(&-2_i64,&0,|a,b| Ok(a-b),|| Ok(80),|a,b| Ok(a.min(b))).unwrap(),-2);
    }

    #[test]
    fn repair_keeps_lazy_predicates_and_original_errors() {
        assert!(!repair_needed_with::<()>(|| Ok(false),|| panic!("ordinary mode"),|| panic!("ordinary mode")).unwrap());
        assert!(!repair_needed_with::<()>(|| Ok(true),|| Ok(false),|| panic!("empty series")).unwrap());
        assert!(repair_needed_with::<()>(|| Ok(true),|| Ok(true),|| Ok(true)).unwrap());
        assert_eq!(repair_needed_with::<&str>(|| Ok(true),|| Ok(true),|| Err("price hook failure")),Err("price hook failure"));
    }
}
