//! Scalar operations shared by strict values and the Python legacy projection.
//! Validation, missing-value policy and rounding belong to the calling profile.

/// Split a YYYYMMDD scalar in source evaluation order. Integer semantics,
/// arbitrary precision and Gregorian validation belong to the caller.
pub fn ymd_parts_with<T,E>(value: &T,
    year: impl FnOnce(&T) -> Result<T,E>,month: impl FnOnce(&T) -> Result<T,E>,
    day: impl FnOnce(&T) -> Result<T,E>) -> Result<(T,T,T),E> {
    Ok((year(value)?,month(value)?,day(value)?))
}

/// Packed EX dates have a caller-specific year operation (shift for minute
/// stamps, floor division for range rows). Preserve all three evaluation steps.
pub fn packed_date_parts_with<T,E>(value: &T,
    year: impl FnOnce(&T) -> Result<T,E>,month: impl FnOnce(&T) -> Result<T,E>,
    day: impl FnOnce(&T) -> Result<T,E>) -> Result<(T,T,T),E> {
    Ok((year(value)?,month(value)?,day(value)?))
}

pub fn clock_parts_with<T,E>(value: &T,hour: impl FnOnce(&T) -> Result<T,E>,
    minute: impl FnOnce(&T) -> Result<T,E>) -> Result<(T,T),E> {
    Ok((hour(value)?,minute(value)?))
}

pub fn seconds_parts_with<T,E>(value: &T,hour: impl FnOnce(&T) -> Result<T,E>,
    minute: impl FnOnce(&T) -> Result<T,E>,second: impl FnOnce(&T) -> Result<T,E>) -> Result<(T,T,T),E> {
    Ok((hour(value)?,minute(value)?,second(value)?))
}

/// Price arithmetic is shared; profile approval, input validity and rounding
/// remain the caller's policy. Python preserves its legacy unapproved rules.
pub fn limit_price_with<T,E>(reference: &T,ratio: &T,upper: bool,
    add_one: impl FnOnce(&T) -> Result<T,E>,subtract_from_one: impl FnOnce(&T) -> Result<T,E>,
    multiply: impl FnOnce(&T,T) -> Result<T,E>) -> Result<T,E> {
    let factor = if upper {add_one(ratio)?} else {subtract_from_one(ratio)?};
    multiply(reference,factor)
}

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

pub fn ratio_with<T, E>(numerator: T, denominator: T,
    divide: impl FnOnce(T, T) -> Result<T, E>) -> Result<T, E> {
    divide(numerator, denominator)
}

/// Optional policy selection happens before these shared price-state rules.
/// Lazy predicates preserve original short-circuit field access.
pub fn classify_limit_state<E>(last_at_limit: bool, sealed: impl FnOnce() -> Result<bool, E>,
    touched: impl FnOnce() -> Result<bool, E>) -> Result<crate::LimitStatus, E> {
    if last_at_limit && sealed()? { Ok(crate::LimitStatus::Sealed) }
    else if touched()? { Ok(crate::LimitStatus::Touched) }
    else { Ok(crate::LimitStatus::NotAtLimit) }
}

pub fn ladder_level_with<T, E>(prior: T, same_day: bool, previous_day: bool,
    increment: impl FnOnce(T) -> Result<T, E>, floor: impl Fn(T) -> Result<T, E>) -> Result<Option<T>, E> {
    if same_day { Ok(Some(floor(prior)?)) }
    else if previous_day { Ok(Some(floor(increment(prior)?)?)) }
    else { Ok(None) }
}

pub fn include_ladder_row(sealed: bool, touched: bool, include_touched: bool) -> bool {
    sealed || (include_touched && touched)
}

pub fn auction_ready(same_server_date: bool, before_auction: bool) -> bool {
    !(same_server_date && before_auction)
}

/// Apply a single inverse corporate action without imposing the caller's
/// numeric validation, rounding, or exception policy.
pub fn inverse_affine_with<T, E>(value: T, multiplier: &T, offset: &T,
    subtract: impl FnOnce(T, &T) -> Result<T, E>,
    divide: impl FnOnce(T, &T) -> Result<T, E>) -> Result<T, E> {
    divide(subtract(value, offset)?, multiplier)
}

