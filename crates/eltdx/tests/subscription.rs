//! Public policy source cases. Network/owner integration cases are added with
//! the active subscription service; nothing in this file has been executed.
use std::time::Duration;
use eltdx::{GapPolicy, SubscriptionOptions};
use eltdx::types::RefreshRequest;

#[test]
fn subscription_policy_is_independent_of_setup_deadlines_and_has_frozen_defaults() {
    let policy = SubscriptionOptions::default();
    assert_eq!(policy.max_events(), 1024); assert_eq!(policy.max_bytes(), 64 * 1024 * 1024);
    assert_eq!(policy.renew_interval(), Some(Duration::from_secs(7)));
    assert_eq!(policy.gap_policy(), GapPolicy::ReportOnly);
    let changed = policy.clone().with_queue_limits(2, 512 * 1024).with_renew_interval(None)
        .with_gap_policy(GapPolicy::Resynchronize);
    assert_eq!(changed.max_events(), 2); assert_eq!(changed.renew_interval(), None);
    assert_eq!(policy.renew_interval(), Some(Duration::from_secs(7)));
}

#[test]
fn refresh_requests_retain_source_local_cursors_without_monotonic_reinterpretation() {
    let mut request = RefreshRequest::new("sh600000".parse().unwrap());
    assert_eq!(request.cursor(), 0);
    request = request.with_cursor(150000).unwrap().with_cursor(93000).unwrap();
    assert_eq!(request.cursor(), 93000);
    assert_eq!(request.id().to_string(), "sh600000");
}
