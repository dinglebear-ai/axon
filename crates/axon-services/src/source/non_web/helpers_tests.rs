use super::*;

fn manifest_with_config(config: &str) -> SourceManifest {
    let mut metadata = MetadataMap::new();
    metadata.insert(
        super::super::PUBLICATION_CONFIG_KEY.to_string(),
        serde_json::json!(config),
    );
    SourceManifest {
        source_id: SourceId::new("src-config-test"),
        generation: SourceGenerationId::new("gen-config-test"),
        adapter: AdapterRef {
            name: "feed".to_string(),
            version: "test".to_string(),
        },
        scope: SourceScope::Api,
        items: Vec::new(),
        created_at: Timestamp("2026-07-16T00:00:00Z".to_string()),
        metadata,
    }
}

#[test]
fn unchanged_fast_path_requires_the_same_publication_configuration() {
    let manifest = manifest_with_config("cfg-original");

    assert!(publication_config_matches(
        &manifest,
        &ConfigSnapshotId::new("cfg-original")
    ));
    assert!(!publication_config_matches(
        &manifest,
        &ConfigSnapshotId::new("cfg-changed")
    ));
}

#[test]
fn legacy_manifest_without_publication_configuration_is_not_reused() {
    let mut manifest = manifest_with_config("cfg-original");
    manifest.metadata.clear();

    assert!(!publication_config_matches(
        &manifest,
        &ConfigSnapshotId::new("cfg-original")
    ));
}

/// Companion to the web-source fix: `record_terminal_status` previously built
/// its `SourceError` with `message: error.to_string()` (anyhow's top-context
/// frame only, discarding the chain) and a hardcoded `cause: None` — so any
/// non-web source (git/local/feed/registry/reddit/youtube) failing deep in
/// the pipeline surfaced just as unhelpfully as the web path did before this
/// fix. `terminal_source_error` must carry the full chain in `cause`.
#[test]
fn terminal_source_error_cause_preserves_full_chain() {
    let root = anyhow::anyhow!("qdrant upsert timed out after 30s");
    let err = root.context("publishing non-web source generation failed");

    let source_error = terminal_source_error(&err);

    let cause = source_error
        .cause
        .as_deref()
        .expect("multi-frame error must produce a cause");
    assert!(
        cause.contains("qdrant upsert timed out after 30s"),
        "expected the root cause to survive in `cause`, got: {cause}"
    );
}

#[test]
fn terminal_source_error_cause_is_none_for_single_frame_error() {
    let err = anyhow::anyhow!("non-web source indexing failed");

    let source_error = terminal_source_error(&err);

    assert_eq!(source_error.cause, None);
}

/// Redaction gate: this `SourceError` is persisted straight into
/// `jobs.last_error_json` (no automatic redaction pass on that column, unlike
/// the `job_events`/`details_json` path — see the review that found this),
/// so `terminal_source_error` itself must scrub secret-shaped text before it
/// ever reaches `message`/`cause`.
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
    let err = root.context("registry source fetch failed");

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
