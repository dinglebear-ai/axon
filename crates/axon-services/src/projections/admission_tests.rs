use super::*;

#[test]
fn admission_enforces_global_and_per_caller_concurrency() {
    let mut policy = ProjectionBatchConfig::default();
    policy.global_max_in_flight_requests = 2;
    policy.caller_max_in_flight_requests = 1;
    let first = acquire_projection_admission("admission-concurrency-a", &policy).unwrap();
    assert!(acquire_projection_admission("admission-concurrency-a", &policy).is_err());
    let second = acquire_projection_admission("admission-concurrency-b", &policy).unwrap();
    assert!(acquire_projection_admission("admission-concurrency-c", &policy).is_err());
    drop((first, second));
    assert!(acquire_projection_admission("admission-concurrency-c", &policy).is_ok());
}

#[test]
fn admission_enforces_per_caller_start_rate() {
    let mut policy = ProjectionBatchConfig::default();
    policy.caller_rate_limit_per_minute = 2;
    let principal = "admission-rate-unique";
    drop(acquire_projection_admission(principal, &policy).unwrap());
    drop(acquire_projection_admission(principal, &policy).unwrap());
    let error = acquire_projection_admission(principal, &policy).unwrap_err();
    assert_eq!(error.code.0, "projection.caller_rate_limited");
}
