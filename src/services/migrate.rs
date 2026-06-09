//! Service-layer wrapper for collection migration (unnamed → named vectors).
//!
//! Scrolls the source Qdrant collection, computes BM42 sparse vectors from
//! `chunk_text` payloads, and upserts named-mode points (dense + bm42) to the
//! destination collection. No TEI calls; no re-crawling.

use crate::core::config::Config;
use crate::core::http::http_client;
use crate::core::logging::{log_info, log_warn};
use crate::services::types::MigrateResult;
use crate::vector::ops::sparse::compute_sparse_vector;
use crate::vector::ops::tei::qdrant_store::clear_collection_mode_cache;
use reqwest::StatusCode;
use std::error::Error;

/// Run the full migration from an unnamed-vector collection to a named-mode
/// collection (dense + bm42 sparse). Returns stats about the migration.
pub async fn migrate(cfg: &Config) -> Result<MigrateResult, Box<dyn Error>> {
    let from = cfg
        .positional
        .first()
        .ok_or("migrate requires --from <source-collection>")?
        .clone();
    let to = cfg
        .positional
        .get(1)
        .ok_or("migrate requires --to <destination-collection>")?
        .clone();

    if from == to {
        return Err(anyhow::anyhow!("--from and --to must be different collections").into());
    }

    log_info(&format!("command=migrate from={from} to={to}"));

    let client = http_client()?;
    let qdrant_url = crate::vector::ops::qdrant::qdrant_base(cfg);

    let dim = inspect_source_collection(client, qdrant_url, &from).await?;
    log_info(&format!("migrate source={from} dim={dim}"));

    ensure_named_collection(client, qdrant_url, &to, dim).await?;
    log_info(&format!("migrate dest_ready collection={to}"));

    let (total_points, pages) = scroll_and_upsert(client, qdrant_url, &from, &to).await?;

    log_info(&format!(
        "migrate complete from={from} to={to} points={total_points} pages={pages}"
    ));

    // Invalidate the process-wide VectorMode cache so long-running workers
    // re-detect the new schema on their next embed/query instead of continuing
    // on the stale Unnamed (dense-only) path.
    clear_collection_mode_cache(&from);
    clear_collection_mode_cache(&to);

    Ok(MigrateResult {
        from,
        to,
        points_migrated: total_points,
        pages_processed: pages,
    })
}

// ─── scroll + upsert loop ──────────────────────────────────────────────────

async fn scroll_and_upsert(
    client: &reqwest::Client,
    qdrant_url: &str,
    from: &str,
    to: &str,
) -> Result<(u64, u64), Box<dyn Error>> {
    let scroll_url = format!("{}/collections/{}/points/scroll", qdrant_url, from);
    let upsert_url = format!("{}/collections/{}/points?wait=true", qdrant_url, to);

    let mut offset: Option<serde_json::Value> = None;
    let mut total_points: u64 = 0;
    let mut pages: u64 = 0;

    loop {
        let mut body = serde_json::json!({
            "limit": 256,
            "with_payload": true,
            "with_vector": true,
        });
        if let Some(ref o) = offset {
            body["offset"] = o.clone();
        }

        let page: serde_json::Value = client
            .post(&scroll_url)
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        let points = match page["result"]["points"].as_array() {
            Some(pts) if !pts.is_empty() => pts,
            _ => break,
        };

        let named_points: Vec<serde_json::Value> = points
            .iter()
            .filter_map(|p| match transform_point(p) {
                Ok(np) => Some(np),
                Err(e) => {
                    log_warn(&format!("migrate skip_point id={} error={e}", p["id"]));
                    None
                }
            })
            .collect();

        if !named_points.is_empty() {
            upsert_batch_raw(client, &upsert_url, &named_points).await?;
            total_points += named_points.len() as u64;
        }

        pages += 1;
        if pages.is_multiple_of(100) {
            log_info(&format!(
                "migrate progress pages={pages} points={total_points}"
            ));
        }

        offset = page["result"]
            .get("next_page_offset")
            .cloned()
            .filter(|v| !v.is_null());
        if offset.is_none() {
            break;
        }
    }

    Ok((total_points, pages))
}

// ─── helpers ─────────────────────────────────────────────────────────────────

