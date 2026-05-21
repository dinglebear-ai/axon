use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::vector::ops::qdrant::qdrant_base;
use std::error::Error;
use std::future::Future;

type IndexFut<'a> =
    std::pin::Pin<Box<dyn Future<Output = Result<(), Box<dyn Error + Send + Sync>>> + Send + 'a>>;

/// Creates keyword payload indexes on commonly-queried fields.
///
/// These indexes are required by the Qdrant `/facet` endpoint used by the
/// `domains` and `sources` MCP actions. The operation is idempotent.
pub(super) async fn ensure_payload_indexes(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = internal_service_http_client()?;
    let index_url = format!(
        "{}/collections/{}/index?wait=false",
        qdrant_base(cfg),
        cfg.collection
    );

    let keyword_fields = [
        "url",
        "domain",
        "source_type",
        "gh_file_language",
        "chunking_method",
        // axon_rust-lu6a: optional vertical-extractor identifier (e.g. "docs").
        // Indexed so `/facet` and term-match filters work; absent fields are
        // tolerated by Qdrant (point simply won't match an equality filter).
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
    ];
    let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(keyword_fields.len() + 3);

    for field in &keyword_fields {
        let url = index_url.clone();
        futures.push(Box::pin(async move {
            client
                .put(&url)
                .json(&serde_json::json!({
                    "field_name": field,
                    "field_schema": "keyword"
                }))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }));
    }

    let chunk_index_url = index_url.clone();
    futures.push(Box::pin(async move {
        client
            .put(&chunk_index_url)
            .json(&serde_json::json!({
                "field_name": "chunk_index",
                "field_schema": "integer"
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }));

    let git_number_url = index_url.clone();
    futures.push(Box::pin(async move {
        client
            .put(&git_number_url)
            .json(&serde_json::json!({
                "field_name": "git_number",
                "field_schema": "integer"
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }));

    // axon_rust-lu6a: integer index on payload_schema_version so retrieval can
    // filter `range: { gte: N }` efficiently and `/facet` can group by version.
    let schema_version_url = index_url.clone();
    futures.push(Box::pin(async move {
        client
            .put(&schema_version_url)
            .json(&serde_json::json!({
                "field_name": "payload_schema_version",
                "field_schema": "integer"
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }));

    let datetime_url = index_url;
    futures.push(Box::pin(async move {
        client
            .put(&datetime_url)
            .json(&serde_json::json!({
                "field_name": "scraped_at",
                "field_schema": "datetime"
            }))
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }));

    let results = futures_util::future::join_all(futures).await;
    for result in results {
        result.map_err(|e| -> Box<dyn Error> { e })?;
    }
    Ok(())
}
