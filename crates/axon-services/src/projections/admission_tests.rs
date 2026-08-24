use super::*;

#[test]
fn admission_enforces_global_and_per_caller_concurrency() {
    let limiter = ProjectionAdmissionLimiter::default();
    let mut policy = ProjectionBatchConfig::default();
    policy.global_max_in_flight_requests = 2;
    policy.caller_max_in_flight_requests = 1;
    let now = Instant::now();
    let first = limiter
        .acquire("admission-concurrency-a", &policy, now)
        .unwrap();
    assert!(
        limiter
            .acquire("admission-concurrency-a", &policy, now)
            .is_err()
    );
    let second = limiter
        .acquire("admission-concurrency-b", &policy, now)
        .unwrap();
    assert!(
        limiter
            .acquire("admission-concurrency-c", &policy, now)
            .is_err()
    );
    drop((first, second));
    assert!(
        limiter
            .acquire("admission-concurrency-c", &policy, now)
            .is_ok()
    );
}

#[test]
fn admission_enforces_per_caller_start_rate() {
    let limiter = ProjectionAdmissionLimiter::default();
    let mut policy = ProjectionBatchConfig::default();
    policy.caller_rate_limit_per_minute = 2;
    let principal = "admission-rate-unique";
    let now = Instant::now();
    drop(limiter.acquire(principal, &policy, now).unwrap());
    drop(limiter.acquire(principal, &policy, now).unwrap());
    let error = limiter.acquire(principal, &policy, now).unwrap_err();
    assert_eq!(error.code.0, "projection.caller_rate_limited");
    assert!(
        limiter
            .acquire(principal, &policy, now + Duration::from_secs(61))
            .is_ok()
    );
}

#[test]
fn simultaneous_callers_release_capacity_on_guard_drop() {
    let limiter = ProjectionAdmissionLimiter::default();
    let mut policy = ProjectionBatchConfig::default();
    policy.global_max_in_flight_requests = 1;
    policy.caller_max_in_flight_requests = 1;
    let now = Instant::now();
    let guard = limiter.acquire("first", &policy, now).unwrap();
    let contender = {
        let limiter = limiter.clone();
        let policy = policy.clone();
        std::thread::spawn(move || limiter.acquire("second", &policy, now).is_err())
    };
    assert!(contender.join().unwrap());
    drop(guard);
    assert!(limiter.acquire("second", &policy, now).is_ok());
}
