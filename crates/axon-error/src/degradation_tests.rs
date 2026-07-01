use super::*;

#[test]
fn degradable_carries_reason() {
    let policy = DegradationPolicy::degradable("optional graph extraction failed");
    assert!(policy.degradable);
    assert_eq!(
        policy.reason.as_deref(),
        Some("optional graph extraction failed")
    );
}

#[test]
fn not_degradable_has_no_reason() {
    let policy = DegradationPolicy::not_degradable();
    assert!(!policy.degradable);
    assert!(policy.reason.is_none());
}

#[test]
fn round_trips_serde() {
    let policy = DegradationPolicy::degradable("fallback used");
    let value = serde_json::to_value(&policy).unwrap();
    let back: DegradationPolicy = serde_json::from_value(value).unwrap();
    assert_eq!(back, policy);
}
