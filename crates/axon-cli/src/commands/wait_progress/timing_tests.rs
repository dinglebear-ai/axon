use std::time::Duration;

use super::*;

#[test]
fn eta_waits_for_two_samples_spanning_one_second() {
    let mut timing = TimingEstimator::default();
    assert_eq!(timing.sample(Duration::ZERO, 0, Some(100)), None);
    assert_eq!(
        timing.sample(Duration::from_millis(500), 20, Some(100)),
        None
    );
    let estimate = timing
        .sample(Duration::from_secs(1), 40, Some(100))
        .unwrap();
    assert_eq!(estimate.per_second.round() as u64, 40);
    assert_eq!(estimate.remaining, Duration::from_millis(1500));
}

#[test]
fn phase_or_denominator_change_resets_timing() {
    let mut timing = TimingEstimator::default();
    timing.sample(Duration::ZERO, 0, Some(10));
    timing.sample(Duration::from_secs(1), 5, Some(10));
    timing.reset();
    assert_eq!(timing.sample(Duration::from_secs(2), 1, Some(20)), None);
}

#[test]
fn regression_and_tiny_remaining_windows_suppress_estimates() {
    let mut timing = TimingEstimator::default();
    timing.sample(Duration::ZERO, 10, Some(100));
    assert_eq!(timing.sample(Duration::from_secs(1), 5, Some(100)), None);

    timing.reset();
    timing.sample(Duration::ZERO, 0, Some(10));
    assert_eq!(timing.sample(Duration::from_secs(1), 10, Some(10)), None);
}
