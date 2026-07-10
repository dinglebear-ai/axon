//! Test fixtures for the memory crate: a mutable, deterministic clock and a
//! fake [`CompactionSynthesizer`] for the `semantic_summary` strategy.

use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};

use async_trait::async_trait;
use axon_api::source::MemoryRecord;

use crate::record::{Clock, format_rfc3339};
use crate::sqlite::compact::CompactionSynthesizer;

/// A clock whose "now" can be pinned and advanced, for deterministic decay
/// tests. Epoch seconds are the single source of truth; the RFC3339 form is
/// derived from them.
#[derive(Debug)]
pub struct FixedClock {
    epoch_secs: AtomicI64,
    // Guards multi-step advances so concurrent test tasks see a consistent view.
    _lock: Mutex<()>,
}

impl FixedClock {
    /// Create a clock pinned at `epoch_secs`.
    pub fn new(epoch_secs: i64) -> Self {
        Self {
            epoch_secs: AtomicI64::new(epoch_secs),
            _lock: Mutex::new(()),
        }
    }

    /// Create a clock pinned at 2026-01-01T00:00:00Z.
    pub fn at_2026() -> Self {
        // 2026-01-01T00:00:00Z = 1767225600 epoch seconds.
        Self::new(1_767_225_600)
    }

    /// Advance the clock by `days` (may be fractional-day precision in seconds).
    pub fn advance_days(&self, days: i64) {
        self.epoch_secs.fetch_add(days * 86_400, Ordering::SeqCst);
    }

    /// Advance the clock by `secs` seconds.
    pub fn advance_secs(&self, secs: i64) {
        self.epoch_secs.fetch_add(secs, Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now_epoch_secs(&self) -> i64 {
        self.epoch_secs.load(Ordering::SeqCst)
    }

    fn now_rfc3339(&self) -> String {
        format_rfc3339(self.now_epoch_secs())
    }
}

/// Deterministic fake [`CompactionSynthesizer`] for tests exercising the
/// `semantic_summary` compaction strategy without a real LLM backend. Joins
/// source bodies with a `[synthesized]` marker so tests can assert the LLM
/// path (not `concatenate`'s `[memory_id] body` format) actually ran.
#[derive(Debug, Default)]
pub struct FakeCompactionSynthesizer {
    /// When set, `synthesize` returns this error instead of a summary
    /// (exercises the fail-closed path).
    pub fail_with: Option<String>,
}

impl FakeCompactionSynthesizer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn failing(message: impl Into<String>) -> Self {
        Self {
            fail_with: Some(message.into()),
        }
    }
}

#[async_trait]
impl CompactionSynthesizer for FakeCompactionSynthesizer {
    async fn synthesize(
        &self,
        sources: &[MemoryRecord],
        instructions: Option<&str>,
    ) -> Result<String, String> {
        if let Some(message) = &self.fail_with {
            return Err(message.clone());
        }
        let joined = sources
            .iter()
            .map(|record| record.body.as_str())
            .collect::<Vec<_>>()
            .join(" | ");
        Ok(match instructions {
            Some(instructions) => format!("[synthesized:{instructions}] {joined}"),
            None => format!("[synthesized] {joined}"),
        })
    }
}
