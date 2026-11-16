//! Ordered calendar operations shared by strict snapshots and legacy dates.
pub fn calendar_contains<T: Ord>(days: &[T], date: &T) -> bool { days.binary_search(date).is_ok() }

pub fn calendar_days<T: Ord>(mut days: Vec<T>) -> Vec<T> {
    days.sort_unstable(); days.dedup(); days
}

pub fn calendar_window<T: Ord>(days: &[T], start: &T, end: &T) -> (usize, usize) {
    (days.partition_point(|date| date < start), days.partition_point(|date| date <= end))
}

pub fn calendar_neighbor<T: Ord>(days: &[T], date: &T, include_self: bool, forward: bool) -> Option<usize> {
    let index = days.partition_point(|at| if forward {
        if include_self { at < date } else { at <= date }
    } else if include_self { at <= date } else { at < date });
    if forward { (index < days.len()).then_some(index) } else { index.checked_sub(1) }
}

/// Python ordinal1 is Monday, and date.max has ordinal3652059.
pub fn legacy_weekday(ordinal: u32) -> bool { (ordinal - 1) % 7 < 5 }

pub fn legacy_weekday_range(start: u32, end: u32) -> Result<Vec<u32>, &'static str> {
    if start == 0 || end > 3_652_059 { return Err("date value out of range"); }
    let mut dates = Vec::new();
    for day in start..=end {
        if legacy_weekday(day) { dates.push(day); }
        // The original Python loop increments even after the final date.
        if day == 3_652_059 { return Err("date value out of range"); }
    }
    Ok(dates)
}

pub fn legacy_weekday_neighbor(ordinal: u32, include_self: bool, forward: bool) -> Result<u32, &'static str> {
    let step = |value: u32| if forward { value.checked_add(1).filter(|value| *value <= 3_652_059) }
        else { value.checked_sub(1).filter(|value| *value != 0) };
    let mut current = if include_self { Some(ordinal) } else { step(ordinal) }.ok_or("date value out of range")?;
    loop {
        if legacy_weekday(current) { return Ok(current); }
        current = step(current).ok_or("date value out of range")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inclusive_windows_and_neighbors_keep_duplicate_boundaries() {
        let days = [1, 3, 3, 5];
        assert_eq!(calendar_window(&days, &3, &4), (1, 3));
        assert_eq!(calendar_neighbor(&days, &3, true, true), Some(1));
        assert_eq!(calendar_neighbor(&days, &3, false, true), Some(3));
        assert_eq!(calendar_neighbor(&days, &3, true, false), Some(2));
        assert_eq!(calendar_neighbor(&days, &3, false, false), Some(0));
    }
    #[test]
    fn legacy_weekday_fallback_preserves_python_range_overflow() {
        assert_eq!(legacy_weekday_range(1, 7).unwrap(), [1, 2, 3, 4, 5]);
        assert!(legacy_weekday_range(3_652_059, 3_652_059).is_err());
        assert!(legacy_weekday_neighbor(1, false, false).is_err());
    }
}
