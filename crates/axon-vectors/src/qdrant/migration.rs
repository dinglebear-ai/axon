use axon_api::source::*;

use super::QdrantVectorStore;
use crate::bm42::compute_bm42_sparse;
use crate::collection::required_retrieval_payload_indexes;
use crate::store::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorMigrationReceipt {
    pub points_migrated: u64,
    pub pages_processed: u64,
}

/// Migrate a legacy unnamed-vector collection without exposing the concrete
/// Qdrant provider to application-service callers.
pub async fn migrate_unnamed_collection(
    qdrant_url: impl Into<String>,
    provider_id: impl Into<String>,
    from: &str,
    to: &str,
    page_size: usize,
) -> Result<VectorMigrationReceipt> {
    QdrantVectorStore::new(qdrant_url, provider_id)
        .migrate_unnamed_collection(from, to, page_size)
        .await
}

impl QdrantVectorStore {
    /// Migrate one legacy unnamed-vector collection through the configured
    /// provider boundary. Authentication, URL redaction, health tracking,
    /// retry policy, and write admission are inherited from this store.
    pub async fn migrate_unnamed_collection(
        &self,
        from: &str,
        to: &str,
        page_size: usize,
    ) -> Result<VectorMigrationReceipt> {
        let result = self
            .migrate_unnamed_collection_inner(from, to, page_size.max(1))
            .await;
        self.track(result).await
    }

    async fn migrate_unnamed_collection_inner(
        &self,
        from: &str,
        to: &str,
        page_size: usize,
    ) -> Result<VectorMigrationReceipt> {
        Self::validate_migration_names(from, to)?;
        let stage = ErrorStage::Upserting;
        let http = self.http()?;
        let dimensions = self.source_dimensions(&http, from).await?;
        self.ensure_migration_destination(&http, to, dimensions)
            .await?;

        let scroll_url = http.endpoint().collection_path(from, "points/scroll");
        let upsert_url = http
            .endpoint()
            .collection_path(to, "points?wait=true&ordering=strong");
        let mut receipt = VectorMigrationReceipt {
            points_migrated: 0,
            pages_processed: 0,
        };
        let first_body =
            serde_json::json!({"limit": page_size, "with_payload": true, "with_vector": true});
        let mut page: serde_json::Value = http
            .post_json(stage, &scroll_url, &first_body, "qdrant_migration_scroll")
            .await?;
        loop {
            let points = page
                .pointer("/result/points")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| {
                    ApiError::new(
                        "vector.migration.invalid_page",
                        stage,
                        "Qdrant migration scroll response omitted its points array",
                    )
                })?;
            if points.is_empty() {
                break;
            }
            let transformed = points
                .iter()
                .map(transform_point)
                .collect::<Result<Vec<_>>>()?;
            let upsert_body = serde_json::json!({"points": transformed});
            receipt.points_migrated = receipt.points_migrated.saturating_add(points.len() as u64);
            receipt.pages_processed = receipt.pages_processed.saturating_add(1);
            let next_offset = page
                .pointer("/result/next_page_offset")
                .cloned()
                .filter(|value| !value.is_null());
            let write_page = async {
                let _permit = self.write_permit(stage).await?;
                http.put_json(stage, &upsert_url, &upsert_body, "qdrant_migration_upsert")
                    .await
            };
            if let Some(offset) = next_offset {
                let next_body = serde_json::json!({
                    "limit": page_size,
                    "with_payload": true,
                    "with_vector": true,
                    "offset": offset,
                });
                let (write_result, next_page) = overlap_write_and_fetch(
                    write_page,
                    http.post_json(stage, &scroll_url, &next_body, "qdrant_migration_scroll"),
                )
                .await;
                write_result?;
                page = next_page?;
            } else {
                write_page.await?;
                break;
            }
        }
        Ok(receipt)
    }

    fn validate_migration_names(from: &str, to: &str) -> Result<()> {
        if from.trim().is_empty() || to.trim().is_empty() || from == to {
            return Err(ApiError::new(
                "vector.migration.invalid_collection",
                ErrorStage::Validation,
                "migration requires distinct non-empty source and destination collections",
            ));
        }
        Ok(())
    }

    async fn source_dimensions(&self, http: &super::http::QdrantHttp, from: &str) -> Result<u64> {
        let stage = ErrorStage::Upserting;
        let source_url = http.endpoint().collection_path(from, "");
        let source = http
            .get_json(stage, &source_url, "qdrant_migration_source")
            .await?
            .ok_or_else(|| {
                ApiError::new(
                    "vector.migration.source_missing",
                    stage,
                    "migration source collection does not exist",
                )
            })?;
        if source
            .pointer("/result/config/params/vectors/dense")
            .is_some()
        {
            return Err(ApiError::new(
                "vector.migration.source_named",
                stage,
                "migration source already uses named vectors",
            ));
        }
        source
            .pointer("/result/config/params/vectors/size")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| {
                ApiError::new(
                    "vector.migration.source_schema",
                    stage,
                    "migration source vector size is missing or invalid",
                )
            })
    }

    async fn ensure_migration_destination(
        &self,
        http: &super::http::QdrantHttp,
        to: &str,
        dimensions: u64,
    ) -> Result<()> {
        let stage = ErrorStage::Upserting;
        let destination_url = http.endpoint().collection_path(to, "");
        match http
            .get_json(stage, &destination_url, "qdrant_migration_destination")
            .await?
        {
            Some(existing) => {
                validate_destination_schema(&existing, dimensions)?;
            }
            None => {
                let _permit = self.write_permit(stage).await?;
                http.put_json(
                    stage,
                    &destination_url,
                    &serde_json::json!({
                        "vectors": {"dense": {"size": dimensions, "distance": "Cosine"}},
                        "sparse_vectors": {"bm42": {"modifier": "idf"}}
                    }),
                    "qdrant_migration_create_destination",
                )
                .await?;
            }
        }
        let migration_spec = CollectionSpec {
            collection: to.to_string(),
            dense: VectorConfig {
                name: "dense".to_string(),
                dimensions: u32::try_from(dimensions).map_err(|_| {
                    ApiError::new(
                        "vector.migration.destination_schema",
                        stage,
                        "migration vector dimensions exceed the supported range",
                    )
                })?,
                distance: VectorDistance::Cosine,
            },
            sparse: Some(SparseVectorConfig {
                name: "bm42".to_string(),
                modifier: SparseVectorModifier::Idf,
            }),
            payload_indexes: required_retrieval_payload_indexes(),
            aliases: Vec::new(),
            distance: Some(VectorDistance::Cosine),
            metadata: MetadataMap::new(),
        };
        self.ensure_payload_indexes(http, &migration_spec, stage)
            .await?;
        Ok(())
    }
}

