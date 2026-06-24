use crate::ops::qdrant::qdrant_base;
use axon_core::config::Config;
use axon_core::http::internal_service_http_client;
use futures_util::stream::{self, StreamExt};
use std::error::Error;
use std::future::Future;
use std::time::Duration;

type IndexFut<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'a>>;

/// Keyword fields indexed for filtering and `/facet` aggregations.
const KEYWORD_INDEX_FIELDS: &[&str] = &[
    "url",
    "domain",
    "source_type",
    // Crawl/ingest origin marker — faceted by `axon refresh` to re-enqueue origins.
    "seed_url",
    "extractor_name",
    // Shared git provider schema (all git-backed ingest sources).
    "provider",
    "git_host",
    "git_owner",
    "git_repo",
    "git_content_kind",
    "git_default_branch",
    "git_repo_language",
    "git_repo_topics",
    "git_state",
    "git_author",
    "git_file_language",
    "git_file_path",
    "code_file_path",
    "code_path_prefixes",
    "code_language",
    "code_file_type",
    "chunk_content_kind",
    "code_chunking_method",
    "symbol_kind",
    // Vertical extractor fields.
    "pkg_registry",
    "pkg_name",
    "pkg_language",
    "pkg_license",
    "pkg_author",
    "hf_task",
    "hf_library",
    "so_is_answered",
    "hn_type",
    "hn_author",
    "arxiv_id",
    "devto_author",
    // Ingest source fields promoted to indexes for per-source filtering.
    "reddit_subreddit",
    "yt_channel",
    "local_project_key",
];

const CORE_KEYWORD_INDEX_FIELDS: &[&str] = &[
    "url",
    "domain",
    "source_type",
    "seed_url",
    "extractor_name",
    "local_project_key",
    "code_file_path",
    "code_path_prefixes",
    "chunk_content_kind",
];

const CORE_TYPED_FIELDS: &[(&str, &str)] = &[
    ("chunk_index", "integer"),
    ("payload_schema_version", "integer"),
    ("local_index_version", "integer"),
    ("local_generation", "integer"),
    ("scraped_at", "datetime"),
];

const MAX_INDEX_ATTEMPTS: u32 = 3;

fn payload_index_profile() -> &'static str {
    match std::env::var("AXON_QDRANT_PAYLOAD_INDEX_PROFILE")
        .unwrap_or_else(|_| "full".to_string())
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "core" | "minimal" => "core",
        _ => "full",
    }
}

fn payload_index_parallelism() -> usize {
    crate::ops::qdrant::env_usize_clamped("AXON_QDRANT_PAYLOAD_INDEX_PARALLELISM", 16, 1, 64)
}

/// PUT a single payload-index request with up to MAX_INDEX_ATTEMPTS attempts.
///
/// Backoff: 500ms after attempt 1, 1000ms after attempt 2. Idempotent — Qdrant
/// returns 200 when the index already exists, so retries are always safe.
async fn put_index_with_retry(
    client: reqwest::Client,
    url: String,
    body: serde_json::Value,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    for attempt in 1..=MAX_INDEX_ATTEMPTS {
        let result = client
            .put(&url)
            .json(&body)
            .send()
            .await
            .and_then(|r| r.error_for_status());
        match result {
            Ok(_) => return Ok(()),
            Err(e) => {
                if attempt < MAX_INDEX_ATTEMPTS {
                    tracing::warn!(
                        attempt,
                        max_attempts = MAX_INDEX_ATTEMPTS,
                        error = %e,
                        "qdrant payload index PUT transient error, retrying"
                    );
                    tokio::time::sleep(Duration::from_millis(500 * u64::from(attempt))).await;
                } else {
                    return Err(Box::new(e));
                }
            }
        }
    }
    unreachable!()
}

