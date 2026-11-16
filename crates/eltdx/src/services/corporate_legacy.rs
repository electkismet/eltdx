//! Compatibility policies over the corporate domain's shared traversal.
//! These adapters contain no Python objects or dependency on its runtime.

use super::recovery::{ordered_tree_with, RecoveryStep};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CapitalPrefixError { Empty, Mismatch }

pub(crate) struct CapitalPrefix { consumed: usize, total: usize }

impl CapitalPrefix {
    pub(crate) fn new(total: usize) -> Self { Self { consumed: 0, total } }
    pub(crate) fn remaining(&self) -> Option<usize> {
        (self.consumed < self.total).then_some(self.consumed)
    }
    pub(crate) fn advance(&mut self, count: usize, matches: bool) -> Result<(), CapitalPrefixError> {
        if count == 0 { return Err(CapitalPrefixError::Empty); }
        if !matches || count > self.total - self.consumed { return Err(CapitalPrefixError::Mismatch); }
        self.consumed += count;
        Ok(())
    }
}

pub enum LegacyCapitalPage<T> { Blocks { response: T, count: usize }, Custom(T) }
pub enum LegacyCapitalCollection<T> { Pages(Vec<T>), Custom(T) }

pub trait LegacyCapitalSource {
    type Response;
    type Error;
    fn fetch(&mut self, start: usize) -> Result<LegacyCapitalPage<Self::Response>, Self::Error>;
    fn matches(&mut self, response: &Self::Response, start: usize, count: usize) -> Result<bool, Self::Error>;
    fn accept(&mut self, response: &Self::Response) -> Result<(), Self::Error>;
    fn protocol_error(&self, message: &str) -> Self::Error;
}

pub fn legacy_capital_collect<S: LegacyCapitalSource>(total: usize, source: &mut S)
    -> Result<LegacyCapitalCollection<S::Response>, S::Error> {
    let mut cursor = CapitalPrefix::new(total);
    let mut pages = Vec::new();
    while let Some(start) = cursor.remaining() {
        let (response, count) = match source.fetch(start)? {
            LegacyCapitalPage::Custom(response) => return Ok(LegacyCapitalCollection::Custom(response)),
            LegacyCapitalPage::Blocks { response, count } => (response, count),
        };
        // The original empty-page error precedes reading row identity properties.
        let matches = count != 0 && source.matches(&response, start, count)?;
        cursor.advance(count, matches).map_err(|error| source.protocol_error(match error {
            CapitalPrefixError::Empty => "capital_changes returned no blocks for a non-empty request",
            CapitalPrefixError::Mismatch => "capital_changes returned blocks outside the requested prefix",
        }))?;
        source.accept(&response)?;
        pages.push(response);
    }
    Ok(LegacyCapitalCollection::Pages(pages))
}

pub enum LegacyFinancePage<T> { Rows { response: T, count: usize }, Custom(T) }

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyFinanceError { pub retryable: bool, pub absent: bool }

impl LegacyFinanceError {
    pub fn from_message(message: &str) -> Self {
        Self {
            retryable: message.contains("finance batch") || message.starts_with("empty finance response")
                || message.starts_with("finance response count mismatch"),
            absent: message.starts_with("finance batch missing record:") || message.starts_with("empty finance response"),
        }
    }
}

pub trait LegacyFinanceSource {
    type Response;
    type Error;
    fn fetch(&mut self, indices: &[usize]) -> Result<LegacyFinancePage<Self::Response>, Self::Error>;
    fn classify(&self, error: &Self::Error) -> Result<LegacyFinanceError, Self::Error>;
    fn missing(&mut self, index: usize) -> Result<(), Self::Error>;
    fn protocol_error(&self, message: &str) -> Self::Error;
}

pub struct LegacyFinanceCollection<T> { pub responses: Vec<T>, pub recovered: bool }

