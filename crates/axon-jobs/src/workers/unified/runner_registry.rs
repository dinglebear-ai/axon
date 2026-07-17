//! Dependency-inversion seam for the unified worker.
//!
//! `axon-jobs` must never depend on `axon-services` (`cargo xtask
//! check-layering`), but several unified `JobKind`s (memory compaction,
//! provider probes, research, …) have their real domain logic living in
//! `axon-services` because it composes multiple domain crates. Rather than
//! reaching upward, the unified worker claims/dispatches jobs through this
//! trait object, and the composition layer (`axon-services::context`) builds
//! the concrete implementations and hands back a populated registry.
//!
//! Job kinds with no registered runner keep falling back to the existing
//! `job_runner.unsupported_stage` terminal failure — registering a runner is
//! strictly additive and never required for the unified worker to make
//! progress on already-wired kinds.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::{ApiError, JobKind};
use tokio_util::sync::CancellationToken;

use crate::unified::SqliteUnifiedJobStore;

use super::UnifiedClaimedJob;

/// Executes the real domain work for one unified `JobKind`.
///
/// Implementations live in the composition layer (`axon-services`) so they
/// can call into whatever domain crates they need without axon-jobs taking a
/// dependency on axon-services. `run` is responsible only for doing the work
/// and reporting progress via `store` (heartbeats/events); the unified worker
/// loop in `unified.rs` owns marking the job's terminal status based on the
/// `Result` returned here.
#[async_trait]
pub trait UnifiedJobRunner: Send + Sync {
    async fn run(
        &self,
        claimed: &UnifiedClaimedJob,
        store: &SqliteUnifiedJobStore,
        shutdown: &CancellationToken,
    ) -> Result<(), ApiError>;
}

/// Lookup table from `JobKind` to its registered [`UnifiedJobRunner`].
///
/// Built once at composition time (`ServiceContext` construction) and shared
/// (`Arc`) across every unified worker poll. Kinds with no entry fall back to
/// `job_runner.unsupported_stage` in `unified.rs::run_unified_claimed`.
#[derive(Default)]
pub struct JobRunnerRegistry {
    runners: HashMap<JobKind, Arc<dyn UnifiedJobRunner>>,
}

impl JobRunnerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) the runner for `kind`.
    pub fn register(&mut self, kind: JobKind, runner: Arc<dyn UnifiedJobRunner>) -> &mut Self {
        self.runners.insert(kind, runner);
        self
    }

    /// Look up the runner for `kind`, if one is registered.
    pub fn get(&self, kind: JobKind) -> Option<Arc<dyn UnifiedJobRunner>> {
        self.runners.get(&kind).cloned()
    }

    /// True if `kind` has a registered runner.
    pub fn contains(&self, kind: JobKind) -> bool {
        self.runners.contains_key(&kind)
    }

    /// Every `JobKind` with a registered runner — i.e. every kind this worker
    /// process actually executes. Order is unspecified (backed by a `HashMap`);
    /// callers that need a stable order should sort. Used by the standalone
    /// worker loop to derive its idle-exit / stale-recovery watch set from the
    /// live registry instead of a hand-maintained second list (see
    /// `axon_rust-x4gxr.4`).
    pub fn registered_kinds(&self) -> Vec<JobKind> {
        self.runners.keys().copied().collect()
    }
}

#[cfg(test)]
#[path = "runner_registry_tests.rs"]
mod tests;
