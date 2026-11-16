//! Source rotation shared by strict and legacy money-flow recovery. Connection
//! ownership and error policy belong to the caller's existing transport domain.

pub(crate) struct ProbeOrder {
    total: usize,
    start: usize,
    step: usize,
    maximum: usize,
}

impl ProbeOrder {
    pub(crate) fn new(total: usize, start: usize, maximum: usize) -> Self {
        Self { total, start: if total == 0 { 0 } else { start % total }, step: 0,
            maximum: maximum.min(total) }
    }
}

impl Iterator for ProbeOrder {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        if self.step >= self.maximum { return None; }
        // Avoid overflowing start + step for generic callers with large lists.
        let index = if self.step >= self.total - self.start {
            self.step - (self.total - self.start)
        } else { self.start + self.step };
        self.step += 1;
        Some(index)
    }
}

/// Narrow callback boundary for legacy factories and custom result objects.
/// The source loop stays here; adapters own language objects and their lifetimes.
pub trait LegacyMoneyFlowSources {
    type Value;
    type Error;
    fn selected(&mut self) -> Result<Option<Self::Value>, Self::Error>;
    fn cursor(&mut self) -> Result<usize, Self::Error>;
    fn take(&mut self) -> Result<bool, Self::Error>;
    fn skip(&mut self, index: usize) -> Result<bool, Self::Error>;
    fn probe(&mut self, index: usize) -> Result<Option<Self::Value>, Self::Error>;
    fn advance(&mut self, cursor: usize) -> Result<(), Self::Error>;
}

pub fn legacy_money_flow_recover<S: LegacyMoneyFlowSources>(
    total: usize, sources: &mut S,
) -> Result<Option<S::Value>, S::Error> {
    if total == 0 { return Ok(None); }
    if let Some(value) = sources.selected()? { return Ok(Some(value)); }
    let start = sources.cursor()? % total;
    for index in ProbeOrder::new(total, start, 3) {
        // Legacy budgets count loop entries, including skipped preferred hosts.
        // Exhaustion returns without advancing the next security's cursor.
        if !sources.take()? { return Ok(None); }
        if sources.skip(index)? { continue; }
        if let Some(value) = sources.probe(index)? { return Ok(Some(value)); }
    }
    sources.advance(if start == total - 1 { 0 } else { start + 1 })?;
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Sources { budget: usize, cursor: usize, events: Vec<String>, found: Option<usize>, skip: Option<usize> }
    impl LegacyMoneyFlowSources for Sources {
        type Value = usize;
        type Error = &'static str;
        fn selected(&mut self) -> Result<Option<usize>, Self::Error> { self.events.push("selected".into()); Ok(None) }
        fn cursor(&mut self) -> Result<usize, Self::Error> { Ok(self.cursor) }
        fn take(&mut self) -> Result<bool, Self::Error> {
            self.events.push("take".into());
            if self.budget == 0 { return Ok(false); }
            self.budget -= 1; Ok(true)
        }
        fn skip(&mut self, index: usize) -> Result<bool, Self::Error> { Ok(self.skip == Some(index)) }
        fn probe(&mut self, index: usize) -> Result<Option<usize>, Self::Error> {
            self.events.push(format!("probe:{index}")); Ok(self.found.filter(|found| *found == index))
        }
        fn advance(&mut self, cursor: usize) -> Result<(), Self::Error> { self.cursor = cursor; Ok(()) }
    }

    #[test]
    fn skipped_sources_consume_budget_and_exhaustion_does_not_rotate() {
        let mut sources = Sources { budget: 2, cursor: 1, events: Vec::new(), found: None, skip: Some(1) };
        assert_eq!(legacy_money_flow_recover(4, &mut sources), Ok(None));
        assert_eq!(sources.cursor, 1);
        assert_eq!(sources.events, ["selected", "take", "take", "probe:2", "take"]);
    }

    #[test]
    fn full_empty_pass_rotates_but_success_preserves_cursor() {
        let mut sources = Sources { budget: 12, cursor: 3, events: Vec::new(), found: None, skip: None };
        assert_eq!(legacy_money_flow_recover(4, &mut sources), Ok(None));
        assert_eq!(sources.cursor, 0);
        assert_eq!(sources.events, ["selected", "take", "probe:3", "take", "probe:0", "take", "probe:1"]);
        sources.found = Some(1);
        assert_eq!(legacy_money_flow_recover(4, &mut sources), Ok(Some(1)));
        assert_eq!(sources.cursor, 0);
    }

    #[test]
    fn rotation_wraps_without_duplicate_indices() {
        assert_eq!(ProbeOrder::new(5, 4, 3).collect::<Vec<_>>(), [4, 0, 1]);
        assert_eq!(ProbeOrder::new(2, usize::MAX, 3).collect::<Vec<_>>(), [1, 0]);
        assert!(ProbeOrder::new(0, 10, 3).next().is_none());
    }
}
