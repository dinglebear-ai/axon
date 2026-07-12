//! Memory lifecycle observability — emits `axon-observe` progress events for
//! durable memory operations per the contract's "Observability" section.
//!
//! Contract required fields: `memory_id`, `memory_type`, `memory_status`,
//! `memory_scope_kind`, `job_id`/`request_id`, `phase`, `severity`,
//! `visibility`, `score_before`/`score_after` (scoring changes),
//! `review_reason` (when applicable).
//!
//! Memory-specific phases (`remembering`, `embedding`, `linking`,
//! `reviewing`, `reinforcing`, `compacting`, `forgetting`) are not all
//! present in the canonical `PipelinePhase` enum — per the contract, an
//! exposed-but-uncataloged phase may be "represented as operation detail
//! under an existing phase" instead of widening the enum (which is matched
//! exhaustively by ~7 unrelated source-family modules outside this crate's
//! territory). This module maps each memory phase to the closest existing
//! `PipelinePhase` and carries the precise memory phase name in the emitted
//! event's `message` field alongside the other required fields.

use std::sync::Arc;

use axon_api::source::{
    JobId, MemoryRecord, PipelinePhase, Severity, SourceProgressEvent, StageCounts, Visibility,
};
use axon_observe::collector::ObservabilitySink;
use axon_observe::event;

/// Memory-specific lifecycle phase (contract "Observability" section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPhase {
    Remembering,
    Embedding,
    Linking,
    Reviewing,
    Reinforcing,
    Compacting,
    Forgetting,
}

impl MemoryPhase {
    /// Stable wire name (matches the contract's phase list verbatim).
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryPhase::Remembering => "remembering",
            MemoryPhase::Embedding => "embedding",
            MemoryPhase::Linking => "linking",
            MemoryPhase::Reviewing => "reviewing",
            MemoryPhase::Reinforcing => "reinforcing",
            MemoryPhase::Compacting => "compacting",
            MemoryPhase::Forgetting => "forgetting",
        }
    }

    /// Closest canonical `PipelinePhase` this memory phase is represented
    /// under (contract: "represented as operation detail under an existing
    /// phase").
    fn canonical(self) -> PipelinePhase {
        match self {
            MemoryPhase::Remembering => PipelinePhase::Preparing,
            MemoryPhase::Embedding => PipelinePhase::Embedding,
            MemoryPhase::Linking => PipelinePhase::Graphing,
            MemoryPhase::Reviewing => PipelinePhase::Evaluating,
            MemoryPhase::Reinforcing => PipelinePhase::Evaluating,
            MemoryPhase::Compacting => PipelinePhase::Preparing,
            MemoryPhase::Forgetting => PipelinePhase::Cleaning,
        }
    }
}

/// Build and emit a memory lifecycle progress event. Fire-and-forget: a
/// dropped/failed observability emit never blocks or fails the memory
/// operation it describes (observability is best-effort, not a durability
/// boundary — the durable write already committed to SQLite by the time
/// this is called).
///
/// `job_id` correlates every event emitted by one logical store-method
/// invocation (contract: "`job_id` or `request_id`"). Callers must generate
/// **one** id per invocation and pass the same value to every `emit()` call
/// made while servicing it — `contradict()` and `import()` each emit more
/// than once per call (once per affected memory), and a fresh random id per
/// `emit()` call (the previous behavior here) silently defeated the
/// observability sink's `(job_id, sequence)` correlation: every event looked
/// like the start of an unrelated single-event job, so `events_for`/
/// `heartbeat_for` lookups keyed by `job_id` could never find sibling events
/// from the same operation.
///
/// No `axon-api::source` memory request DTO carries a caller-supplied
/// request/job/session id today (`remember`/`search`/`context`/`link`/
/// `update`/`reinforce`/`supersede`/`contradict`/`pin`/`archive`/`forget`/
/// `review`/`compact` — none of the 14 contract DTOs have one), so there is
/// no real external id to thread through yet; adding one is a DTO-shape
/// change across the whole memory family (plus every CLI/MCP/REST caller),
/// out of scope here. Callers therefore still synthesize a fresh
/// [`uuid::Uuid::new_v4`]-backed id per invocation — genuinely synthetic,
/// but now scoped to "one per operation" rather than "one per event".
#[allow(clippy::too_many_arguments)]
pub(crate) async fn emit(
    sink: &Arc<dyn ObservabilitySink>,
    job_id: JobId,
    phase: MemoryPhase,
    record: &MemoryRecord,
    severity: Severity,
    score_before: Option<f32>,
    score_after: Option<f32>,
    review_reason: Option<&str>,
) {
    let message = lifecycle_message(phase, record, score_before, score_after, review_reason);
    let mut evt: SourceProgressEvent = event::stage_completed(
        job_id,
        None,
        phase.canonical(),
        StageCounts {
            items_total: None,
            items_done: 1,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        message,
    );
    evt.severity = severity;
    evt.visibility = record.visibility;
    evt.canonical_uri = Some(format!("memory://{}", record.memory_id.0));
    let _ = sink.emit(evt).await;
}

fn lifecycle_message(
    phase: MemoryPhase,
    record: &MemoryRecord,
    score_before: Option<f32>,
    score_after: Option<f32>,
    review_reason: Option<&str>,
) -> String {
    let mut parts = vec![
        format!("phase={}", phase.as_str()),
        format!("memory_id={}", record.memory_id.0),
        format!("memory_type={:?}", record.memory_type).to_lowercase(),
        format!("memory_status={:?}", record.status).to_lowercase(),
        format!("memory_scope_kind={}", record.scope.kind),
        format!("visibility={}", visibility_str(record.visibility)),
    ];
    if let Some(before) = score_before {
        parts.push(format!("score_before={before:.4}"));
    }
    if let Some(after) = score_after {
        parts.push(format!("score_after={after:.4}"));
    }
    if let Some(reason) = review_reason {
        parts.push(format!("review_reason={reason}"));
    }
    parts.join(" ")
}

fn visibility_str(visibility: Visibility) -> &'static str {
    match visibility {
        Visibility::Public => "public",
        Visibility::Internal => "internal",
        Visibility::Sensitive => "sensitive",
        Visibility::Redacted => "redacted",
        Visibility::Derived => "derived",
    }
}

#[cfg(test)]
#[path = "observe_tests.rs"]
mod tests;
