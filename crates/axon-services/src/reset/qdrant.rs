//! Qdrant side of `axon reset`: inventory (point count + payload-schema
//! compatibility) and destructive drop + fresh named-mode recreation.

use axon_core::config::Config;
use axon_core::http::http_client;
use axon_vector::ops::qdrant::{PAYLOAD_SCHEMA_VERSION, qdrant_base};
use reqwest::StatusCode;
use serde_json::Value;
use std::error::Error;

/// Inventory of the configured Qdrant collection.
#[derive(Debug, Clone, Default)]
pub struct QdrantInventory {
    /// The collection exists.
    pub exists: bool,
    /// Total points currently stored (0 when absent/unreachable).
    pub points: u64,
    /// A point was observed carrying a `payload_schema_version` older than the
    /// current `PAYLOAD_SCHEMA_VERSION` — the store is schema-incompatible and
    /// must be reset before unified workers run.
    pub schema_incompatible: bool,
    /// Lowest payload schema version observed in the sampled points (`None`
    /// when empty/absent).
    pub min_schema_version: Option<u32>,
    /// The collection was unreachable during planning.
    pub unreachable: bool,
}

impl QdrantInventory {
    /// True when the collection holds data a reset would destroy.
    #[must_use]
    pub fn non_empty(&self) -> bool {
        self.exists && self.points > 0
    }
}

/// Read-only inventory: does the collection exist, how many points, and does any
/// sampled point carry an outdated payload schema version. Never mutates.
///
/// Best-effort — an unreachable Qdrant yields `unreachable = true` with zeroed
/// counts rather than an error, so planning/doctor degrade gracefully.
pub async fn inventory(cfg: &Config) -> QdrantInventory {
    let client = match http_client() {
        Ok(c) => c,
        Err(_) => {
            return QdrantInventory {
                unreachable: true,
                ..Default::default()
            };
        }
    };
    let base = qdrant_base(cfg);
    let collection = &cfg.collection;

    let info_url = format!("{base}/collections/{collection}");
    let resp = match client.get(&info_url).send().await {
        Ok(r) => r,
        Err(_) => {
            return QdrantInventory {
                unreachable: true,
                ..Default::default()
            };
        }
    };
    if resp.status() == StatusCode::NOT_FOUND {
        return QdrantInventory {
            exists: false,
            ..Default::default()
        };
    }
    if !resp.status().is_success() {
        return QdrantInventory {
            unreachable: true,
            ..Default::default()
        };
    }

    let points = collection_point_count(client, base, collection).await;
    let (min_schema_version, schema_incompatible) = if points > 0 {
        sample_schema_version(client, base, collection).await
    } else {
        (None, false)
    };

    QdrantInventory {
        exists: true,
        points,
        schema_incompatible,
        min_schema_version,
        unreachable: false,
    }
}

async fn collection_point_count(client: &reqwest::Client, base: &str, collection: &str) -> u64 {
    let url = format!("{base}/collections/{collection}/points/count");
    let body = serde_json::json!({ "exact": true });
    match client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.pointer("/result/count").and_then(Value::as_u64))
            .unwrap_or(0),
        _ => 0,
    }
}

/// Scroll a bounded sample of points and read their `payload_schema_version`.
/// Points with no version field are implicit schema `1` (pre-versioning), which
/// is incompatible with the current shape.
async fn sample_schema_version(
    client: &reqwest::Client,
    base: &str,
    collection: &str,
) -> (Option<u32>, bool) {
    let url = format!("{base}/collections/{collection}/points/scroll");
    let body = serde_json::json!({
        "limit": 256,
        "with_payload": ["payload_schema_version"],
        "with_vector": false,
    });
    let page: Value = match client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => match resp.json().await {
            Ok(v) => v,
            Err(_) => return (None, false),
        },
        _ => return (None, false),
    };
    let points = match page.pointer("/result/points").and_then(Value::as_array) {
        Some(p) if !p.is_empty() => p,
        _ => return (None, false),
    };
    let mut min_version: Option<u32> = None;
    for point in points {
        let version = point
            .pointer("/payload/payload_schema_version")
            .and_then(Value::as_u64)
            .map(|v| v as u32)
            .unwrap_or(1); // absent field ⇒ implicit v1
        min_version = Some(min_version.map_or(version, |m| m.min(version)));
    }
    let incompatible = min_version.is_some_and(|v| v < PAYLOAD_SCHEMA_VERSION);
    (min_version, incompatible)
}

/// Best-effort embedding dimension from TEI `/info`. Returns `None` when TEI is
/// unreachable or does not expose the field (older TEI releases).
pub async fn probe_tei_dim(cfg: &Config) -> Option<u64> {
    if cfg.tei_url.trim().is_empty() {
        return None;
    }
    let client = http_client().ok()?;
    for path in ["/info", "/v1/info"] {
        let url = format!("{}{path}", cfg.tei_url.trim_end_matches('/'));
        let Ok(resp) = client.get(&url).send().await else {
            continue;
        };
        if !resp.status().is_success() {
            continue;
        }
        let Ok(info) = resp.json::<Value>().await else {
            continue;
        };
        for key in ["embedding_dim", "dim", "hidden_size", "output_dim"] {
            if let Some(v) = info.get(key).and_then(Value::as_u64) {
                return Some(v);
            }
        }
    }
    None
}

/// Drop the configured collection (idempotent — a missing collection is a
/// success). Returns true when a collection was actually deleted.
pub async fn drop_collection(cfg: &Config) -> Result<bool, Box<dyn Error>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    let resp = client.delete(&url).send().await?;
    if resp.status() == StatusCode::NOT_FOUND {
        return Ok(false);
    }
    if !resp.status().is_success() {
        return Err(format!(
            "qdrant delete collection '{}' failed: {}",
            cfg.collection,
            resp.status()
        )
        .into());
    }
    Ok(true)
}

/// Create a fresh named-mode collection (dense + bm42 sparse) at `dim`. Mirrors
/// the schema `axon migrate` writes so hybrid RRF search works immediately.
pub async fn create_named_collection(cfg: &Config, dim: u64) -> Result<(), Box<dyn Error>> {
    let client = http_client()?;
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    client
        .put(&url)
        .json(&serde_json::json!({
            "vectors": { "dense": { "size": dim, "distance": "Cosine" } },
            "sparse_vectors": { "bm42": { "modifier": "idf" } }
        }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
