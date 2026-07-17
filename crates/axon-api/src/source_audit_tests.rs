use super::audit::*;
use super::auth::AuthScope;
use super::ids::{JobId, SourceId};

#[test]
fn all_nine_contract_kinds_serialize_to_snake_case() {
    let cases: &[(SecurityAuditEventKind, &str)] = &[
        (SecurityAuditEventKind::AuthDenied, "\"auth_denied\""),
        (SecurityAuditEventKind::SsrfDenied, "\"ssrf_denied\""),
        (
            SecurityAuditEventKind::LocalPathDenied,
            "\"local_path_denied\"",
        ),
        (
            SecurityAuditEventKind::ToolExecutionDenied,
            "\"tool_execution_denied\"",
        ),
        (
            SecurityAuditEventKind::RedactionFailure,
            "\"redaction_failure\"",
        ),
        (
            SecurityAuditEventKind::SecretDetectedDropped,
            "\"secret_detected_dropped\"",
        ),
        (
            SecurityAuditEventKind::ArtifactTraversalAttempt,
            "\"artifact_traversal_attempt\"",
        ),
        (
            SecurityAuditEventKind::DestructivePruneAction,
            "\"destructive_prune_action\"",
        ),
        (
            SecurityAuditEventKind::CredentialDegraded,
            "\"credential_degraded\"",
        ),
    ];

    assert_eq!(
        cases.len(),
        9,
        "contract defines exactly nine audit event kinds"
    );

    for (kind, expected_json) in cases {
        let json = serde_json::to_string(kind).expect("kind serializes");
        assert_eq!(&json, expected_json);

        let round_tripped: SecurityAuditEventKind =
            serde_json::from_str(&json).expect("kind round-trips");
        assert_eq!(
            serde_json::to_string(&round_tripped).unwrap(),
            json,
            "round-trip must be stable"
        );
    }
}

#[test]
fn security_audit_event_carries_contract_required_fields() {
    let event = SecurityAuditEvent::new(SecurityAuditEventKind::SsrfDenied, "blocked host")
        .with_job_id(JobId::new(uuid::Uuid::nil()))
        .with_source_id(SourceId::from("src-1"))
        .with_policy("ssrf-default-deny", "2026-06-30")
        .with_ssrf_detail(SsrfAuditDetail {
            requested_url: "http://169.254.169.254/latest/meta-data".to_string(),
            canonical_url: "http://169.254.169.254/latest/meta-data".to_string(),
            resolved_ip_class: ResolvedIpClass::LinkLocal,
            redirect_chain_index: 0,
            policy_decision: SecurityPolicyDecision::Deny,
            headers_redacted: true,
        });

    assert_eq!(event.kind, SecurityAuditEventKind::SsrfDenied);
    assert_eq!(event.job_id, Some(JobId::new(uuid::Uuid::nil())));
    assert_eq!(event.source_id, Some(SourceId::from("src-1")));
    assert_eq!(event.policy_id.as_deref(), Some("ssrf-default-deny"));
    assert_eq!(event.policy_version.as_deref(), Some("2026-06-30"));
    assert_eq!(event.reason, "blocked host");

    let detail = event.ssrf.as_ref().expect("ssrf detail present");
    assert_eq!(detail.resolved_ip_class, ResolvedIpClass::LinkLocal);
    assert_eq!(detail.policy_decision, SecurityPolicyDecision::Deny);
    assert!(detail.headers_redacted);

    // Round-trip through JSON to ensure the whole event (not just the kind)
    // is a stable, deny-unknown-fields-safe wire contract.
    let json = serde_json::to_value(&event).expect("event serializes");
    let parsed: SecurityAuditEvent = serde_json::from_value(json).expect("event deserializes");
    assert_eq!(parsed.kind, SecurityAuditEventKind::SsrfDenied);
    assert_eq!(parsed.reason, "blocked host");
}

#[test]
fn security_audit_event_without_ssrf_detail_omits_field() {
    let event = SecurityAuditEvent::new(SecurityAuditEventKind::AuthDenied, "missing scope");
    let json = serde_json::to_value(&event).expect("event serializes");
    assert!(
        json.get("ssrf").is_none(),
        "ssrf detail must be omitted for non-SSRF event kinds"
    );
}

#[test]
fn authorization_local_and_tool_decisions_have_typed_audit_detail() {
    let cases = [
        (
            SecurityAuditEventKind::AuthDenied,
            SecurityDecisionBoundary::Authorization,
            AuthScope::Write,
        ),
        (
            SecurityAuditEventKind::LocalPathDenied,
            SecurityDecisionBoundary::LocalPath,
            AuthScope::Local,
        ),
        (
            SecurityAuditEventKind::ToolExecutionDenied,
            SecurityDecisionBoundary::CliToolExecution,
            AuthScope::Execute,
        ),
        (
            SecurityAuditEventKind::ToolExecutionDenied,
            SecurityDecisionBoundary::McpToolExecution,
            AuthScope::Execute,
        ),
    ];

    for (kind, boundary, scope) in cases {
        let event = SecurityAuditEvent::new(kind, "policy denied")
            .with_caller_id("caller-hash")
            .with_policy("default-deny", "1")
            .with_decision_detail(SecurityDecisionAuditDetail {
                boundary,
                policy_decision: SecurityPolicyDecision::Deny,
                required_scope: Some(scope),
                target: Some("redacted-target".to_string()),
            });
        let value = serde_json::to_value(&event).expect("serialize audit event");
        let round_trip: SecurityAuditEvent =
            serde_json::from_value(value).expect("deserialize audit event");

        assert_eq!(round_trip.decision.unwrap().boundary, boundary);
        assert_eq!(round_trip.caller_id.as_deref(), Some("caller-hash"));
    }
}
