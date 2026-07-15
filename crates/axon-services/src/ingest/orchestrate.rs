//! Source-ingestion orchestration: drives session-export ingest and reports
//! progress over an optional `ServiceEvent` channel, returning an `IngestResult`.
//!
//! These functions take only `cfg` + progress channels (no `ServiceContext`,
//! no jobs), so they live here under `axon-services::ingest` and are called by
//! both the services layer and the jobs ingest runner.
//!
//! Phase 12 clean break (issue #298): the github/gitlab/gitea/generic_git/
//! reddit/youtube/rss provider orchestration that used to live here was
//! deleted outright — only session-export ingest is still executed by the
//! legacy per-family job runner. `classify_target`'s IngestSource variants
//! for those providers remain (backed by `crate::ingest::target_parse`) since
//! `axon refresh` still needs to classify previously-ingested origins.
//!
//! Migrated from the (now-deleted) `axon-ingest` crate as part of issue #298's
//! pipeline-unification cleanup.

use axon_api::job_dto::IngestResult;
pub fn map_ingest_result(payload: serde_json::Value) -> IngestResult {
    IngestResult { payload }
}

pub fn ingest_payload(
    source: &str,
    target_field: Option<(&str, &str)>,
    chunks_embedded: usize,
) -> serde_json::Value {
    let mut payload = serde_json::json!({
        "source": source,
        "chunks_embedded": chunks_embedded,
        "chunks": chunks_embedded,
    });
    if let Some((key, value)) = target_field
        && let Some(object) = payload.as_object_mut()
    {
        object.insert(
            key.to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }
    payload
}