pub fn percentage_change_with<T, E>(price: T, base: &T,
    subtract: impl FnOnce(T, &T) -> Result<T, E>,
    divide: impl FnOnce(T, &T) -> Result<T, E>,
    hundred: impl FnOnce(T) -> Result<T, E>) -> Result<T, E> {
    hundred(divide(subtract(price, base)?, base)?)
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
    fn seconds_projection_keeps_precision_evaluation_order_and_original_failure() {
        use std::cell::RefCell;
        let order = RefCell::new(Vec::new());
        let result = seconds_parts_with::<_,&str>(&34201_u32,
            |raw| {order.borrow_mut().push("hour");Ok(raw/3600)},
            |raw| {order.borrow_mut().push("minute");Ok((raw%3600)/60)},
            |raw| {order.borrow_mut().push("second");Ok(raw%60)}).unwrap();
        assert_eq!(result,(9,30,1)); assert_eq!(*order.borrow(),["hour","minute","second"]);
        assert_eq!(seconds_parts_with::<_,&str>(&0,|_| Ok(0),|_| Err("minute failure"),
            |_| panic!("minute failed")),Err("minute failure"));
    }

    #[test]
    fn limit_scalar_uses_selected_factor_before_multiplication_and_no_rounding() {
        use std::cell::RefCell;
        let order = RefCell::new(Vec::new());
        let upper = limit_price_with::<_,&str>(&10.0,&0.2,true,
            |ratio| {order.borrow_mut().push("add");Ok(1.0+ratio)},
            |_| panic!("upper does not subtract"),
            |reference,factor| {order.borrow_mut().push("multiply");Ok(reference*factor)}).unwrap();
        assert_eq!(upper,12.0); assert_eq!(*order.borrow(),["add","multiply"]);
        let lower = limit_price_with::<_,&str>(&10.0,&0.2,false,
            |_| panic!("lower does not add"),|ratio| Ok(1.0-ratio),|reference,factor| Ok(reference*factor)).unwrap();
        assert_eq!(lower,8.0);
        assert_eq!(limit_price_with::<_,&str>(&1.0,&0.1,true,|_| Err("factor failure"),
            |_| panic!("upper does not subtract"),|_,_| panic!("factor failed")),Err("factor failure"));
    }

    #[test]
    fn packed_projection_preserves_unvalidated_components_and_error_identity() {
        use std::cell::RefCell;
        let steps = RefCell::new(Vec::new());
        let raw = 22_u32*2048+1399;
        let result = packed_date_parts_with::<_,&str>(&raw,
            |raw| { steps.borrow_mut().push("year"); Ok((raw>>11)+2004) },
            |raw| { steps.borrow_mut().push("month"); Ok((raw%2048)/100) },
            |raw| { steps.borrow_mut().push("day"); Ok((raw%2048)%100) }).unwrap();
        assert_eq!(result,(2026,13,99));
        assert_eq!(*steps.borrow(),["year","month","day"]);
        let failure = "month arithmetic";
        assert_eq!(packed_date_parts_with::<_,&str>(&raw,|_| Ok(2026),|_| Err(failure),
            |_| panic!("month failed")),Err(failure));
    }

    #[test]
    fn clock_projection_has_no_implicit_day_wrap_and_stops_after_hour_error() {
        for (raw,expected) in [(0,(0,0)),(59,(0,59)),(60,(1,0)),(1439,(23,59)),(1441,(24,1))] {
            assert_eq!(clock_parts_with::<_,() >(&raw,|raw| Ok(raw/60),|raw| Ok(raw%60)),Ok(expected));
        }
        assert_eq!(clock_parts_with::<_,&str>(&1,|_| Err("hour arithmetic"),
            |_| panic!("hour failed")),Err("hour arithmetic"));
    }

    #[test]
    fn ymd_projection_keeps_order_and_stops_on_original_error() {
        use std::cell::RefCell;
        let steps = RefCell::new(Vec::new());
        let result = ymd_parts_with::<_,&str>(&20261117,
            |raw| { steps.borrow_mut().push("year"); Ok(raw/10000) },
            |raw| { steps.borrow_mut().push("month"); Ok((raw%10000)/100) },
            |raw| { steps.borrow_mut().push("day"); Ok(raw%100) }).unwrap();
        assert_eq!(result,(2026,11,17)); assert_eq!(*steps.borrow(),["year","month","day"]);
        assert_eq!(ymd_parts_with::<_,&str>(&1,|_| Err("source division"),|_| panic!("year failed"),|_| panic!("year failed")),Err("source division"));
    }

    #[test]
    fn inverse_action_keeps_subtraction_before_division() {
        let mut steps = Vec::new();
        let after_subtraction = inverse_affine_with::<_, ()>(10.0, &2.0, &1.0,
            |value, offset| { steps.push((value, *offset)); Ok(value - offset) },
            |value, multiplier| Ok(value / multiplier)).unwrap();
        assert_eq!(after_subtraction, 4.5);
        assert_eq!(steps, [(10.0, 1.0)]);
    }

    #[test]
    fn percentage_keeps_difference_ratio_then_hundred_without_rounding() {
        let value = percentage_change_with::<_, ()>(12.5, &10.0,
            |price, base| Ok(price - base), |difference, base| Ok(difference / base), |ratio| Ok(ratio * 100.0)).unwrap();
        assert_eq!(value, 25.0);
        let negative = percentage_change_with::<_, ()>(10.0, &-10.0,
            |price, base| Ok(price - base), |difference, base| Ok(difference / base), |ratio| Ok(ratio * 100.0)).unwrap();
        assert_eq!(negative, -200.0);
    }

    #[test]
    fn limit_state_preserves_lazy_predicates_and_trading_day_progression() {
        let state = classify_limit_state::<()>(true, || Ok(true), || panic!("sealed does not evaluate touched")).unwrap();
        assert_eq!(state, crate::LimitStatus::Sealed);
        let state = classify_limit_state::<()>(false, || panic!("last is not at limit"), || Ok(true)).unwrap();
        assert_eq!(state, crate::LimitStatus::Touched);
        assert_eq!(ladder_level_with::<_, ()>(0_u32, true, false, |_| panic!("same day"), |n| Ok(n.max(1))).unwrap(), Some(1));
        assert_eq!(ladder_level_with::<_, ()>(3_u32, false, true, |n| Ok(n + 1), |n| Ok(n.max(1))).unwrap(), Some(4));
        assert_eq!(ladder_level_with::<_, ()>(3_u32, false, false, |_| panic!("unrelated day"), |_| panic!("unrelated day")).unwrap(), None);
        assert!(!auction_ready(true, true));
        assert!(auction_ready(false, true));
        assert!(auction_ready(true, false));
        assert!(include_ladder_row(true, false, false));
        assert!(!include_ladder_row(false, true, false));
    }

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