pub fn legacy_finance_recover<S: LegacyFinanceSource>(indices: &[usize], source: &mut S)
    -> Result<LegacyFinanceCollection<S::Response>, S::Error> {
    let mut remaining = 32;
    // The shared tree never asks to split a singleton: the domain policy either
    // classifies it missing or propagates its original error first.
    ordered_tree_with(indices, false, &mut |part, recovery| {
        if recovery {
            if remaining == 0 {
                return Err(source.protocol_error("finance batch recovery request budget exhausted (32)"));
            }
            remaining -= 1;
        }
        let error = match source.fetch(part) {
            Ok(LegacyFinancePage::Custom(response)) => return Ok(RecoveryStep::Complete(
                LegacyFinanceCollection { responses: vec![response], recovered: recovery })),
            Ok(LegacyFinancePage::Rows { response, count }) => {
                if !part.is_empty() && count == 0 {
                    source.protocol_error("empty finance response")
                } else if count != part.len() {
                    source.protocol_error(&format!("finance response count mismatch: expected {}, got {count}", part.len()))
                } else {
                    return Ok(RecoveryStep::Complete(LegacyFinanceCollection {
                        responses: vec![response], recovered: recovery,
                    }));
                }
            }
            Err(error) => error,
        };
        let class = source.classify(&error)?;
        if !class.retryable || part.is_empty() { return Err(error); }
        if part.len() == 1 {
            if !class.absent { return Err(error); }
            source.missing(part[0])?;
            return Ok(RecoveryStep::Complete(LegacyFinanceCollection { responses: Vec::new(), recovered: true }));
        }
        Ok(RecoveryStep::Split)
    }, &|mut first, second| {
        first.responses.extend(second.responses);
        first.recovered |= second.recovered;
        Ok(first)
    }, &|| unreachable!("finance singleton split is rejected by its policy"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Finance { calls: Vec<Vec<usize>>, missing: Vec<usize> }
    impl LegacyFinanceSource for Finance {
        type Response = Vec<usize>;
        type Error = String;
        fn fetch(&mut self, part: &[usize]) -> Result<LegacyFinancePage<Self::Response>, String> {
            self.calls.push(part.to_vec());
            if part.len() > 1 || part == [1] { return Err("empty finance response".into()); }
            Ok(LegacyFinancePage::Rows { response: part.to_vec(), count: part.len() })
        }
        fn classify(&self, error: &String) -> Result<LegacyFinanceError, String> { Ok(LegacyFinanceError::from_message(error)) }
        fn missing(&mut self, index: usize) -> Result<(), String> { self.missing.push(index); Ok(()) }
        fn protocol_error(&self, message: &str) -> String { message.into() }
    }

    #[test]
    fn finance_tree_preserves_depth_first_order_and_missing_occurrences() {
        let mut source = Finance { calls: Vec::new(), missing: Vec::new() };
        let result = legacy_finance_recover(&[0, 1, 2], &mut source).unwrap();
        assert_eq!(source.calls, [vec![0, 1, 2], vec![0], vec![1, 2], vec![1], vec![2]]);
        assert_eq!(source.missing, [1]);
        assert_eq!(result.responses, [vec![0], vec![2]]);
        assert!(result.recovered);
    }

    #[test]
    fn finance_tree_debits_every_recovery_call_before_query() {
        let mut source = Finance { calls: Vec::new(), missing: Vec::new() };
        let result = legacy_finance_recover(&(0..40).collect::<Vec<_>>(), &mut source);
        assert_eq!(result.err().unwrap(), "finance batch recovery request budget exhausted (32)");
        assert_eq!(source.calls.len(), 33);
    }

    #[test]
    fn capital_cursor_rejects_empty_and_excess_without_advancing() {
        let mut cursor = CapitalPrefix::new(3);
        assert_eq!(cursor.advance(0, true), Err(CapitalPrefixError::Empty));
        assert_eq!(cursor.advance(4, true), Err(CapitalPrefixError::Mismatch));
        assert_eq!(cursor.remaining(), Some(0));
        cursor.advance(2, true).unwrap();
        assert_eq!(cursor.remaining(), Some(2));
        cursor.advance(1, true).unwrap();
        assert_eq!(cursor.remaining(), None);
    }
}
