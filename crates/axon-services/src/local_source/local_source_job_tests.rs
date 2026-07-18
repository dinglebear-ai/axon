use super::*;

/// Companion to the web-source fix: `terminal_source_error` set both
/// `message` and `cause` to the same top-frame-only `err.to_string()`,
/// discarding whatever the chain carried underneath — the same defect fixed
/// in `web_source/web_source_job.rs`. `cause` must carry the full chain
/// (still redacted the same way `message` already is).
#[test]
fn terminal_source_error_cause_preserves_full_chain() {
    let root = Path::new("/tmp/axon-local-source-test-root");
    let inner = anyhow::anyhow!("qdrant upsert timed out after 30s");
    let err = inner.context("publishing local source generation failed");

    let source_error = terminal_source_error(&err, root);

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
            .contains("publishing local source generation failed"),
        "expected the top-level context to remain the primary message, got: {}",
        source_error.message
    );
}

#[test]
fn terminal_source_error_cause_is_none_for_single_frame_error() {
    let root = Path::new("/tmp/axon-local-source-test-root");
    let err = anyhow::anyhow!("local source indexing failed");

    let source_error = terminal_source_error(&err, root);

    assert_eq!(source_error.cause, None);
}

/// `cause` must get the same local-root redaction `message` already gets —
/// widening to the full chain must not leak the local filesystem root.
#[test]
fn terminal_source_error_cause_redacts_local_root() {
    let root = Path::new("/tmp/axon-local-source-test-root");
    let inner = anyhow::anyhow!(format!(
        "failed to read {}/secret-notes.txt",
        root.display()
    ));
    let err = inner.context("publishing local source generation failed");

    let source_error = terminal_source_error(&err, root);

    let cause = source_error.cause.as_deref().unwrap_or_default();
    assert!(
        !cause.contains("/tmp/axon-local-source-test-root"),
        "expected the local root to be redacted from cause, got: {cause}"
    );
    assert!(cause.contains("<local-source-root>"));
}
