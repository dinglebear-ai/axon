use crate::core::config::Config;
use crate::core::http::http_client;
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
    let client = http_client()?;
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
    ];
    let mut futures: Vec<IndexFut<'_>> = Vec::with_capacity(keyword_fields.len() + 2);

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
