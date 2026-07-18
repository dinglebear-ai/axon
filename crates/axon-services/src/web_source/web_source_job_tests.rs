use super::*;

/// `terminal_source_error` is the first place a web-source pipeline failure
/// gets classified into a `SourceError`. Before this fix it collapsed the
/// `anyhow::Error` chain with `.to_string()` (top context frame only) for
/// both `message` and `cause`, discarding whatever the real root cause was —
/// e.g. a job that fully embedded/upserted 336/336 documents but failed at
/// generation-commit would surface only "web source indexing failed" with no
/// way to tell why. `cause` must carry the full chain so operators can
/// actually diagnose a failure from `axon jobs get`/`axon status` output.
#[test]
fn terminal_source_error_cause_preserves_full_chain() {
    let root = anyhow::anyhow!("qdrant upsert timed out after 30s");
    let err = root.context("publishing web source generation failed");

    let source_error = terminal_source_error(&err);

    let cause = source_error
        .cause
        .as_deref()
        .expect("multi-frame error must produce a cause");
    assert!(
        cause.contains("qdrant upsert timed out after 30s"),
        "expected the root cause to survive in `cause`, got: {cause}"
    );
    assert!(
        source_error
            .message
            .contains("publishing web source generation failed"),
        "expected the top-level context to remain the primary message, got: {}",
        source_error.message
    );
}

/// A single-frame error (no underlying cause) should not produce a
/// pointlessly duplicated `cause` — `None` signals "nothing more specific
/// than the message" instead of restating it.
#[test]
fn terminal_source_error_cause_is_none_for_single_frame_error() {
    let err = anyhow::anyhow!("web source indexing failed");

    let source_error = terminal_source_error(&err);

    assert_eq!(source_error.cause, None);
}

/// Redaction gate: `SourceError` is persisted straight into
/// `jobs.last_error_json`, a column with no automatic redaction pass (unlike
/// `job_events`/`details_json`) — widening `cause` to the full chain must not
/// widen the leak surface, so secret-shaped text has to be scrubbed before
/// either field is populated.
#[test]
fn terminal_source_error_redacts_secrets_in_message_and_cause() {
    // Repeated-character token body (matching the structured-rule shape used
    // by axon-core's own redact_tests.rs), not a plausible real key, and not
    // framed as an HTTP `Authorization:` header — avoids tripping third-party
    // secret scanners (e.g. GitGuardian's generic Bearer-token detector) on
    // this synthetic fixture while still exercising the same structured
    // `sk-`-prefix redaction rule.
    let token = format!("sk-{}", "a".repeat(20));
    let root = anyhow::anyhow!(format!("upstream rejected credentials: {token}"));
    let err = root.context("web source fetch failed");

    let source_error = terminal_source_error(&err);

    assert!(
        !source_error.message.contains(&token),
        "expected the secret token to be redacted from message, got: {}",
        source_error.message
    );
    let cause = source_error.cause.as_deref().unwrap_or_default();
    assert!(
        !cause.contains(&token),
        "expected the secret token to be redacted from cause, got: {cause}"
    );
}
