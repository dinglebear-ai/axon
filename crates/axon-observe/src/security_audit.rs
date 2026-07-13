//! Emission helper for [`SecurityAuditEvent`](axon_api::source::SecurityAuditEvent).
//!
//! Security-relevant events (SSRF denials, local path denials, tool execution
//! denials, redaction failures, ...) are transport-neutral DTOs owned by
//! `axon-api::source::audit`. This module folds them into the existing
//! [`SourceProgressEvent`] shape so they can flow through the same
//! [`ObservabilitySink`] used by every other pipeline stage — no new sink
//! trait, no new wire format.
//!
//! Deliberately does not touch `crate::event`, `crate::log`, `crate::span`,
//! `crate::progress`, or `crate::phase` — those own the general progress-event
//! builder surface. This module owns only the security-audit → progress-event
//! projection.

pub const MODULE_NAME: &str = "security_audit";

use axon_api::source::{
    LifecycleStatus, PipelinePhase, ProgressCurrent, SecurityAuditEvent, SecurityAuditEventKind,
    Severity, SourceProgressEvent, StageCounts, Visibility,
};

use crate::collector::{ObservabilitySink, Result};

/// Project a [`SecurityAuditEvent`] onto the shared [`SourceProgressEvent`]
/// wire shape. `sequence` is always `0` here — the sink stamps the real
/// monotonic per-`job_id` sequence at emit time, matching every other
/// pure event builder in this crate (see `crate::event::base_event`).
pub fn to_progress_event(event: &SecurityAuditEvent) -> SourceProgressEvent {
    let job_id = event.job_id.unwrap_or_default();
    let (severity, status) = severity_and_status(event.kind);

    SourceProgressEvent {
        event_id: event.event_id.clone(),
        sequence: 0,
        job_id,
        attempt: 1,
        stage_id: None,
        batch_id: None,
        reservation_id: None,
        checkpoint_id: None,
        dedupe_key: None,
        phase: PipelinePhase::Authorizing,
        status,
        severity,
        // Audit events record policy decisions and redacted reasons, not
        // caller-facing content, but they are not safe to surface to public
        // read-only callers by default — keep them Internal.
        visibility: Visibility::Internal,
        message: format!("{}: {}", kind_label(event.kind), event.reason),
        timestamp: event.timestamp.clone(),
        source_id: event.source_id.clone(),
        canonical_uri: event
            .ssrf
            .as_ref()
            .map(|detail| detail.canonical_url.clone()),
        adapter: None,
        scope: None,
        generation: None,
        counts: zero_counts(),
        timing: None,
        current: current_from(event),
        throughput: None,
        retry: None,
        warning: None,
        error: None,
    }
}

/// Emit a [`SecurityAuditEvent`] through an [`ObservabilitySink`], reusing the
/// sink's existing `emit()` path (sequencing, persistence, tracing forwarding
/// — whatever the concrete sink does for `SourceProgressEvent`).
pub async fn emit_security_audit(
    sink: &dyn ObservabilitySink,
    event: &SecurityAuditEvent,
) -> Result<()> {
    sink.emit(to_progress_event(event)).await
}

fn severity_and_status(kind: SecurityAuditEventKind) -> (Severity, LifecycleStatus) {
    match kind {
        SecurityAuditEventKind::RedactionFailure | SecurityAuditEventKind::CredentialDegraded => {
            (Severity::Degraded, LifecycleStatus::CompletedDegraded)
        }
        _ => (Severity::Warning, LifecycleStatus::Failed),
    }
}

fn kind_label(kind: SecurityAuditEventKind) -> &'static str {
    match kind {
        SecurityAuditEventKind::AuthDenied => "auth_denied",
        SecurityAuditEventKind::SsrfDenied => "ssrf_denied",
        SecurityAuditEventKind::LocalPathDenied => "local_path_denied",
        SecurityAuditEventKind::ToolExecutionDenied => "tool_execution_denied",
        SecurityAuditEventKind::RedactionFailure => "redaction_failure",
        SecurityAuditEventKind::SecretDetectedDropped => "secret_detected_dropped",
        SecurityAuditEventKind::ArtifactTraversalAttempt => "artifact_traversal_attempt",
        SecurityAuditEventKind::DestructivePruneAction => "destructive_prune_action",
        SecurityAuditEventKind::CredentialDegraded => "credential_degraded",
    }
}

fn current_from(event: &SecurityAuditEvent) -> Option<ProgressCurrent> {
    if event.caller_id.is_none() && event.policy_id.is_none() {
        return None;
    }
    Some(ProgressCurrent {
        source_item_key: None,
        document_id: None,
        chunk_id: None,
        adapter: None,
        provider: None,
        message: Some(caller_and_policy_summary(event)),
    })
}

fn caller_and_policy_summary(event: &SecurityAuditEvent) -> String {
    let caller = event.caller_id.as_deref().unwrap_or("unknown");
    let policy = event.policy_id.as_deref().unwrap_or("unknown");
    let version = event.policy_version.as_deref().unwrap_or("unknown");
    format!("caller={caller} policy={policy}@{version}")
}

fn zero_counts() -> StageCounts {
    StageCounts {
        items_total: None,
        items_done: 0,
        documents_total: None,
        documents_done: 0,
        chunks_total: None,
        chunks_done: 0,
        bytes_total: None,
        bytes_done: 0,
    }
}
