//! Collection migration: unnamed-mode → named-mode (dense + bm42).
//!
//! Scrolls the source collection page-by-page with stored vectors included,
//! computes BM42 sparse vectors locally from stored `chunk_text` payloads,
//! and upserts named-mode points to the destination collection.
//! No TEI calls; no re-crawling.

use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_info, log_warn};
use crate::crates::vector::ops::sparse::compute_sparse_vector;
use reqwest::StatusCode;
use std::error::Error;

// ─── public entry point ──────────────────────────────────────────────────────

pub async fn run_migrate(cfg: &Config) -> Result<(), Box<dyn Error>> {
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
        return Err("--from and --to must be different collections".into());
    }

    log_info(&format!("command=migrate from={from} to={to}"));

    let client = http_client()?;
    let qdrant_url = crate::crates::vector::ops::qdrant::qdrant_base(cfg);

    let dim = inspect_source_collection(client, qdrant_url, &from).await?;
    log_info(&format!("migrate source={from} dim={dim}"));

    ensure_named_collection(client, qdrant_url, &to, dim).await?;
    log_info(&format!("migrate dest_ready collection={to}"));

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

    log_info(&format!(
        "migrate complete from={from} to={to} points={total_points} pages={pages}"
    ));

    if cfg.json_output {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "from": from,
                "to": to,
                "points_migrated": total_points,
                "pages_processed": pages,
            })
        );
    } else {
        println!("Migration complete: {total_points} points copied from '{from}' → '{to}'");
        println!("Next: set AXON_COLLECTION={to} in your .env to use hybrid search.");
    }

    Ok(())
}

// ─── helpers ─────────────────────────────────────────────────────────────────

/// Read the source collection schema and return the dense vector dimension.
///
/// Returns an error if the collection does not exist, is unreachable, or is
/// already a Named-mode collection (no migration needed).
async fn inspect_source_collection(
    client: &reqwest::Client,
    qdrant_url: &str,
    collection: &str,
) -> Result<usize, Box<dyn Error>> {
    let url = format!("{}/collections/{}", qdrant_url, collection);
    let resp = client.get(&url).send().await?;
    if resp.status() == StatusCode::NOT_FOUND {
        return Err(format!("source collection '{collection}' not found").into());
    }
    let body: serde_json::Value = resp.error_for_status()?.json().await?;

    // Named collection — already migrated
    if body
        .pointer("/result/config/params/vectors/dense")
        .is_some()
    {
        return Err(format!(
            "source collection '{collection}' already uses named vectors; no migration needed"
        )
        .into());
    }

    // Unnamed collection — extract dim
    let dim = body
        .pointer("/result/config/params/vectors/size")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| format!("could not read vector size from collection '{collection}'"))?;

    Ok(dim as usize)
}

/// Create `collection` as a Named-mode collection (dense + bm42) if it does
/// not already exist. If it exists and is already Named, this is a no-op.
/// Returns an error if it exists as Unnamed (name collision).
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
            log_info(&format!(
                "migrate dest_exists_named collection={collection}"
            ));
            return Ok(());
        }
        return Err(format!(
            "destination '{collection}' exists with unnamed vectors; choose a different name"
        )
        .into());
    } else if resp.status() != StatusCode::NOT_FOUND {
        let status = resp.status();
        return Err(format!("Qdrant GET collection/{collection} failed: {status}").into());
    }

    // Create Named collection
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

    // Payload indexes (required for /facet endpoints used by sources/domains)
    let index_url = format!("{}/collections/{}/index?wait=true", qdrant_url, collection);
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
    client
        .put(&index_url)
        .json(&serde_json::json!({
            "field_name": "scraped_at",
            "field_schema": "datetime"
        }))
        .send()
        .await?
        .error_for_status()?;

    log_info(&format!("migrate dest_created collection={collection}"));
    Ok(())
}

/// Convert a single point from unnamed-mode (flat vector array) to named-mode
/// (dense + bm42 named vectors).
///
/// `point` is the raw JSON from a Qdrant scroll response with `with_vector: true`.
fn transform_point(point: &serde_json::Value) -> Result<serde_json::Value, Box<dyn Error>> {
    let id = &point["id"];
    if id.is_null() {
        return Err("point has no id".into());
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
        return Err("point has empty dense vector".into());
    }

    let chunk_text = point["payload"]["chunk_text"]
        .as_str()
        .unwrap_or_else(|| point["payload"]["text"].as_str().unwrap_or(""));
    let sparse = compute_sparse_vector(chunk_text);

    Ok(serde_json::json!({
        "id": id,
        "vector": {
            "dense": dense_vec,
            "bm42": sparse.to_json()
        },
        "payload": point["payload"]
    }))
}

/// POST a batch of pre-formed point JSON objects to the Qdrant upsert endpoint.
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
mod tests {
    use super::*;

    #[test]
    fn transform_point_converts_unnamed_to_named() {
        let p = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440001",
            "vector": [1.0_f64, 0.0, 0.0, 0.5],
            "payload": {"chunk_text": "rust async programming", "url": "https://example.com", "chunk_index": 0}
        });
        let result = transform_point(&p).unwrap();
        let dense = &result["vector"]["dense"];
        assert!(dense.is_array());
        let arr = dense.as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert!((arr[0].as_f64().unwrap() - 1.0).abs() < 1e-6);
        let bm42 = &result["vector"]["bm42"];
        assert!(bm42["indices"].is_array());
        assert!(bm42["values"].is_array());
        assert!(!bm42["indices"].as_array().unwrap().is_empty());
        assert_eq!(result["payload"]["url"], "https://example.com");
        assert_eq!(result["payload"]["chunk_index"], 0);
        assert_eq!(result["id"], "550e8400-e29b-41d4-a716-446655440001");
    }

    #[test]
    fn transform_point_empty_chunk_text_yields_empty_sparse() {
        let p = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440002",
            "vector": [0.1_f64, 0.2, 0.3, 0.4],
            "payload": {"chunk_text": "", "url": "https://example.com"}
        });
        let result = transform_point(&p).unwrap();
        let indices = result["vector"]["bm42"]["indices"].as_array().unwrap();
        assert!(
            indices.is_empty(),
            "empty text should produce empty sparse vector"
        );
    }

    #[test]
    fn transform_point_falls_back_to_text_field() {
        let p = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440003",
            "vector": [0.5_f64, 0.5, 0.0, 0.0],
            "payload": {"text": "vector database search", "url": "https://example.com"}
        });
        let result = transform_point(&p).unwrap();
        assert!(result["vector"]["bm42"]["indices"].is_array());
    }

    #[test]
    fn transform_point_missing_vector_returns_error() {
        let p = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440004",
            "payload": {"chunk_text": "some text"}
        });
        assert!(transform_point(&p).is_err());
    }

    #[test]
    fn transform_point_empty_vector_returns_error() {
        let p = serde_json::json!({
            "id": "550e8400-e29b-41d4-a716-446655440005",
            "vector": [],
            "payload": {"chunk_text": "some text"}
        });
        assert!(transform_point(&p).is_err());
    }
}
