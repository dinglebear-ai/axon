use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use spider::utils::adaptive_concurrency::{AIMDController, AdaptiveSemaphore};
use spider::website::Website;

use axon_core::config::Config;

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
        })
    }

    pub(crate) fn attach_to(&self, website: &mut Website) {
        website.with_adaptive_concurrency(&self.semaphore);
    }

    pub(crate) fn record_status(&self, status: u16) {
        if status == 429 || status >= 500 {
            self.record_failure();
        } else if (200..300).contains(&status) {
            self.record_success();
        }
    }

    pub(crate) fn record_content_success(&self) {
        self.record_success();
    }

    pub(crate) fn record_content_failure(&self) {
        self.record_failure();
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
        if !self.sync_if_target_changed(before) {
            self.forget_surplus_available_permits();
        }
    }

    fn sync_if_target_changed(&self, before: usize) -> bool {
        let after = self.controller.current_limit();
        if before == after {
            return false;
        }
        self.semaphore.sync_from(&self.controller);
        self.syncs.fetch_add(1, Ordering::Relaxed);
        true
    }

    fn forget_surplus_available_permits(&self) {
        let target = self.controller.current_limit();
        let available = self.semaphore.available();
        if available <= target {
            return;
        }
        self.semaphore
            .semaphore()
            .forget_permits(available - target);
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
#[path = "adaptive_tests.rs"]
mod tests;
