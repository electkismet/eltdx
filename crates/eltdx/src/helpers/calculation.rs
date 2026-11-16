//! Scalar operations shared by strict values and the Python legacy projection.
//! Validation, missing-value policy and rounding belong to the calling profile.

pub fn turnover_with<T, E>(volume: T, shares: &T,
    mut multiply_hundred: impl FnMut(T) -> Result<T, E>,
    divide: impl FnOnce(T, &T) -> Result<T, E>) -> Result<T, E> {
    let volume = multiply_hundred(volume)?;
    multiply_hundred(divide(volume, shares)?)
}

pub fn product_with<T, E>(left: T, right: T,
    multiply: impl FnOnce(T, T) -> Result<T, E>) -> Result<T, E> {
    multiply(left, right)
}

pub fn share_source_names(total: bool, circulating: bool, free_float: bool) -> Vec<&'static str> {
    let mut sources = Vec::new();
    if total || circulating { sources.push("finance_snapshot"); }
    if free_float { sources.push("tdx_stats"); }
    sources
}

#[cfg(test)]
mod source_cases {
    use super::*;

    #[test]
    fn profile_math_preserves_operation_order_and_source_labels() {
        let mut stages = Vec::new();
        let value = turnover_with::<_, ()>(200.0, &100000.0,
            |value| { stages.push(value); Ok(value * 100.0) },
            |value, shares| Ok(value / shares)).unwrap();
        assert_eq!(value, 20.0);
        assert_eq!(stages, [200.0, 0.2]);
        assert_eq!(share_source_names(false, true, true), ["finance_snapshot", "tdx_stats"]);
        assert!(share_source_names(false, false, false).is_empty());
        assert_eq!(share_source_names(false, false, true), ["tdx_stats"]);
    }
}