fn validate_destination_schema(existing: &serde_json::Value, dimensions: u64) -> Result<()> {
    let params = existing.pointer("/result/config/params");
    let dense = params.and_then(|value| value.pointer("/vectors/dense"));
    let sparse = params.and_then(|value| value.pointer("/sparse_vectors/bm42"));
    let valid = dense
        .and_then(|value| value.get("size"))
        .and_then(serde_json::Value::as_u64)
        == Some(dimensions)
        && dense
            .and_then(|value| value.get("distance"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("cosine"))
        && sparse
            .and_then(|value| value.get("modifier"))
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value.eq_ignore_ascii_case("idf"));
    if valid {
        Ok(())
    } else {
        Err(ApiError::new(
            "vector.migration.destination_schema",
            ErrorStage::Upserting,
            "migration destination must use dense dimensions with Cosine distance and bm42 IDF",
        ))
    }
}

async fn overlap_write_and_fetch<W, F>(write: W, fetch: F) -> (W::Output, F::Output)
where
    W: Future,
    F: Future,
{
    tokio::join!(write, fetch)
}

fn transform_point(point: &serde_json::Value) -> Result<serde_json::Value> {
    let id = point.get("id").filter(|id| !id.is_null()).ok_or_else(|| {
        ApiError::new(
            "vector.migration.invalid_point",
            ErrorStage::Upserting,
            "migration point is missing its id",
        )
    })?;
    let vector = point
        .get("vector")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            ApiError::new(
                "vector.migration.invalid_point",
                ErrorStage::Upserting,
                "migration point is missing its dense vector",
            )
        })?;
    if vector.is_empty() || vector.iter().any(|value| value.as_f64().is_none()) {
        return Err(ApiError::new(
            "vector.migration.invalid_point",
            ErrorStage::Upserting,
            "migration point dense vector is empty or contains a non-number",
        ));
    }
    let payload = point
        .get("payload")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            ApiError::new(
                "vector.migration.invalid_point",
                ErrorStage::Upserting,
                "migration point is missing its payload object",
            )
        })?;
    let text = payload
        .get("chunk_text")
        .or_else(|| payload.get("text"))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            ApiError::new(
                "vector.migration.invalid_point",
                ErrorStage::Upserting,
                "migration point payload is missing chunk_text/text",
            )
        })?;
    let chunk_id = ChunkId::new(
        id.as_str()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| id.to_string()),
    );
    let sparse = compute_bm42_sparse(chunk_id, text);
    Ok(serde_json::json!({
        "id": id,
        "vector": {"dense": vector, "bm42": {"indices": sparse.indices, "values": sparse.values}},
        "payload": payload,
    }))
}

#[cfg(test)]
#[path = "migration_tests.rs"]
mod tests;
