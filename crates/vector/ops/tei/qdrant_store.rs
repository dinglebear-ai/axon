use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_info};
use crate::crates::vector::ops::qdrant::{env_usize_clamped, qdrant_base};
use reqwest::StatusCode;
use std::collections::HashSet;
use std::error::Error;
use std::sync::{Mutex, OnceLock};

static INITIALIZED_COLLECTIONS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

pub(super) fn collection_needs_init(name: &str) -> bool {
    let map = INITIALIZED_COLLECTIONS.get_or_init(|| Mutex::new(HashSet::new()));
    let mut set = map.lock().expect("INITIALIZED_COLLECTIONS mutex poisoned");
    if set.contains(name) {
        return false;
    }
    set.insert(name.to_owned());
    true
}

/// Creates keyword payload indexes on `url` and `domain` fields.
///
/// These indexes are required by the Qdrant `/facet` endpoint used by the
/// `domains` and `sources` MCP actions.  The operation is idempotent —
/// Qdrant returns HTTP 200 when the index already exists.
async fn ensure_payload_indexes(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let client = http_client()?;
    let index_url = format!(
        "{}/collections/{}/index?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
    // Code-specific indexes enable filtered searches without full collection scans
    // chunking_method: universal field present on all chunk types (not just GitHub)
    for field in &[
        "url",
        "domain",
        "source_type",
        "gh_file_language",
        "chunking_method",
    ] {
        client
            .put(&index_url)
            .json(&serde_json::json!({
                "field_name": field,
                "field_schema": "keyword"
            }))
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

pub(super) async fn ensure_collection(cfg: &Config, dim: usize) -> Result<(), Box<dyn Error>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);

    let get_resp = client.get(&url).send().await?;
    if get_resp.status().is_success() {
        let body: serde_json::Value = get_resp.json().await?;
        let existing_dim = body
            .pointer("/result/config/params/vectors/size")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        if existing_dim == dim {
            log_debug(&format!(
                "qdrant collection_exists collection={}",
                cfg.collection
            ));
            ensure_payload_indexes(cfg).await?;
            return Ok(());
        }
    }

    let create = serde_json::json!({
        "vectors": {"size": dim, "distance": "Cosine"}
    });
    let resp = client.put(&url).json(&create).send().await?;
    if resp.status() != StatusCode::CONFLICT {
        resp.error_for_status()?;
    }

    log_info(&format!(
        "qdrant collection_created collection={}",
        cfg.collection
    ));
    ensure_payload_indexes(cfg).await?;
    Ok(())
}

pub(super) async fn qdrant_upsert(
    cfg: &Config,
    points: &[serde_json::Value],
) -> Result<(), Box<dyn Error>> {
    if points.is_empty() {
        return Ok(());
    }
    let client = http_client()?;
    let upsert_batch_size = env_usize_clamped("AXON_QDRANT_UPSERT_BATCH_SIZE", 256, 1, 4096);
    let url = format!(
        "{}/collections/{}/points?wait=true",
        qdrant_base(cfg),
        cfg.collection
    );
    log_debug(&format!(
        "qdrant upsert_start point_count={} collection={}",
        points.len(),
        cfg.collection
    ));
    for batch in points.chunks(upsert_batch_size) {
        client
            .put(&url)
            .json(&serde_json::json!({"points": batch}))
            .send()
            .await?
            .error_for_status()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::ensure_collection;
    use crate::crates::jobs::common::{resolve_test_qdrant_url, test_config};
    use std::error::Error;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    #[ignore = "integration test — requires running Qdrant; run with cargo test -- --ignored"]
    async fn ensure_collection_is_idempotent() -> Result<(), Box<dyn Error>> {
        let Some(qdrant_url) = resolve_test_qdrant_url() else {
            return Ok(());
        };
        let mut cfg = test_config("postgresql://dummy@127.0.0.1:1/dummy");
        cfg.qdrant_url = qdrant_url;
        cfg.collection = format!("test_{}", uuid::Uuid::new_v4().simple());

        // First call creates the collection.
        ensure_collection(&cfg, 4).await?;
        // Second call must not error — verifies the GET-first bug fix (no 409 Conflict).
        ensure_collection(&cfg, 4).await?;

        // Cleanup: delete the ephemeral test collection.
        let base = cfg.qdrant_url.trim_end_matches('/');
        let _ = reqwest::Client::new()
            .delete(format!("{}/collections/{}", base, cfg.collection))
            .send()
            .await;
        Ok(())
    }
}
