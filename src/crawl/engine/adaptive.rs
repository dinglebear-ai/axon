use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use spider::utils::adaptive_concurrency::{AIMDController, AdaptiveSemaphore};
use spider::website::Website;

use crate::core::config::Config;

const ADAPTIVE_INCREASE_THRESHOLD: usize = 10;
const ADAPTIVE_DECREASE_FACTOR: f64 = 0.5;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AdaptiveCrawlSnapshot {
    pub successes: usize,
    pub failures: usize,
    pub lag_events: usize,
    pub syncs: usize,
    pub current_target: usize,
    pub available_permits: usize,
}

impl AdaptiveCrawlSnapshot {
    pub(crate) fn log_summary(&self) -> String {
        format!(
            "target={} available={} successes={} failures={} lag_events={} syncs={}",
            self.current_target,
            self.available_permits,
            self.successes,
            self.failures,
            self.lag_events,
            self.syncs
        )
    }
}

#[derive(Clone)]
pub(crate) struct AdaptiveCrawlControl {
    semaphore: AdaptiveSemaphore,
    controller: Arc<AIMDController>,
    successes: Arc<AtomicUsize>,
    failures: Arc<AtomicUsize>,
    lag_events: Arc<AtomicUsize>,
    syncs: Arc<AtomicUsize>,
    last_target: Arc<AtomicUsize>,
}

impl AdaptiveCrawlControl {
    pub(crate) fn from_config(cfg: &Config) -> Option<Self> {
        if !cfg.adaptive_concurrency.enabled {
            return None;
        }

        let min = cfg.adaptive_concurrency.min.max(1);
        let max = cfg
            .adaptive_concurrency
            .max
            .unwrap_or_else(|| cfg.crawl_concurrency_limit.unwrap_or(min))
            .max(min);
        let initial = cfg.crawl_concurrency_limit.unwrap_or(max).clamp(min, max);
        let semaphore = AdaptiveSemaphore::new(initial);
        let controller = Arc::new(AIMDController::new(
            initial,
            min,
            max,
            ADAPTIVE_INCREASE_THRESHOLD,
            ADAPTIVE_DECREASE_FACTOR,
        ));

        Some(Self {
            semaphore,
            controller,
            successes: Arc::new(AtomicUsize::new(0)),
            failures: Arc::new(AtomicUsize::new(0)),
            lag_events: Arc::new(AtomicUsize::new(0)),
            syncs: Arc::new(AtomicUsize::new(0)),
            last_target: Arc::new(AtomicUsize::new(initial)),
        })
    }

    pub(crate) fn attach_to(&self, website: &mut Website) {
        website.with_adaptive_concurrency(&self.semaphore);
    }

    pub(crate) fn record_status(&self, status: u16) {
        if status == 429 || status >= 500 {
            self.record_failure();
        } else {
            self.record_success();
        }
    }

    pub(crate) fn record_broadcast_lag(&self, dropped: u64) {
        if dropped == 0 {
            return;
        }
        self.lag_events.fetch_add(1, Ordering::Relaxed);
        let failures = dropped.clamp(1, 8);
        for _ in 0..failures {
            self.record_failure();
        }
    }

    pub(crate) fn snapshot(&self) -> AdaptiveCrawlSnapshot {
        AdaptiveCrawlSnapshot {
            successes: self.successes.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            lag_events: self.lag_events.load(Ordering::Relaxed),
            syncs: self.syncs.load(Ordering::Relaxed),
            current_target: self.controller.current_limit(),
            available_permits: self.semaphore.available(),
        }
    }

    fn record_success(&self) {
        let before = self.controller.current_limit();
        self.controller.record_success();
        self.successes.fetch_add(1, Ordering::Relaxed);
        self.sync_if_target_changed(before);
    }

    fn record_failure(&self) {
        let before = self.controller.current_limit();
        self.controller.record_failure();
        self.failures.fetch_add(1, Ordering::Relaxed);
        self.sync_if_target_changed(before);
    }

    fn sync_if_target_changed(&self, before: usize) {
        let after = self.controller.current_limit();
        if before == after {
            return;
        }
        self.semaphore.sync_from(&self.controller);
        self.last_target.store(after, Ordering::Relaxed);
        self.syncs.fetch_add(1, Ordering::Relaxed);
    }
}

pub(crate) fn warnings_for_config(cfg: &Config) -> Vec<String> {
    if !cfg.adaptive_concurrency.enabled {
        return Vec::new();
    }

    let mut warnings = Vec::new();
    if !cfg.respect_robots {
        warnings.push("adaptive concurrency is enabled while respect-robots is false".to_string());
    }
    if cfg.delay_ms == 0 {
        warnings.push("adaptive concurrency is enabled without a crawl delay".to_string());
    }
    if cfg.max_pages == 0 {
        warnings.push("adaptive concurrency is enabled with uncapped max-pages".to_string());
    }
    if cfg.path_budgets.is_empty() && cfg.url_whitelist.is_empty() {
        warnings.push(
            "adaptive concurrency is enabled without path budgets or URL whitelist".to_string(),
        );
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::AdaptiveConcurrencyConfig;

    fn cfg(enabled: bool, min: usize, max: Option<usize>, crawl_limit: Option<usize>) -> Config {
        Config {
            adaptive_concurrency: AdaptiveConcurrencyConfig { enabled, min, max },
            crawl_concurrency_limit: crawl_limit,
            ..Config::default()
        }
    }

    #[test]
    fn disabled_config_returns_none() {
        assert!(AdaptiveCrawlControl::from_config(&cfg(false, 1, Some(8), Some(8))).is_none());
    }

    #[test]
    fn enabled_config_attaches_with_resolved_max_permits() {
        let control = AdaptiveCrawlControl::from_config(&cfg(true, 1, Some(8), None))
            .expect("adaptive enabled");

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
        let semaphore = control.semaphore.semaphore();
        let p1 = semaphore.clone().acquire_owned().await.unwrap();
        let p2 = semaphore.clone().acquire_owned().await.unwrap();
        let p3 = semaphore.clone().acquire_owned().await.unwrap();
        let p4 = semaphore.clone().acquire_owned().await.unwrap();

        control.record_status(503);

        assert_eq!(control.snapshot().current_target, 2);
        assert_eq!(control.snapshot().available_permits, 0);
        drop((p1, p2, p3, p4));
        assert_eq!(
            control.snapshot().available_permits,
            4,
            "Spider 2.52.0 only forgets currently available permits during set_target(); \
             all in-flight permits return after release, so the controller target shrinks \
             immediately but admission may temporarily exceed it until future resize support lands"
        );
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
}
