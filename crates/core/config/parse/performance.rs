use super::super::types::PerformanceProfile;
use std::env;

/// Profile-derived settings returned by [`profile_settings`].
pub(super) struct ProfileSettings {
    pub crawl_concurrency: usize,
    pub backfill_concurrency: usize,
    pub request_timeout_ms: u64,
    pub fetch_retries: usize,
    pub retry_backoff_ms: u64,
    pub broadcast_buffer_min: usize,
    pub broadcast_buffer_max: usize,
}

/// Returns full profile settings including broadcast buffer sizes.
pub(super) fn profile_settings(profile: PerformanceProfile) -> ProfileSettings {
    let logical_cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);

    match profile {
        PerformanceProfile::HighStable => ProfileSettings {
            crawl_concurrency: (logical_cpus.saturating_mul(8)).clamp(64, 192),
            backfill_concurrency: (logical_cpus.saturating_mul(6)).clamp(32, 128),
            request_timeout_ms: 20_000,
            fetch_retries: 2,
            retry_backoff_ms: 250,
            broadcast_buffer_min: 4096,
            broadcast_buffer_max: 16_384,
        },
        PerformanceProfile::Extreme => ProfileSettings {
            crawl_concurrency: (logical_cpus.saturating_mul(16)).clamp(128, 384),
            backfill_concurrency: (logical_cpus.saturating_mul(10)).clamp(64, 256),
            request_timeout_ms: 15_000,
            fetch_retries: 1,
            retry_backoff_ms: 100,
            broadcast_buffer_min: 8_192,
            broadcast_buffer_max: 32_768,
        },
        PerformanceProfile::Balanced => ProfileSettings {
            crawl_concurrency: (logical_cpus.saturating_mul(4)).clamp(32, 96),
            backfill_concurrency: (logical_cpus.saturating_mul(3)).clamp(16, 64),
            request_timeout_ms: 30_000,
            fetch_retries: 2,
            retry_backoff_ms: 300,
            broadcast_buffer_min: 4096,
            broadcast_buffer_max: 8_192,
        },
        PerformanceProfile::Max => ProfileSettings {
            crawl_concurrency: (logical_cpus.saturating_mul(24)).clamp(256, 1024),
            backfill_concurrency: (logical_cpus.saturating_mul(20)).clamp(128, 1024),
            request_timeout_ms: 12_000,
            fetch_retries: 1,
            retry_backoff_ms: 50,
            broadcast_buffer_min: 16_384,
            broadcast_buffer_max: 65_536,
        },
    }
}

pub(super) fn env_usize_clamped(key: &str, default: usize, min: usize, max: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

/// Like `env_usize_clamped` but returns `None` when the var is absent or unparseable,
/// so TOML and hardcoded defaults can be consulted.
pub(super) fn env_usize_opt(key: &str, min: usize, max: usize) -> Option<usize> {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .map(|v| v.clamp(min, max))
}

pub(super) fn env_f64_clamped(key: &str, default: f64, min: f64, max: f64) -> f64 {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(default)
        .clamp(min, max)
}

/// Like `env_f64_clamped` but returns `None` when the var is absent or unparseable.
pub(super) fn env_f64_opt(key: &str, min: f64, max: f64) -> Option<f64> {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .map(|v| v.clamp(min, max))
}
