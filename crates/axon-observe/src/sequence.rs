//! Monotonic per-stream sequence assignment.
//!
//! The event-schema contract requires that `SourceProgressEvent.sequence` and
//! `JobHeartbeat.last_event_sequence` strictly increase within a single stream
//! (keyed by `job_id`). The pure event builders in [`crate::event`] cannot own
//! this state, so the emitting sink stamps sequences at serialization time via a
//! [`SequenceRegistry`]. This keeps the single ordering point at the boundary
//! where events actually become durable/observable.

pub const MODULE_NAME: &str = "sequence";

use std::collections::HashMap;
use std::sync::Mutex;

use axon_api::source::JobId;

/// Assigns strictly-increasing sequence numbers per `job_id`.
///
/// The first sequence handed out for any stream is `1`; each subsequent call for
/// the same job returns the prior value plus one. Distinct jobs have independent
/// counters. Cloning is intentionally not derived — share via `Arc` so all
/// emitters on one sink observe the same counter.
#[derive(Debug, Default)]
pub struct SequenceRegistry {
    counters: Mutex<HashMap<JobId, u64>>,
}

impl SequenceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the next monotonic sequence for `job_id`, advancing the counter.
    pub fn next(&self, job_id: JobId) -> u64 {
        let mut counters = self.counters.lock().expect("sequence registry poisoned");
        let slot = counters.entry(job_id).or_insert(0);
        *slot += 1;
        *slot
    }

    /// Peek the most recently issued sequence for `job_id` without advancing it.
    ///
    /// Returns `None` when no sequence has been issued for the stream yet.
    pub fn last(&self, job_id: JobId) -> Option<u64> {
        let counters = self.counters.lock().expect("sequence registry poisoned");
        counters.get(&job_id).copied()
    }

    /// Number of distinct streams that have been issued a sequence.
    pub fn stream_count(&self) -> usize {
        let counters = self.counters.lock().expect("sequence registry poisoned");
        counters.len()
    }
}

#[cfg(test)]
#[path = "sequence_tests.rs"]
mod tests;