/// Creates keyword payload indexes on commonly-queried fields.
///
/// Required by the Qdrant `/facet` endpoint used by `domains` and `sources`.
/// The operation is idempotent — safe to call on every embed.
///
/// `collection_info` is the `GET /collections/{name}` response when the caller
/// already fetched it (ensure_collection does): fields present in its
/// `result.payload_schema` are skipped, so a warm collection issues **zero**
/// index PUTs instead of ~46 concurrent re-asserts per embed.
///
/// Index failures are non-fatal: an index is a query-time optimization, and a
/// slow/overloaded Qdrant must not turn an idempotent index assertion into a
/// failed embed. Missing indexes are retried on the next embed (cheaply, since
/// existing ones are skipped); facets on a still-unindexed field fail loudly
/// at query time.
pub(super) async fn ensure_payload_indexes(
    cfg: &Config,
    collection_info: Option<&serde_json::Value>,
) -> Result<(), Box<dyn Error>> {
    let existing: std::collections::HashSet<String> = collection_info
        .and_then(|info| info.pointer("/result/payload_schema"))
        .and_then(|schema| schema.as_object())
        .map(|fields| fields.keys().cloned().collect())
        .unwrap_or_default();

    let client = internal_service_http_client()?;
    let index_url = format!(
        "{}/collections/{}/index?wait=false",
        qdrant_base(cfg),
        cfg.collection
    );

    let profile = payload_index_profile();
    let keyword_fields = if profile == "core" {
        CORE_KEYWORD_INDEX_FIELDS
    } else {
        KEYWORD_INDEX_FIELDS
    };
    let typed_fields = if profile == "core" {
        CORE_TYPED_FIELDS
    } else {
        FULL_TYPED_FIELDS
    };

    // keyword(N) + typed(N)
    let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(KEYWORD_INDEX_FIELDS.len() + 18);

    for field in keyword_fields {
        let field = *field;
        if existing.contains(field) {
            continue;
        }
        let url = index_url.clone();
        let c = client.clone();
        futures.push(Box::pin(async move {
            put_index_with_retry(
                c,
                url,
                serde_json::json!({"field_name": field, "field_schema": "keyword"}),
            )
            .await
        }));
    }

    push_non_keyword_indexes(&mut futures, &index_url, &existing, typed_fields);

    if futures.is_empty() {
        return Ok(());
    }
    tracing::debug!(
        missing = futures.len(),
        existing = existing.len(),
        profile,
        "qdrant payload indexes: asserting missing fields"
    );
    let results = stream::iter(futures)
        .buffer_unordered(payload_index_parallelism())
        .collect::<Vec<_>>()
        .await;
    let failed = results.iter().filter(|r| r.is_err()).count();
    if failed > 0 {
        tracing::warn!(
            failed,
            "qdrant payload index assertion failed for {failed} field(s); \
             continuing — missing indexes are retried on the next embed"
        );
    }
    Ok(())
}

/// Appends integer, datetime, and bool index futures to the shared futures
/// vec, skipping fields already present in `existing`.
fn push_non_keyword_indexes<'a>(
    futures: &mut Vec<IndexFut<'a>>,
    index_url: &str,
    existing: &std::collections::HashSet<String>,
    typed_fields: &'static [(&'static str, &'static str)],
) {
    let client = match internal_service_http_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("push_non_keyword_indexes: failed to build HTTP client: {e}");
            return;
        }
    };
    for &(field, schema) in typed_fields {
        if existing.contains(field) {
            continue;
        }
        let url = index_url.to_string();
        let c = client.clone();
        futures.push(Box::pin(async move {
            put_index_with_retry(
                c,
                url,
                serde_json::json!({"field_name": field, "field_schema": schema}),
            )
            .await
        }));
    }
}

const FULL_TYPED_FIELDS: &[(&str, &str)] = &[
    ("chunk_index", "integer"),
    ("git_number", "integer"),
    ("git_comment_count", "integer"),
    ("git_repo_stars", "integer"),
    ("git_repo_forks", "integer"),
    ("git_repo_open_issues", "integer"),
    ("so_question_id", "integer"),
    ("payload_schema_version", "integer"),
    ("local_index_version", "integer"),
    ("local_generation", "integer"),
    ("code_file_size_bytes", "integer"),
    ("code_line_start", "integer"),
    ("code_line_end", "integer"),
    ("scraped_at", "datetime"),
    ("git_repo_is_fork", "bool"),
    ("git_repo_is_archived", "bool"),
    ("git_repo_is_private", "bool"),
    ("git_is_pr", "bool"),
    ("git_is_draft", "bool"),
    ("code_is_test", "bool"),
];

#[cfg(test)]
#[path = "payload_indexes_tests.rs"]
mod tests;
