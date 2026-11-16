//! Authored public default-HQ helper source cases. Execution deferred.
use eltdx::{helpers::classify_fund_flow, AmountUnit};

#[test]
fn pure_fund_flow_is_available_without_optional_features_or_a_python_runtime() {
    let flow = classify_fund_flow(&[]).unwrap();
    assert_eq!(flow.classified_count(), 0); assert_eq!(flow.skipped_count(), 0);
    assert_eq!(flow.total_net().unit(), AmountUnit::Cny); assert_eq!(flow.total_net().value().as_f64(), 0.0);
}
