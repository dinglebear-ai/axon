use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::vector::ops::qdrant::qdrant_base;
use std::error::Error;
use std::future::Future;

type IndexFut<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'a>>;

/// Keyword fields indexed for filtering and `/facet` aggregations.
const KEYWORD_INDEX_FIELDS: &[&str] = &[
    "url",
    "domain",
    "source_type",
    "gh_file_language",
    // GitHub-specific indexed fields (top-level, not deprecated).
    "gh_language",
    "gh_file_type",
    "gh_topics",
    "chunking_method",
    "extractor_name",
    // Shared git provider schema (all git-backed ingest sources).
    "provider",
    "git_host",
    "git_owner",
    "git_repo",
    "git_content_kind",
    "git_state",
    "git_author",
    "git_file_language",
    "git_file_path",
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
];

/// Creates keyword payload indexes on commonly-queried fields.
///
/// Required by the Qdrant `/facet` endpoint used by `domains` and `sources`.
/// The operation is idempotent — safe to call on every embed.
pub(super) async fn ensure_payload_indexes(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = internal_service_http_client()?;
    let index_url = format!(
        "{}/collections/{}/index?wait=false",
        qdrant_base(cfg),
        cfg.collection
    );

    // keyword(N) + integer(8) + datetime(1) + bool(2) = N + 11
    let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(KEYWORD_INDEX_FIELDS.len() + 11);

    for field in KEYWORD_INDEX_FIELDS {
        let url = index_url.clone();
        futures.push(Box::pin(async move {
            client
                .put(&url)
                .json(&serde_json::json!({"field_name": field, "field_schema": "keyword"}))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }));
    }

    push_non_keyword_indexes(&mut futures, &index_url);

    let results = futures_util::future::join_all(futures).await;
    for result in results {
        result.map_err(|e| -> Box<dyn Error> { e })?;
    }
    Ok(())
}

/// Appends integer, datetime, and bool index futures to the shared futures vec.
fn push_non_keyword_indexes<'a>(futures: &mut Vec<IndexFut<'a>>, index_url: &str) {
    let integer_fields = [
        ("chunk_index", index_url.to_string()),
        ("git_number", index_url.to_string()),
        ("so_question_id", index_url.to_string()),
        ("payload_schema_version", index_url.to_string()),
        // GitHub-specific integer indexes (top-level, not deprecated).
        ("gh_stars", index_url.to_string()),
        ("gh_forks", index_url.to_string()),
        ("gh_line_start", index_url.to_string()),
        ("gh_line_end", index_url.to_string()),
    ];
    let client = match internal_service_http_client() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("push_non_keyword_indexes: failed to build HTTP client: {e}");
            return;
        }
    };
    for (field, url) in integer_fields {
        futures.push(Box::pin(async move {
            client
                .put(&url)
                .json(&serde_json::json!({"field_name": field, "field_schema": "integer"}))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }));
    }
    let datetime_url = index_url.to_string();
    futures.push(Box::pin(async move {
        client
            .put(&datetime_url)
            .json(&serde_json::json!({"field_name": "scraped_at", "field_schema": "datetime"}))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }));
    // Boolean indexes for GitHub flag fields (gh_is_fork, gh_is_archived).
    // Qdrant has a native "bool" index type — booleans must not use "keyword".
    let bool_fields = [
        ("gh_is_fork", index_url.to_string()),
        ("gh_is_archived", index_url.to_string()),
    ];
    for (field, url) in bool_fields {
        futures.push(Box::pin(async move {
            client
                .put(&url)
                .json(&serde_json::json!({"field_name": field, "field_schema": "bool"}))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }));
    }
}
