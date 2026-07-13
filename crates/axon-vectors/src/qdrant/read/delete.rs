//! Canonical URI/prefix purge — ports legacy `axon-vector`'s `qdrant_delete_by_url`
//! (re-exported there as `axon_vector::purge`).

use axon_api::purge::PurgeResult;

use crate::qdrant::QdrantVectorStore;
use crate::store::Result;

impl QdrantVectorStore {
    /// Delete indexed points whose target canonical URI fields match `target`.
    ///
    /// With `prefix=true`, canonical URI fields also match descendants below
    /// the target path, using URL path boundaries so `https://x/docs` does not
    /// match `https://x/docs-old`. `dry_run=true` computes the same counts
    /// without deleting anything.
    ///
    /// Boundary-aware prefix matching is not expressible as a single Qdrant
    /// filter, so — like legacy — this scrolls the *entire* collection
    /// (projecting only canonical URI fields) and matches client-side before
    /// batch-deleting the matched point ids.
    pub async fn delete_by_url(
        &self,
        collection: &str,
        target: &str,
        prefix: bool,
        dry_run: bool,
    ) -> Result<PurgeResult> {
        let target = target.trim();
        if target.is_empty() {
            return Err(axon_api::source::ApiError::new(
                "vector.invalid_purge_target",
                axon_error::ErrorStage::Cleaning,
                "purge target URL cannot be empty",
            ));
        }

        let mut ids: Vec<serde_json::Value> = Vec::new();
        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut urls: std::collections::HashSet<String> = std::collections::HashSet::new();

        self.scroll_pages(
            collection,
            None,
            serde_json::json!({"include": [
                "item_canonical_uri",
                "source_canonical_uri",
                "source_item_key",
                "chunk_locator"
            ]}),
            256,
            |points| {
                for point in points {
                    let values = canonical_values(&point.payload);
                    if !point_matches_url_target(&values, target, prefix) {
                        continue;
                    }
                    if let Some(url) = values.first().filter(|value| !value.is_empty()) {
                        urls.insert((*url).to_string());
                    }
                    if seen_ids.insert(point.id.to_string()) {
                        ids.push(point.id.clone());
                    }
                }
                true
            },
        )
        .await?;

        let deleted_points = if dry_run {
            0
        } else {
            self.delete_points_by_id(collection, &ids).await?
        };

        let mut sample_urls: Vec<String> = urls.iter().cloned().collect();
        sample_urls.sort();
        sample_urls.truncate(20);

        Ok(PurgeResult {
            target: target.to_string(),
            prefix,
            dry_run,
            matched_points: ids.len(),
            deleted_points,
            matched_url_count: urls.len(),
            sample_urls,
        })
    }

    /// Batch-delete points by id (`points/delete`, 1000 ids per request).
    async fn delete_points_by_id(
        &self,
        collection: &str,
        ids: &[serde_json::Value],
    ) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }
        let stage = axon_error::ErrorStage::Cleaning;
        let http = self.http()?;
        let url = http
            .endpoint()
            .collection_path(collection, "points/delete?wait=true");
        for batch in ids.chunks(1000) {
            let body = serde_json::json!({ "points": batch });
            let _ack: serde_json::Value = http
                .post_json(stage, &url, &body, "qdrant_delete_points")
                .await?;
        }
        Ok(ids.len())
    }
}

fn canonical_values(payload: &serde_json::Value) -> Vec<&str> {
    let mut values = Vec::new();
    for field in [
        "item_canonical_uri",
        "source_canonical_uri",
        "source_item_key",
    ] {
        if let Some(value) = payload.get(field).and_then(serde_json::Value::as_str) {
            values.push(value);
        }
    }
    if let Some(value) = payload
        .get("chunk_locator")
        .and_then(serde_json::Value::as_object)
        .and_then(|locator| locator.get("canonical_uri"))
        .and_then(serde_json::Value::as_str)
    {
        values.push(value);
    }
    values
}

fn point_matches_url_target(values: &[&str], target: &str, prefix: bool) -> bool {
    values
        .iter()
        .any(|value| url_matches_target(value, target, prefix))
}

/// Ports legacy `url_matches_target`: exact match, or (when `prefix`) a
/// path-boundary-aware prefix match so `https://x/docs` does not also match
/// `https://x/docs-old`.
fn url_matches_target(value: &str, target: &str, prefix: bool) -> bool {
    if value == target {
        return true;
    }
    if !prefix {
        return false;
    }
    if value.starts_with(target) && target.ends_with('/') {
        return true;
    }
    value.strip_prefix(target).is_some_and(|rest| {
        matches!(
            rest.as_bytes().first(),
            Some(b'/') | Some(b'?') | Some(b'#')
        )
    })
}

#[cfg(test)]
#[path = "delete_tests.rs"]
mod tests;
