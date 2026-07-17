//! Qdrant side of `axon reset`: inventory (point count + payload-schema
//! compatibility) and destructive drop + fresh named-mode recreation.

use axon_api::reset::TARGET_PAYLOAD_CONTRACT_VERSION;
use axon_core::config::Config;
use axon_core::http::http_client;
use reqwest::StatusCode;
use serde_json::Value;
use std::error::Error;

/// Qdrant REST base URL derived from `cfg.qdrant_url` (trailing slash
/// trimmed). Inlined here rather than pulled from the legacy `axon-vector`
/// crate — `axon-vectors` has no equivalent free function, and this is a
/// one-line derivation, not worth wrapping a whole `QdrantVectorStore` for.
fn qdrant_base(cfg: &Config) -> &str {
    cfg.qdrant_url.trim_end_matches('/')
}

/// Inventory of the configured Qdrant collection.
#[derive(Debug, Clone, Default)]
pub struct QdrantInventory {
    /// The collection exists.
    pub exists: bool,
    /// Total points currently stored (0 when absent/unreachable).
    pub points: u64,
    /// A point was observed without the current `payload_contract_version` —
    /// the store is schema-incompatible and must be reset before reuse.
    pub schema_incompatible: bool,
    /// Legacy compatibility field kept for callers/renderers that still show a
    /// numeric schema. Always `None` for current contract-version checks.
    pub min_schema_version: Option<u32>,
    /// Distinct payload contract versions observed in the sampled points.
    pub payload_contract_versions: Vec<String>,
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
/// sampled point fail the current vector payload contract version. Never mutates.
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

    let Some(points) = collection_point_count(client, base, collection).await else {
        return QdrantInventory {
            exists: true,
            unreachable: true,
            ..Default::default()
        };
    };
    let (payload_contract_versions, schema_incompatible) = if points > 0 {
        match payload_contract_versions(client, base, collection).await {
            Some(inventory) => inventory,
            None => {
                return QdrantInventory {
                    exists: true,
                    points,
                    unreachable: true,
                    ..Default::default()
                };
            }
        }
    } else {
        (Vec::new(), false)
    };

    QdrantInventory {
        exists: true,
        points,
        schema_incompatible,
        min_schema_version: None,
        payload_contract_versions,
        unreachable: false,
    }
}

async fn collection_point_count(
    client: &reqwest::Client,
    base: &str,
    collection: &str,
) -> Option<u64> {
    let url = format!("{base}/collections/{collection}/points/count");
    let body = serde_json::json!({ "exact": true });
    match client.post(&url).json(&body).send().await {
        Ok(resp) if resp.status().is_success() => resp
            .json::<Value>()
            .await
            .ok()
            .and_then(|v| v.pointer("/result/count").and_then(Value::as_u64)),
        _ => None,
    }
}

/// Scroll every point and read its `payload_contract_version`. Compatibility
/// is a collection-wide cutover invariant: sampling the first page can approve
/// a collection that still contains legacy points later in the id order.
async fn payload_contract_versions(
    client: &reqwest::Client,
    base: &str,
    collection: &str,
) -> Option<(Vec<String>, bool)> {
    let url = format!("{base}/collections/{collection}/points/scroll");
    let mut scan = PayloadContractScan::default();
    let mut offset = None;
    let mut seen_offsets = std::collections::BTreeSet::new();
    loop {
        let mut body = serde_json::json!({
            "limit": 256,
            "with_payload": ["payload_contract_version"],
            "with_vector": false,
        });
        if let Some(current) = offset.take() {
            body["offset"] = current;
        }
        let response = client.post(&url).json(&body).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        let page: Value = response.json().await.ok()?;
        let points = page.pointer("/result/points")?.as_array()?;
        scan.observe(points);
        if points.is_empty() {
            break;
        }
        let Some(next) = page.pointer("/result/next_page_offset").cloned() else {
            break;
        };
        if next.is_null() {
            break;
        }
        let offset_key = next.to_string();
        if !seen_offsets.insert(offset_key) {
            return None;
        }
        offset = Some(next);
    }
    Some(scan.finish())
}

#[derive(Default)]
struct PayloadContractScan {
    versions: std::collections::BTreeSet<String>,
    incompatible: bool,
}

impl PayloadContractScan {
    fn observe(&mut self, points: &[Value]) {
        for point in points {
            match point
                .pointer("/payload/payload_contract_version")
                .and_then(Value::as_str)
            {
                Some(version) => {
                    self.versions.insert(version.to_string());
                    self.incompatible |= version != TARGET_PAYLOAD_CONTRACT_VERSION;
                }
                None => {
                    self.versions.insert("<missing>".to_string());
                    self.incompatible = true;
                }
            }
        }
    }

    fn finish(self) -> (Vec<String>, bool) {
        let mut versions = self.versions.into_iter().collect::<Vec<_>>();
        versions.sort_by_key(|version| (version != "<missing>", version.clone()));
        (versions, self.incompatible)
    }
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

#[cfg(test)]
#[path = "qdrant_tests.rs"]
mod tests;
