//! Qdrant gRPC point publication.

use std::sync::Arc;

use axon_api::source::*;
use futures_util::{StreamExt, stream};
use qdrant_client::qdrant::{
    PointStruct, UpsertPoints, UpsertPointsBuilder, WriteOrdering, WriteOrderingType,
};

use super::QdrantVectorStore;
use super::qdrant_upsert_points;
use super::store_impl::request_usage;
use crate::store::Result;
use crate::store_helpers::stage_header;

#[async_trait::async_trait]
trait GrpcUpserter: Send + Sync {
    async fn upsert(&self, request: UpsertPoints) -> std::result::Result<(), String>;
}

#[async_trait::async_trait]
impl GrpcUpserter for qdrant_client::Qdrant {
    async fn upsert(&self, request: UpsertPoints) -> std::result::Result<(), String> {
        self.upsert_points(request)
            .await
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub(super) async fn upsert_batches_grpc(
    store: &QdrantVectorStore,
    spec: &CollectionSpec,
    batch: VectorPointBatch,
) -> Result<VectorStoreWriteResult> {
    let client = store.grpc_client.as_ref().cloned().ok_or_else(|| {
        ApiError::new(
            "vector.qdrant.grpc_unconfigured",
            ErrorStage::Upserting,
            "Qdrant gRPC transport was selected without a client",
        )
    })?;
    let collection = batch.collection.clone();
    let points_attempted = batch.points.len() as u64;
    let payload_indexes_created = batch
        .payload_indexes
        .iter()
        .map(|index| index.field_name.clone())
        .collect();
    let points = qdrant_upsert_points(spec, &batch)?;
    let chunks = points
        .chunks(store.point_buffer())
        .map(<[_]>::to_vec)
        .collect::<Vec<_>>();
    let (requests, barrier) = grpc_upsert_plan(&collection, chunks, store.async_writes);
    let client: Arc<dyn GrpcUpserter> = client;
    let requests = execute_grpc_plan(client, requests, barrier, store.write_parallelism()).await?;
    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Upserting),
        collection,
        points_attempted,
        points_written: points_attempted,
        payload_indexes_created,
        usage: request_usage(requests),
    })
}

async fn execute_grpc_plan(
    client: Arc<dyn GrpcUpserter>,
    writes: Vec<UpsertPoints>,
    barrier: Option<UpsertPoints>,
    parallelism: usize,
) -> Result<u64> {
    let mut pending = stream::iter(writes)
        .map(|request| {
            let client = Arc::clone(&client);
            async move {
                client.upsert(request).await.map_err(|error| {
                    ApiError::new(
                        "vector.qdrant.grpc_upsert",
                        ErrorStage::Upserting,
                        format!("Qdrant gRPC upsert failed: {error}"),
                    )
                })
            }
        })
        .buffer_unordered(parallelism.max(1));
    let mut requests = 0_u64;
    while let Some(response) = pending.next().await {
        response?;
        requests += 1;
    }
    drop(pending);
    if let Some(barrier) = barrier {
        client.upsert(barrier).await.map_err(|error| {
            ApiError::new(
                "vector.qdrant.grpc_barrier",
                ErrorStage::Upserting,
                format!("Qdrant gRPC completion barrier failed: {error}"),
            )
        })?;
        requests += 1;
    }
    Ok(requests)
}

fn grpc_upsert_plan(
    collection: &str,
    chunks: Vec<Vec<PointStruct>>,
    async_writes: bool,
) -> (Vec<UpsertPoints>, Option<UpsertPoints>) {
    let barrier = async_writes
        .then(|| chunks.last()?.last().cloned().map(|point| vec![point]))
        .flatten();
    let requests = chunks
        .into_iter()
        .map(|points| {
            UpsertPointsBuilder::new(collection.to_string(), points)
                .wait(!async_writes)
                .ordering(WriteOrdering {
                    r#type: WriteOrderingType::Strong.into(),
                })
                .build()
        })
        .collect::<Vec<_>>();
    let barrier = barrier.map(|points| {
        UpsertPointsBuilder::new(collection.to_string(), points)
            .wait(true)
            .ordering(WriteOrdering {
                r#type: WriteOrderingType::Strong.into(),
            })
            .build()
    });
    (requests, barrier)
}

#[cfg(test)]
#[path = "grpc_tests.rs"]
mod tests;
