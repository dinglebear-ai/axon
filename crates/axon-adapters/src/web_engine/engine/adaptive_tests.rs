use super::*;
use axon_core::config::AdaptiveConcurrencyConfig;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

fn cfg(enabled: bool, min: usize, max: Option<usize>, crawl_limit: Option<usize>) -> Config {
    Config {
        adaptive_concurrency: AdaptiveConcurrencyConfig { enabled, min, max },
        crawl_concurrency_limit: crawl_limit,
        ..Config::default()
    }
}

async fn acquire_all(semaphore: Arc<Semaphore>, count: usize) -> Vec<OwnedSemaphorePermit> {
    let mut permits = Vec::with_capacity(count);
    for _ in 0..count {
        permits.push(semaphore.clone().acquire_owned().await.unwrap());
    }
    permits
}

#[test]
fn disabled_config_returns_none() {
    assert!(AdaptiveCrawlControl::from_config(&cfg(false, 1, Some(8), Some(8))).is_none());
}

#[test]
fn enabled_config_attaches_with_resolved_max_permits() {
    let control =
        AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(8), None)).expect("adaptive enabled");

    assert_eq!(control.snapshot().current_target, 8);
    assert_eq!(control.snapshot().available_permits, 8);
}

#[test]
fn ten_200_statuses_increase_target_by_one() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(8), Some(4)))
        .expect("adaptive enabled");

    for _ in 0..ADAPTIVE_INCREASE_THRESHOLD {
        control.record_status(200);
    }

    let snapshot = control.snapshot();
    assert_eq!(snapshot.current_target, 5);
    assert_eq!(snapshot.successes, ADAPTIVE_INCREASE_THRESHOLD);
    assert_eq!(snapshot.syncs, 1);
}

#[test]
fn one_429_decreases_target() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(8), Some(8)))
        .expect("adaptive enabled");

    control.record_status(429);

    let snapshot = control.snapshot();
    assert_eq!(snapshot.current_target, 4);
    assert_eq!(snapshot.failures, 1);
}

#[test]
fn one_503_decreases_target() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(8), Some(8)))
        .expect("adaptive enabled");

    control.record_status(503);

    assert_eq!(control.snapshot().current_target, 4);
}

#[test]
fn non_success_non_pressure_statuses_are_neutral() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(8), Some(4)))
        .expect("adaptive enabled");

    control.record_status(404);
    control.record_status(302);

    let snapshot = control.snapshot();
    assert_eq!(snapshot.current_target, 4);
    assert_eq!(snapshot.successes, 0);
    assert_eq!(snapshot.failures, 0);
}

#[test]
fn broadcast_lag_applies_negative_pressure() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(16), Some(16)))
        .expect("adaptive enabled");

    control.record_broadcast_lag(10);

    let snapshot = control.snapshot();
    assert_eq!(snapshot.lag_events, 1);
    assert_eq!(snapshot.failures, 8);
    assert!(snapshot.current_target < 16);
}

#[tokio::test]
async fn shrink_below_in_flight_reduces_target_but_does_not_cancel_or_retroactively_forget() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(4), Some(4)))
        .expect("adaptive enabled");
    let permits = acquire_all(control.semaphore.semaphore(), 4).await;

    control.record_status(503);

    assert_eq!(control.snapshot().current_target, 2);
    assert_eq!(control.snapshot().available_permits, 0);
    drop(permits);
    assert_eq!(
        control.snapshot().available_permits,
        4,
        "Spider 2.52.0 only forgets currently available permits during set_target(); \
         all in-flight permits return after release, so the controller target shrinks \
         immediately but admission may temporarily exceed it until future resize support lands"
    );
}

#[tokio::test]
async fn repeated_failure_at_current_target_drains_returned_surplus_permits() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(4), Some(4)))
        .expect("adaptive enabled");
    let permits = acquire_all(control.semaphore.semaphore(), 4).await;

    control.record_status(503);
    control.record_status(503);
    assert_eq!(control.snapshot().current_target, 1);

    drop(permits);
    assert_eq!(control.snapshot().available_permits, 4);

    control.record_status(503);

    let snapshot = control.snapshot();
    assert_eq!(snapshot.current_target, 1);
    assert_eq!(snapshot.available_permits, 1);
}

#[test]
fn controller_resizes_same_semaphore_attached_to_website() {
    let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(8), Some(4)))
        .expect("adaptive enabled");
    let mut website = Website::new("https://example.com");
    control.attach_to(&mut website);
    let spider_semaphore = website.setup_semaphore();

    assert_eq!(spider_semaphore.available_permits(), 4);
    for _ in 0..ADAPTIVE_INCREASE_THRESHOLD {
        control.record_status(200);
    }

    assert_eq!(control.snapshot().current_target, 5);
    assert_eq!(spider_semaphore.available_permits(), 5);
}
