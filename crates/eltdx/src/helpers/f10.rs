//! HTTP helper operations. F10 is required; HQ is neither started nor used as
//! a fallback. All pages share one owner admission, deadline and work budget.
use std::time::Instant;
use crate::f10::{BoardMembersQuery, BoardMemberTable, IndexMembersQuery, IndexMemberTable,
    IndexTrackingProductsQuery, IndexTrackingTable, IpoCalendar};
use crate::{OperationOptions, Result};
use super::HelperService;

impl HelperService {
    pub fn index_members(&self, query: &IndexMembersQuery, options: &OperationOptions) -> Result<IndexMemberTable> {
        let context = self.f10.context(Instant::now(), "helpers.index_members", options)?;
        let result = self.f10.index_members_in_context(query, &context)?;
        context.finish()?; Ok(result)
    }
    /// All tracking products are retained. The result's etfs() is an explicit
    /// prefix heuristic; the source does not establish exchange membership.
    pub fn index_etfs(&self, query: &IndexTrackingProductsQuery, options: &OperationOptions) -> Result<IndexTrackingTable> {
        let context = self.f10.context(Instant::now(), "helpers.index_etfs", options)?;
        let result = self.f10.index_tracking_in_context(query, &context)?;
        context.finish()?; Ok(result)
    }
    pub fn board_members(&self, query: &BoardMembersQuery, options: &OperationOptions) -> Result<BoardMemberTable> {
        let context = self.f10.context(Instant::now(), "helpers.board_members", options)?;
        let result = self.f10.board_members_in_context(query, &context)?;
        context.finish()?; Ok(result)
    }
    pub fn ipo_calendar(&self, options: &OperationOptions) -> Result<IpoCalendar> {
        let context = self.f10.context(Instant::now(), "helpers.ipo_calendar", options)?;
        let result = self.f10.ipo_in_context(&context)?;
        context.finish()?; Ok(result)
    }
}
