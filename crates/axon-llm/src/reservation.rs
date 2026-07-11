//! Process-wide LLM completion reservation pool.
//!
//! Per the provider contract
//! (`docs/pipeline-unification/runtime/provider-contract.md`, "LlmProvider
//! Contract"): "LLM calls use `llm` reservations and cannot consume embedding
//! capacity." This module gives LLM completion its own
//! [`ProviderReservationManager`] — a SEPARATE pool from the embedding/vector
//! managers in `axon-embedding::reservation` — so bulk LLM work (synthesis,
//! research, extraction) cannot starve interactive `ask`.
//!
//! [`crate::runtime::complete_text`]/[`crate::runtime::complete_streaming`]
//! reserve one unit from the singleton pool before dispatching to a backend,
//! and record success/failure on completion — this is the single choke point
//! every LLM caller passes through (ask synthesis, the `axon-core` extract LLM
//! fallback via [`crate::runtime::completer::BackendTextCompleter`], research,
//! summarize, evaluate, suggest, debug), so wiring it here covers all of them
//! without threading a reservation handle through every call site.
//!
//! Callers that need the `interactive` priority lane wrap their call with
//! [`with_priority`] — currently only the `ask` synthesis path
//! (`axon-vector::ops::commands::streaming::ask_llm_*`) does this. Every other
//! caller defaults to `JobPriority::Background`, which the shared
//! [`ProviderReservationManager`] refuses once granting it would eat into the
//! configured interactive reserve — see `preserves_interactive_capacity` in
//! `axon_observe::reservation`.

use std::sync::LazyLock;

use axon_api::source::{
    HealthStatus, JobPriority, ProviderCoolingSnapshot, ProviderId, ProviderKind, Timestamp,
};

pub use axon_observe::reservation::{
    ProviderReservation, ProviderReservationConfig, ProviderReservationContext,
    ProviderReservationManager, ProviderReservationOutcome,
};

pub type Result<T> = axon_observe::reservation::Result<T>;

/// Total abstract capacity units in the process-wide LLM pool. This is a
/// fairness/priority/cooldown pool, not a hard concurrency limiter — the
/// per-backend `completion_concurrency` semaphores in
/// [`crate::runtime::concurrency`] already bound real in-flight requests.
const LLM_RESERVATION_CAPACITY: u32 = 64;
/// Units always left free for `interactive`-priority reservations (`ask`).
const LLM_INTERACTIVE_RESERVE: u32 = 4;
const LLM_COOLDOWN_AFTER_FAILURES: u32 = 3;
const LLM_COOLDOWN_SECS: u64 = 30;

static LLM_RESERVATIONS: LazyLock<ProviderReservationManager> = LazyLock::new(|| {
    ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("llm-provider-pool"),
        provider_kind: ProviderKind::Llm,
        capacity: LLM_RESERVATION_CAPACITY,
        interactive_reserve: LLM_INTERACTIVE_RESERVE,
        cooldown_after_failures: LLM_COOLDOWN_AFTER_FAILURES,
        cooldown_secs: LLM_COOLDOWN_SECS,
    })
});

tokio::task_local! {
    static PRIORITY: JobPriority;
}

/// Run `fut` with `priority` applied to any LLM reservation acquired within it
/// via [`reserve`]. Not nestable in a way that layers priorities — the
/// innermost `with_priority` scope wins for reservations acquired inside it.
pub async fn with_priority<F: std::future::Future>(priority: JobPriority, fut: F) -> F::Output {
    PRIORITY.scope(priority, fut).await
}

/// The priority a [`reserve`] call would use right now: whatever
/// [`with_priority`] scope the caller is running inside, or
/// `JobPriority::Background` by default.
fn current_priority() -> JobPriority {
    PRIORITY
        .try_with(|priority| *priority)
        .unwrap_or(JobPriority::Background)
}

/// Reserve one unit of LLM completion capacity at [`current_priority`].
pub async fn reserve() -> Result<ProviderReservation> {
    LLM_RESERVATIONS.reserve(current_priority(), 1).await
}

/// Record a successful completion, clearing any cooldown/failure streak.
pub async fn record_success() {
    LLM_RESERVATIONS.record_success().await;
}

/// Record a failed completion. Returns [`ProviderReservationOutcome::Cooling`]
/// once `code`'s failures push the pool into cooldown.
pub async fn record_failure(
    code: impl Into<String>,
    retryable: bool,
) -> ProviderReservationOutcome {
    LLM_RESERVATIONS.record_failure(code, retryable).await
}

/// Current cooling snapshot, if the pool is presently cooling down.
pub async fn cooling_snapshot() -> Option<ProviderCoolingSnapshot> {
    LLM_RESERVATIONS.cooling_snapshot().await
}

/// Current pool health.
pub async fn health() -> HealthStatus {
    LLM_RESERVATIONS.health().await
}

/// Current cooldown deadline, if any.
pub async fn cooldown_until() -> Option<Timestamp> {
    LLM_RESERVATIONS.cooldown_until().await
}

/// Test-only handle to the process-wide singleton, so tests can assert on its
/// state without a separate manager instance (the pool being singleton-shaped
/// is exactly the behavior under test — every LLM call funnels through it).
#[cfg(test)]
pub(crate) fn manager_for_tests() -> &'static ProviderReservationManager {
    &LLM_RESERVATIONS
}

#[cfg(test)]
#[path = "reservation_tests.rs"]
mod tests;
