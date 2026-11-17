//! Checked progress shared by MAC collectors. These policies belong only to
//! commands whose source loop establishes empty/short response termination.
//! Ordinary EX single-page decoders do not acquire that evidence by reuse.
use std::collections::VecDeque;
use std::num::{NonZeroU16, NonZeroU32};
use crate::{Error, ErrorKind, PageOptions, Result};
use crate::ex::models::PageCompletion;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Target { Count(NonZeroU32), All }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EndPolicy { EmptyOrShort, EmptyOnly }
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PageRequest { offset: u32, count: NonZeroU16 }
impl PageRequest {
    pub(crate) const fn offset(self) -> u32 { self.offset }
    pub(crate) const fn count(self) -> u16 { self.count.get() }
}

pub(crate) struct Pagination {
    target: Target, policy: PageOptions, end_policy: EndPolicy,
    wire_maximum: NonZeroU16, offset_maximum: u32,
    offset: u32, rows: usize, pages: usize,
    pending: Option<PageRequest>, completion: Option<PageCompletion>,
}
impl Pagination {
    pub(crate) fn new(start: u32, target: Target, policy: PageOptions,
        wire_maximum: NonZeroU16, offset_maximum: u32, end_policy: EndPolicy) -> Result<Self> {
        super::query::validate_pages(&policy, usize::from(wire_maximum.get()))?;
        if start > offset_maximum { return Err(super::query::invalid("MAC starting page offset exceeds its wire field")); }
        if let Target::Count(count) = target { super::query::span(start, count)?; }
        Ok(Self { target, policy, end_policy, wire_maximum, offset_maximum,
            offset: start, rows: 0, pages: 0, pending: None, completion: None })
    }
    pub(crate) fn next_request(&mut self) -> Result<Option<PageRequest>> {
        if self.pending.is_some() { return Err(protocol("MAC page response must be accepted before another request")); }
        if self.completion.is_some() { return Ok(None); }
        if self.pages >= self.policy.max_pages() || self.rows >= self.policy.max_records() {
            return Err(self.limit("MAC collection reached its safety limit without terminal evidence"));
        }
        if self.offset > self.offset_maximum { return Err(self.limit("MAC next offset exceeds its wire field")); }
        let remaining = match self.target {
            Target::Count(count) => usize::try_from(count.get()).map_err(|_| self.limit("MAC target exceeds platform capacity"))?
                .checked_sub(self.rows).ok_or_else(|| protocol("MAC consumed rows exceed its requested target"))?,
            Target::All => usize::MAX,
        };
        let count = remaining.min(self.policy.page_size()).min(self.policy.max_records() - self.rows)
            .min(usize::from(self.wire_maximum.get()));
        let count = u16::try_from(count).ok().and_then(NonZeroU16::new)
            .ok_or_else(|| protocol("MAC page has no positive representable request allowance"))?;
        let request = PageRequest { offset: self.offset, count };
        self.pending = Some(request);
        Ok(Some(request))
    }
    /// Call only after the complete declared response has decoded. Progress is
    /// based on wire rows, before any projection/filtering/aggregation.
    pub(crate) fn accept(&mut self, wire_rows: usize) -> Result<()> {
        let request = self.pending.ok_or_else(|| protocol("unsolicited MAC pagination response"))?;
        if wire_rows > usize::from(request.count()) {
            return Err(protocol("MAC server returned more rows than the checked page request"));
        }
        let consumed = u32::try_from(wire_rows).map_err(|_| protocol("MAC wire row count exceeds u32"))?;
        let next = self.offset.checked_add(consumed).ok_or_else(|| self.limit("MAC next offset overflows u32"))?;
        let rows = self.rows.checked_add(wire_rows).ok_or_else(|| self.limit("MAC consumed count overflows platform capacity"))?;
        let pages = self.pages.checked_add(1).ok_or_else(|| self.limit("MAC page counter overflows platform capacity"))?;
        let ended = wire_rows == 0 || (self.end_policy == EndPolicy::EmptyOrShort && wire_rows < usize::from(request.count()));
        let requested = matches!(self.target, Target::Count(count) if u64::from(count.get()) == rows as u64);
        self.offset = next; self.rows = rows; self.pages = pages; self.pending = None;
        self.completion = if ended { Some(PageCompletion::EndOfData) }
            else if requested { Some(PageCompletion::RequestedCount) } else { None };
        Ok(())
    }
    pub(crate) const fn next_offset(&self) -> u32 { self.offset }
    pub(crate) const fn pages(&self) -> usize { self.pages }
    pub(crate) const fn rows(&self) -> usize { self.rows }
    pub(crate) const fn completion(&self) -> Option<PageCompletion> { self.completion }
    fn limit(&self, message: &'static str) -> Error {
        Error::new(ErrorKind::LimitExceeded, message).with_context("domain", "mac")
            .with_context("offset", format!("{}", self.offset))
            .with_context("pages", format!("{}", self.pages))
            .with_context("wire_rows", format!("{}", self.rows))
    }
}
fn protocol(message: &'static str) -> Error { Error::new(ErrorKind::Protocol, message).with_context("domain", "mac") }

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MergeOrder { Append, PrependPages }
/// Collect whole pages without repeated front-copying. Row order inside a
/// page and duplicate occurrences both survive the final flattening.
pub(crate) struct PageRows<T> { order: MergeOrder, pages: VecDeque<Vec<T>> }
impl<T> PageRows<T> {
    pub(crate) fn new(order: MergeOrder) -> Self { Self { order, pages: VecDeque::new() } }
    pub(crate) fn push(&mut self, rows: Vec<T>) {
        match self.order { MergeOrder::Append => self.pages.push_back(rows), MergeOrder::PrependPages => self.pages.push_front(rows) }
    }
    pub(crate) fn into_rows(self) -> Vec<T> { self.pages.into_iter().flatten().collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(target: Target, pages: PageOptions, end: EndPolicy) -> Pagination {
        Pagination::new(0, target, pages, NonZeroU16::new(80).unwrap(), u32::MAX, end).unwrap()
    }
    #[test]
    fn final_page_uses_remainder_and_wire_rows_preserve_occurrences() {
        let mut page = state(Target::Count(NonZeroU32::new(100).unwrap()), PageOptions::new(80, 20, 1000).unwrap(), EndPolicy::EmptyOrShort);
        assert_eq!(page.next_request().unwrap().unwrap(), PageRequest { offset: 0, count: NonZeroU16::new(80).unwrap() });
        page.accept(80).unwrap();
        assert_eq!(page.next_request().unwrap().unwrap(), PageRequest { offset: 80, count: NonZeroU16::new(20).unwrap() });
        page.accept(20).unwrap();
        assert_eq!(page.completion(), Some(PageCompletion::RequestedCount));
        assert_eq!(page.next_offset(), 100);
        assert!(page.next_request().unwrap().is_none());
        assert_eq!(page.pages(), 2);
    }
    #[test]
    fn full_safety_cap_does_not_prove_all_rows_collected() {
        let mut page = state(Target::All, PageOptions::new(80, 1, 80).unwrap(), EndPolicy::EmptyOrShort);
        page.next_request().unwrap(); page.accept(80).unwrap();
        assert_eq!(page.next_request().unwrap_err().kind(), ErrorKind::LimitExceeded);
        assert_eq!(page.completion(), None);
    }
    #[test]
    fn short_page_ends_only_for_a_policy_with_that_evidence() {
        let mut short = state(Target::All, PageOptions::new(80, 5, 100).unwrap(), EndPolicy::EmptyOrShort);
        short.next_request().unwrap(); short.accept(10).unwrap();
        assert_eq!(short.completion(), Some(PageCompletion::EndOfData));
        let mut only_empty = state(Target::All, PageOptions::new(80, 5, 100).unwrap(), EndPolicy::EmptyOnly);
        only_empty.next_request().unwrap(); only_empty.accept(10).unwrap();
        assert_eq!(only_empty.completion(), None);
        assert_eq!(only_empty.next_request().unwrap().unwrap().offset(), 10);
        only_empty.accept(0).unwrap();
        assert_eq!(only_empty.completion(), Some(PageCompletion::EndOfData));
    }
    #[test]
    fn smaller_record_budget_limits_request_but_not_claimed_completeness() {
        let mut page = state(Target::Count(NonZeroU32::new(100).unwrap()), PageOptions::new(80, 10, 90).unwrap(), EndPolicy::EmptyOrShort);
        page.next_request().unwrap(); page.accept(80).unwrap();
        assert_eq!(page.next_request().unwrap().unwrap().count(), 10);
        page.accept(10).unwrap();
        assert_eq!(page.rows(), 90);
        assert_eq!(page.next_request().unwrap_err().kind(), ErrorKind::LimitExceeded);
    }
    #[test]
    fn malformed_progress_and_offset_wrap_cannot_be_success() {
        let mut page = state(Target::All, PageOptions::new(80, 10, 100).unwrap(), EndPolicy::EmptyOrShort);
        assert_eq!(page.accept(1).unwrap_err().kind(), ErrorKind::Protocol);
        page.next_request().unwrap();
        assert_eq!(page.next_request().unwrap_err().kind(), ErrorKind::Protocol);
        assert_eq!(page.accept(81).unwrap_err().kind(), ErrorKind::Protocol);
        let mut narrow = Pagination::new(u32::from(u16::MAX), Target::All,
            PageOptions::new(1, 3, 3).unwrap(), NonZeroU16::MIN, u32::from(u16::MAX), EndPolicy::EmptyOrShort).unwrap();
        narrow.next_request().unwrap(); narrow.accept(1).unwrap();
        assert_eq!(narrow.next_request().unwrap_err().kind(), ErrorKind::LimitExceeded);
    }
    #[test]
    fn prepend_reverses_only_pages_and_preserves_duplicate_rows() {
        let mut lists = PageRows::new(MergeOrder::Append);
        lists.push(vec![3, 4, 4]); lists.push(vec![1, 2]);
        assert_eq!(lists.into_rows(), [3, 4, 4, 1, 2]);
        let mut bars = PageRows::new(MergeOrder::PrependPages);
        bars.push(vec![3, 4, 4]); bars.push(vec![1, 2]);
        assert_eq!(bars.into_rows(), [1, 2, 3, 4, 4]);
    }
}