async fn inspect_source_collection(
    client: &reqwest::Client,
    qdrant_url: &str,
    collection: &str,
) -> Result<usize, Box<dyn Error>> {
    let url = format!("{}/collections/{}", qdrant_url, collection);
    let resp = client.get(&url).send().await?;
    if resp.status() == StatusCode::NOT_FOUND {
        return Err(anyhow::anyhow!("source collection '{collection}' not found").into());
    }
    let body: serde_json::Value = resp.error_for_status()?.json().await?;

    if body
        .pointer("/result/config/params/vectors/dense")
        .is_some()
    {
        return Err(anyhow::anyhow!(
            "source collection '{collection}' already uses named vectors; no migration needed"
        )
        .into());
    }

    let dim = body
        .pointer("/result/config/params/vectors/size")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("could not read vector size from collection '{collection}'"))?;

    Ok(dim as usize)
}

async fn ensure_named_collection(
    client: &reqwest::Client,
    qdrant_url: &str,
    collection: &str,
    dim: usize,
) -> Result<(), Box<dyn Error>> {
    let url = format!("{}/collections/{}", qdrant_url, collection);
    let resp = client.get(&url).send().await?;

    if resp.status().is_success() {
        let body: serde_json::Value = resp.json().await?;
        if body
            .pointer("/result/config/params/vectors/dense")
            .is_some()
        {
            // Verify bm42 sparse vector is also configured; without it scroll_and_upsert
            // will attempt to upsert sparse vectors into a collection that lacks that schema.
            if body
                .pointer("/result/config/params/sparse_vectors/bm42")
                .is_none()
            {
                return Err(anyhow::anyhow!(
                    "destination '{collection}' has 'dense' vectors but is missing 'bm42' sparse \
                     vectors; drop the collection or choose a different destination name"
                )
                .into());
            }
            log_info(&format!(
                "migrate dest_exists_named collection={collection}"
            ));
            return Ok(());
        }
        return Err(anyhow::anyhow!(
            "destination '{collection}' exists with unnamed vectors; choose a different name"
        )
        .into());
    } else if resp.status() != StatusCode::NOT_FOUND {
        let status = resp.status();
        return Err(anyhow::anyhow!("Qdrant GET collection/{collection} failed: {status}").into());
    }

    client
        .put(&url)
        .json(&serde_json::json!({
            "vectors": {
                "dense": {"size": dim, "distance": "Cosine"}
            },
            "sparse_vectors": {
                "bm42": {"modifier": "idf"}
            }
        }))
        .send()
        .await?
        .error_for_status()?;

    create_payload_indexes(client, qdrant_url, collection).await?;

    log_info(&format!("migrate dest_created collection={collection}"));
    Ok(())
}

async fn create_payload_indexes(
    client: &reqwest::Client,
    qdrant_url: &str,
    collection: &str,
) -> Result<(), Box<dyn Error>> {
    let index_url = format!("{}/collections/{}/index?wait=true", qdrant_url, collection);
    for field in &[
        "url",
        "domain",
        "source_type",
        "code_file_path",
        "code_language",
        "code_file_type",
        "code_chunking_method",
        "symbol_kind",
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
    for field in &["code_line_start", "code_line_end"] {
        client
            .put(&index_url)
            .json(&serde_json::json!({
                "field_name": field,
                "field_schema": "integer"
            }))
            .send()
            .await?
            .error_for_status()?;
    }
    client
        .put(&index_url)
        .json(&serde_json::json!({
            "field_name": "scraped_at",
            "field_schema": "datetime"
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

fn transform_point(point: &serde_json::Value) -> Result<serde_json::Value, Box<dyn Error>> {
    let id = &point["id"];
    if id.is_null() {
        return Err(anyhow::anyhow!("point has no id").into());
    }

    let dense_vec: Vec<f32> = point["vector"]
        .as_array()
        .ok_or("point missing vector array")?
        .iter()
        .enumerate()
        .map(|(i, v)| {
            v.as_f64()
                .map(|f| f as f32)
                .ok_or_else(|| format!("vector element {i} is not a number: {v}"))
        })
        .collect::<Result<_, _>>()?;

    if dense_vec.is_empty() {
        return Err(anyhow::anyhow!("point has empty dense vector").into());
    }

    let chunk_text = point["payload"]["chunk_text"]
        .as_str()
        .unwrap_or_else(|| point["payload"]["text"].as_str().unwrap_or(""));
    let sparse = compute_sparse_vector(chunk_text);

    Ok(serde_json::json!({
        "id": id,
        "vector": {
            "dense": dense_vec,
            "bm42": sparse
        },
        "payload": point["payload"]
    }))
}

async fn upsert_batch_raw(
    client: &reqwest::Client,
    upsert_url: &str,
    points: &[serde_json::Value],
) -> Result<(), Box<dyn Error>> {
    client
        .put(upsert_url)
        .json(&serde_json::json!({"points": points}))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

#[cfg(test)]
#[path = "migrate_tests.rs"]
mod tests;
