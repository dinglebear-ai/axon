//! Qdrant gRPC point publication.

use std::sync::Arc;

use axon_api::source::*;
use futures_util::{StreamExt, stream};
use qdrant_client::qdrant::UpsertPointsBuilder;

use super::QdrantVectorStore;
use super::qdrant_upsert_points;
use super::store_impl::request_usage;
use crate::store::Result;
use crate::store_helpers::stage_header;

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
    let barrier_points = store.async_writes.then(|| chunks.last().cloned()).flatten();
    let mut pending = stream::iter(chunks)
        .map(|points| {
            let client = Arc::clone(&client);
            let collection = collection.clone();
            async move {
                client
                    .upsert_points(
                        UpsertPointsBuilder::new(collection, points).wait(!store.async_writes),
                    )
                    .await
                    .map_err(|error| {
                        ApiError::new(
                            "vector.qdrant.grpc_upsert",
                            ErrorStage::Upserting,
                            format!("Qdrant gRPC upsert failed: {error}"),
                        )
                    })
            }
        })
        .buffer_unordered(store.write_parallelism());
    let mut requests = 0_u64;
    while let Some(response) = pending.next().await {
        response?;
        requests += 1;
    }
    drop(pending);
    if let Some(points) = barrier_points {
        client
            .upsert_points(UpsertPointsBuilder::new(collection.clone(), points).wait(true))
            .await
            .map_err(|error| {
                ApiError::new(
                    "vector.qdrant.grpc_barrier",
                    ErrorStage::Upserting,
                    format!("Qdrant gRPC completion barrier failed: {error}"),
                )
            })?;
        requests += 1;
    }
    Ok(VectorStoreWriteResult {
        header: stage_header(PipelinePhase::Upserting),
        collection,
        points_attempted,
        points_written: points_attempted,
        payload_indexes_created,
        usage: request_usage(requests),
    })
}
