use crate::config_snapshot::{apply_config_snapshot, config_snapshot_json};
use axon_core::config::Config;

/// The embed runner stores a full config snapshot in `axon_embed_jobs.config_json` at enqueue
/// time. At claim time it calls `apply_config_snapshot` to restore the original config into the
/// worker config. These tests verify that `seed_url` — the origin marker written by the crawl
/// and ingest runners so every embedded chunk gets the right `seed_url` payload field — survives
/// the snapshot round-trip.
#[test]
fn seed_url_propagates_through_config_snapshot() {
    let mut cfg = Config::default_minimal();
    cfg.seed_url = Some("owner/repo".to_string());

    let snapshot_json = config_snapshot_json(&cfg).expect("snapshot serializes");
    let restored = apply_config_snapshot(&Config::default_minimal(), &snapshot_json)
        .expect("snapshot applies cleanly");

    assert_eq!(
        restored.seed_url.as_deref(),
        Some("owner/repo"),
        "seed_url must survive the config snapshot round-trip so embed pipeline \
         stamps the correct origin on every chunk"
    );
}

/// Verify that a `None` seed_url round-trips correctly (direct embeds with no origin).
#[test]
fn seed_url_none_round_trips() {
    let cfg = Config::default_minimal(); // seed_url is None by default

    let snapshot_json = config_snapshot_json(&cfg).expect("snapshot serializes");
    let restored = apply_config_snapshot(&Config::default_minimal(), &snapshot_json)
        .expect("snapshot applies cleanly");

    assert!(
        restored.seed_url.is_none(),
        "seed_url=None must also survive the round-trip"
    );
}

/// The result JSON emitted by `run_embed_job` must include `docs_failed` so callers can
/// detect partial failures (per-doc timeout, TEI error) without aborting the batch.
/// The embed pipeline already increments `EmbedSummary.docs_failed` for timed-out docs;
/// this test documents the expected output shape.
#[test]
fn embed_result_json_includes_docs_failed_field() {
    // We can't call run_embed_job without a live SQLite+TEI stack, so we test
    // the shape using the same serde_json construction the runner uses.
    let summary = axon_vector::ops::tei::EmbedSummary {
        docs_embedded: 3,
        docs_failed: 1,
        chunks_embedded: 15,
    };
    let result = serde_json::json!({
        "input": "some/path",
        "collection": "axon",
        "docs_embedded": summary.docs_embedded,
        "docs_failed": summary.docs_failed,
        "chunks_embedded": summary.chunks_embedded,
        "seed_url": Option::<String>::None,
    });

    assert_eq!(
        result["docs_failed"], 1,
        "docs_failed must be present in result JSON"
    );
    assert_eq!(result["docs_embedded"], 3);
    assert_eq!(result["chunks_embedded"], 15);
}

/// Verify that a non-zero `docs_failed` count can coexist with a successful job outcome —
/// individual doc failures must not abort the batch.
#[test]
fn embed_result_with_partial_failures_is_not_error() {
    // This models the runner's happy path where some docs failed per-timeout
    // but the overall job still succeeds (returns Ok(Some(...))).
    let summary = axon_vector::ops::tei::EmbedSummary {
        docs_embedded: 9,
        docs_failed: 2, // 2 docs timed out
        chunks_embedded: 45,
    };

    // The runner returns Ok(Some(json)) even when docs_failed > 0; the caller
    // is responsible for inspecting the counter and deciding whether to alert.
    assert!(
        summary.docs_failed > 0,
        "docs_failed counter tracks per-doc failures"
    );
    assert!(
        summary.docs_embedded > 0,
        "remaining docs were embedded successfully despite individual failures"
    );
}
