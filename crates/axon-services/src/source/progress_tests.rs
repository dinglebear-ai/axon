use super::*;

/// This is the exact defect that made job `ecee11e8` (336/336 documents
/// discovered/prepared/embedded/published, zero item failures) surface only
/// as "web source indexing failed" with no way to tell that the real failure
/// was in generation-commit finalization: `pipeline_failed_error` used
/// `error.to_string()`, which for an `anyhow::Error` only ever prints the
/// outermost `.context()` frame. `{error:#}` prints the whole chain.
#[test]
fn pipeline_failed_error_preserves_full_anyhow_chain() {
    let root = anyhow::anyhow!("qdrant upsert timed out after 30s");
    let err = root.context("publishing web source generation failed");

    let api_error = pipeline_failed_error(&err);

    assert!(
        api_error
            .message
            .contains("qdrant upsert timed out after 30s"),
        "expected the root cause to survive in the message, got: {}",
        api_error.message
    );
    assert!(
        api_error
            .message
            .contains("publishing web source generation failed"),
        "expected the top-level context to remain present, got: {}",
        api_error.message
    );
}

#[test]
fn pipeline_failed_error_single_frame_is_unchanged() {
    let err = anyhow::anyhow!("web source indexing failed");

    let api_error = pipeline_failed_error(&err);

    assert_eq!(api_error.message, "web source indexing failed");
}
