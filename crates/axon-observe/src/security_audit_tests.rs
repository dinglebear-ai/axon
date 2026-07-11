use axon_api::source::{
    JobId, LifecycleStatus, ResolvedIpClass, SecurityAuditEvent, SecurityAuditEventKind,
    SecurityPolicyDecision, Severity, SourceId, SsrfAuditDetail, Visibility,
};

use crate::collector::{NoopObservabilitySink, ObservabilitySink};
use crate::security_audit::{emit_security_audit, to_progress_event};

fn denied_ssrf_event() -> SecurityAuditEvent {
    SecurityAuditEvent::new(
        SecurityAuditEventKind::SsrfDenied,
        "blocked private IP range",
    )
    .with_job_id(JobId(uuid::Uuid::new_v4()))
    .with_source_id(SourceId::from("src-crawl-1"))
    .with_policy("ssrf-default-deny", "2026-06-30")
    .with_ssrf_detail(SsrfAuditDetail {
        requested_url: "http://169.254.169.254/latest/meta-data".to_string(),
        canonical_url: "http://169.254.169.254/latest/meta-data".to_string(),
        resolved_ip_class: ResolvedIpClass::LinkLocal,
        redirect_chain_index: 0,
        policy_decision: SecurityPolicyDecision::Deny,
        headers_redacted: true,
    })
}

#[test]
fn ssrf_denied_projects_to_failed_internal_progress_event() {
    let audit = denied_ssrf_event();
    let progress = to_progress_event(&audit);

    assert_eq!(progress.event_id, audit.event_id);
    assert_eq!(progress.job_id, audit.job_id.unwrap());
    assert_eq!(progress.status, LifecycleStatus::Failed);
    assert_eq!(progress.severity, Severity::Warning);
    assert_eq!(progress.visibility, Visibility::Internal);
    assert_eq!(progress.source_id, audit.source_id);
    assert_eq!(
        progress.canonical_uri.as_deref(),
        Some("http://169.254.169.254/latest/meta-data")
    );
    assert!(progress.message.contains("ssrf_denied"));
    assert!(progress.message.contains("blocked private IP range"));

    let current = progress.current.expect("caller/policy summary present");
    let summary = current.message.expect("summary message present");
    assert!(summary.contains("policy=ssrf-default-deny@2026-06-30"));
}

#[test]
fn redaction_failure_projects_to_degraded_severity() {
    let audit = SecurityAuditEvent::new(
        SecurityAuditEventKind::RedactionFailure,
        "redactor errored on payload",
    );
    let progress = to_progress_event(&audit);

    assert_eq!(progress.status, LifecycleStatus::CompletedDegraded);
    assert_eq!(progress.severity, Severity::Degraded);
    // No job id was set on the source event; the sentinel default must not
    // silently drop the event or panic.
    assert_eq!(progress.job_id, JobId::default());
}

#[tokio::test]
async fn emit_security_audit_forwards_to_sink() {
    let sink = NoopObservabilitySink;
    let audit = denied_ssrf_event();

    emit_security_audit(&sink, &audit)
        .await
        .expect("noop sink accepts the projected event");

    // Exercise the trait-object call path too (the shape callers will use in
    // production — `Arc<dyn ObservabilitySink>` — is object-safe here).
    let boxed: &dyn ObservabilitySink = &sink;
    emit_security_audit(boxed, &audit)
        .await
        .expect("emits through a dyn ObservabilitySink");
}
