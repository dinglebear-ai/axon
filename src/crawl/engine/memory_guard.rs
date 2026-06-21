use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::core::logging::log_warn;

pub(crate) const MEMORY_ABORT_PREFIX: &str = "crawl memory guard tripped";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy)]
pub(crate) struct MemorySnapshot {
    pub(crate) rss_bytes: u64,
    /// Total memory the guard measures RSS against. `NonZeroU64` makes a zero
    /// denominator unrepresentable, so the percentage below can never divide by
    /// zero — `linux_memory_snapshot` returns `None` rather than a zero total.
    pub(crate) total_bytes: NonZeroU64,
}

/// Whether RSS has reached `abort_percent` of total memory.
///
/// `abort_percent` is assumed positive: the spawn path only calls this with a
/// `Some(positive)` value, and the config layer clamps it to `1.0..=100.0` and
/// maps non-positive/non-finite input to `None` — the single "disabled"
/// encoding. `total_bytes` is `NonZeroU64`, so this never divides by zero.
pub(crate) fn should_abort_for_usage(snapshot: MemorySnapshot, abort_percent: f64) -> bool {
    let used_percent = (snapshot.rss_bytes as f64 / snapshot.total_bytes.get() as f64) * 100.0;
    used_percent >= abort_percent
}

pub(crate) fn is_memory_abort_message(message: &str) -> bool {
    message.contains(MEMORY_ABORT_PREFIX)
}

pub(crate) struct CrawlMemoryGuard {
    cancel: CancellationToken,
    abort_reason: Arc<Mutex<Option<String>>>,
}

impl CrawlMemoryGuard {
    pub(crate) fn spawn(crawl_id: &str, start_url: &str, abort_percent: Option<f64>) -> Self {
        let cancel = CancellationToken::new();
        let abort_reason = Arc::new(Mutex::new(None));
        let Some(abort_percent) = abort_percent else {
            return Self {
                cancel,
                abort_reason,
            };
        };

        let target = format!("{crawl_id}{start_url}");
        let url = start_url.to_string();
        let cancel_task = cancel.clone();
        let reason_task = Arc::clone(&abort_reason);
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(POLL_INTERVAL);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            let mut snapshot_warning_logged = false;
            loop {
                tokio::select! {
                    _ = cancel_task.cancelled() => break,
                    _ = ticker.tick() => {
                        let Some(snapshot) = linux_memory_snapshot() else {
                            if !snapshot_warning_logged {
                                log_warn("crawl memory guard could not read RSS/limit telemetry; guard will retry on the next poll");
                                snapshot_warning_logged = true;
                            }
                            continue;
                        };
                        if !should_abort_for_usage(snapshot, abort_percent) {
                            continue;
                        }
                        let reason = format!(
                            "{MEMORY_ABORT_PREFIX} for {url}: rss={} bytes total={} bytes threshold={abort_percent:.1}%",
                            snapshot.rss_bytes, snapshot.total_bytes
                        );
                        log_warn(&reason);
                        if let Ok(mut slot) = reason_task.lock() {
                            *slot = Some(reason);
                        }
                        spider::utils::shutdown(&target).await;
                        break;
                    }
                }
            }
        });

        Self {
            cancel,
            abort_reason,
        }
    }

    pub(crate) fn stop(&self) {
        self.cancel.cancel();
    }

    pub(crate) fn abort_reason(&self) -> Option<String> {
        self.abort_reason.lock().ok().and_then(|slot| slot.clone())
    }
}

impl Drop for CrawlMemoryGuard {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(target_os = "linux")]
fn linux_memory_snapshot() -> Option<MemorySnapshot> {
    let rss_bytes = read_status_rss_bytes()?;
    // A zero total is meaningless and would make the percentage undefined;
    // treat it as "no telemetry" so the guard simply retries.
    let total_bytes = NonZeroU64::new(effective_memory_total_bytes()?)?;
    Some(MemorySnapshot {
        rss_bytes,
        total_bytes,
    })
}

#[cfg(not(target_os = "linux"))]
fn linux_memory_snapshot() -> Option<MemorySnapshot> {
    None
}

#[cfg(target_os = "linux")]
fn read_status_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmRSS:")?.trim();
        parse_kib_line(value)
    })
}

#[cfg(target_os = "linux")]
fn read_meminfo_total_bytes() -> Option<u64> {
    let meminfo = std::fs::read_to_string("/proc/meminfo").ok()?;
    meminfo.lines().find_map(|line| {
        let value = line.strip_prefix("MemTotal:")?.trim();
        parse_kib_line(value)
    })
}

#[cfg(target_os = "linux")]
fn effective_memory_total_bytes() -> Option<u64> {
    match (read_cgroup_limit_bytes(), read_meminfo_total_bytes()) {
        (Some(cgroup), Some(host)) => Some(cgroup.min(host)),
        (Some(cgroup), None) => Some(cgroup),
        (None, Some(host)) => Some(host),
        (None, None) => None,
    }
}

#[cfg(target_os = "linux")]
fn read_cgroup_limit_bytes() -> Option<u64> {
    [
        "/sys/fs/cgroup/memory.max",
        "/sys/fs/cgroup/memory/memory.limit_in_bytes",
    ]
    .iter()
    .filter_map(|path| read_cgroup_limit_file(path))
    .min()
}

#[cfg(target_os = "linux")]
fn read_cgroup_limit_file(path: &str) -> Option<u64> {
    let raw = std::fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "max" {
        return None;
    }
    let limit = trimmed.parse::<u64>().ok()?;
    (limit < (1_u64 << 60)).then_some(limit)
}

#[cfg(target_os = "linux")]
fn parse_kib_line(value: &str) -> Option<u64> {
    let kib = value.split_whitespace().next()?.parse::<u64>().ok()?;
    kib.checked_mul(1024)
}
