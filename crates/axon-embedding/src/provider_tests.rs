use axon_api::source::*;
use uuid::Uuid;

use crate::fake::{FakeEmbeddingMode, FakeEmbeddingProvider};
use crate::provider::EmbeddingProvider;
use crate::reservation::ProviderReservations;

fn batch(priority: JobPriority) -> EmbeddingBatch {
    EmbeddingBatch {
        batch_id: BatchId::new(Uuid::from_u128(1)),
        job_id: JobId::new(Uuid::from_u128(2)),
        provider_id: ProviderId::new("fake-embedding"),
        model: "fake-embedding".to_string(),
        items: vec![EmbeddingInput {
            chunk_id: ChunkId::new("chunk-a"),
            text: "hello world".to_string(),
            content_kind: ContentKind::PlainText,
            metadata: MetadataMap::new(),
        }],
        instruction: None,
        priority,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn fake_embedding_provider_returns_deterministic_dimensioned_vectors() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 4);

    let first = provider
        .embed(batch(JobPriority::Background))
        .await
        .unwrap();
    let second = provider
        .embed(batch(JobPriority::Background))
        .await
        .unwrap();

    assert_eq!(first.dimensions, 4);
    assert_eq!(first.vectors, second.vectors);
    assert_eq!(provider.calls().await.len(), 2);
}

#[tokio::test]
async fn fake_embedding_provider_reports_capability_and_health_overrides() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 8)
        .with_health(HealthStatus::Cooling)
        .with_mode(FakeEmbeddingMode::RateLimited);

    let capability = provider.capabilities().await.unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert_eq!(capability.provider_kind, ProviderKind::Embedding);

    let err = provider
        .embed(batch(JobPriority::Interactive))
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "provider.rate_limited");
    assert!(err.retryable);
}

#[tokio::test]
async fn reservations_preserve_interactive_capacity_under_background_load() {
    let reservations = ProviderReservations::new(2, 1);

    let _background = reservations
        .reserve(
            ProviderId::new("fake-embedding"),
            JobPriority::Background,
            1,
        )
        .await
        .unwrap();
    let interactive = reservations
        .reserve(
            ProviderId::new("fake-embedding"),
            JobPriority::Interactive,
            1,
        )
        .await
        .unwrap();

    assert_eq!(interactive.priority(), JobPriority::Interactive);
    assert_eq!(reservations.snapshot().await.active, 2);

    let denied = reservations
        .reserve(
            ProviderId::new("fake-embedding"),
            JobPriority::Background,
            1,
        )
        .await
        .unwrap_err();
    assert_eq!(denied.code.to_string(), "provider.capacity_exhausted");
}
