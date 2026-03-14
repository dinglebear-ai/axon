//! Process-wide rate limiter for WebSocket message categories.
//!
//! Extracted from `ws_handler.rs` to stay under the 500-line module limit.
//! Each IP gets independent sliding windows per category (execute vs read_file)
//! to prevent cross-category reset exploits.

use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

/// Maximum `execute` messages per IP per window (H-12, P1-2).
pub(crate) const RATE_LIMIT_WINDOW_SECS: u64 = 60;
pub(crate) const RATE_LIMIT_MAX_EXECUTES: u32 = 120;
/// Maximum `read_file` messages per IP per window (P3-4).
pub(crate) const RATE_LIMIT_MAX_READ_FILE: u32 = 60;

/// Unix timestamp (seconds) of the last TTL eviction sweep.
/// Updated at most once per `RATE_LIMIT_WINDOW_SECS` to amortize the O(N)
/// `retain` cost across all callers rather than paying it on every request.
static LAST_EVICTION_SECS: AtomicU64 = AtomicU64::new(0);

pub(crate) enum RateLimitCategory {
    Execute,
    ReadFile,
}

/// Check the process-wide rate limiter for a given message category (P1-2, P3-4).
/// Returns `true` if the request is allowed, `false` if rate-limited.
///
/// Each category has its own independent sliding window `(count, window_start)`.
/// This prevents a window reset in one category from zeroing the counter for the
/// other — without this separation, an attacker could time requests to force
/// cross-category resets and achieve ~2x the intended throughput.
///
/// After processing the current IP, performs a best-effort sweep to evict stale
/// entries (both windows older than the rate limit window). The sweep runs at
/// most once per window period to avoid O(N) cost on every request.
pub(crate) fn check_rate_limit(
    rate_limiter: &DashMap<IpAddr, (u32, Instant, u32, Instant)>,
    ip: IpAddr,
    category: RateLimitCategory,
) -> bool {
    let now = Instant::now();
    let window = Duration::from_secs(RATE_LIMIT_WINDOW_SECS);
    let mut entry = rate_limiter.entry(ip).or_insert((0, now, 0, now));
    let (exec_count, exec_window, read_count, read_window) = entry.value_mut();

    let allowed = match category {
        RateLimitCategory::Execute => {
            if now.duration_since(*exec_window) > window {
                *exec_count = 0;
                *exec_window = now;
            }
            *exec_count += 1;
            *exec_count <= RATE_LIMIT_MAX_EXECUTES
        }
        RateLimitCategory::ReadFile => {
            if now.duration_since(*read_window) > window {
                *read_count = 0;
                *read_window = now;
            }
            *read_count += 1;
            *read_count <= RATE_LIMIT_MAX_READ_FILE
        }
    };

    // Drop the entry ref before calling retain (which needs exclusive iteration).
    drop(entry);

    // Amortized TTL eviction: only sweep once per window period to avoid paying
    // the O(N) retain cost on every request. A compare-exchange ensures only one
    // thread triggers the sweep even under concurrent load.
    let now_unix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let last = LAST_EVICTION_SECS.load(Ordering::Relaxed);
    if now_unix.saturating_sub(last) >= RATE_LIMIT_WINDOW_SECS
        && LAST_EVICTION_SECS
            .compare_exchange(last, now_unix, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
    {
        rate_limiter.retain(|_, v| {
            now.duration_since(v.1).as_secs() < RATE_LIMIT_WINDOW_SECS
                || now.duration_since(v.3).as_secs() < RATE_LIMIT_WINDOW_SECS
        });
    }

    allowed
}
