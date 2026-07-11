use super::*;
use axon_api::source::*;
use uuid::Uuid;

use fake::{FakeEmbeddingMode, FakeEmbeddingProvider};
use provider::EmbeddingProvider;
use reservation::{ProviderReservationConfig, ProviderReservationManager};
use std::time::Duration;

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

fn input(chunk_id: &str, text: &str, content_kind: ContentKind) -> EmbeddingInput {
    EmbeddingInput {
        chunk_id: ChunkId::new(chunk_id),
        text: text.to_string(),
        content_kind,
        metadata: MetadataMap::new(),
    }
}

#[test]
fn batch_builder_rejects_empty_batches() {
    let err = batch::EmbeddingBatchBuilder::new(
        BatchId::new(Uuid::from_u128(10)),
        JobId::new(Uuid::from_u128(11)),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
    )
    .build()
    .unwrap_err();

    assert_eq!(err.code.to_string(), "embedding.batch_empty");
}

#[test]
fn batch_builder_rejects_duplicate_chunk_ids() {
    let err = batch::EmbeddingBatchBuilder::new(
        BatchId::new(Uuid::from_u128(12)),
        JobId::new(Uuid::from_u128(13)),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
    )
    .push_input(input("chunk-a", "first", ContentKind::PlainText))
    .push_input(input("chunk-a", "second", ContentKind::PlainText))
    .build()
    .unwrap_err();

    assert_eq!(err.code.to_string(), "embedding.duplicate_chunk_id");
    assert_eq!(err.chunk_id.as_deref(), Some("chunk-a"));
}

#[test]
fn batch_builder_rejects_blank_embedding_text() {
    let err = batch::EmbeddingBatchBuilder::new(
        BatchId::new(Uuid::from_u128(14)),
        JobId::new(Uuid::from_u128(15)),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
    )
    .push_input(input("chunk-a", "  \n\t", ContentKind::Markdown))
    .build()
    .unwrap_err();

    assert_eq!(err.code.to_string(), "embedding.blank_text");
    assert_eq!(err.chunk_id.as_deref(), Some("chunk-a"));
}

#[test]
fn batch_builder_accepts_mixed_content_kinds_and_preserves_order() {
    let built = batch::EmbeddingBatchBuilder::new(
        BatchId::new(Uuid::from_u128(16)),
        JobId::new(Uuid::from_u128(17)),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
    )
    .priority(JobPriority::Interactive)
    .push_input(input("chunk-a", "plain", ContentKind::PlainText))
    .push_input(input("chunk-b", "# markdown", ContentKind::Markdown))
    .push_input(input("chunk-c", "fn main() {}", ContentKind::Code))
    .build()
    .unwrap();

    let chunk_ids: Vec<_> = built
        .items
        .iter()
        .map(|item| item.chunk_id.0.as_str())
        .collect();
    assert_eq!(chunk_ids, vec!["chunk-a", "chunk-b", "chunk-c"]);

    let validation = batch::validate_batch(&built).unwrap();
    assert_eq!(validation.item_count, 3);
    assert_eq!(
        validation.content_kinds,
        vec![
            ContentKind::PlainText,
            ContentKind::Markdown,
            ContentKind::Code
        ]
    );
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
async fn fake_embedding_provider_preserves_input_order() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 2);
    let batch = batch::EmbeddingBatchBuilder::new(
        BatchId::new(Uuid::from_u128(20)),
        JobId::new(Uuid::from_u128(21)),
        ProviderId::new("fake-embedding"),
        "fake-embedding",
    )
    .push_input(input("chunk-a", "first", ContentKind::PlainText))
    .push_input(input("chunk-b", "second", ContentKind::PlainText))
    .push_input(input("chunk-c", "third", ContentKind::PlainText))
    .build()
    .unwrap();

    let result = provider.embed(batch).await.unwrap();
    let chunk_ids: Vec<_> = result
        .vectors
        .iter()
        .map(|vector| vector.chunk_id.0.as_str())
        .collect();

    assert_eq!(chunk_ids, vec!["chunk-a", "chunk-b", "chunk-c"]);
}

#[tokio::test]
async fn fake_embedding_provider_rejects_zero_dimensions() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 0);
    let err = provider
        .embed(batch(JobPriority::Background))
        .await
        .unwrap_err();

    assert_eq!(err.code.to_string(), "provider.invalid_dimensions");
}

#[tokio::test]
async fn fake_embedding_provider_rejects_raw_invalid_batches() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 4);

    let mut empty = batch(JobPriority::Background);
    empty.items.clear();
    let err = provider.embed(empty).await.unwrap_err();
    assert_eq!(err.code.to_string(), "embedding.batch_empty");

    let mut blank = batch(JobPriority::Background);
    blank.items[0].text = "  \n".to_string();
    let err = provider.embed(blank).await.unwrap_err();
    assert_eq!(err.code.to_string(), "embedding.blank_text");

    let mut duplicate = batch(JobPriority::Background);
    duplicate.items.push(duplicate.items[0].clone());
    let err = provider.embed(duplicate).await.unwrap_err();
    assert_eq!(err.code.to_string(), "embedding.duplicate_chunk_id");
}

#[tokio::test]
async fn fake_embedding_provider_rejects_provider_and_model_mismatches() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 4);

    let mut wrong_provider = batch(JobPriority::Background);
    wrong_provider.provider_id = ProviderId::new("other-provider");
    let err = provider.embed(wrong_provider).await.unwrap_err();
    assert_eq!(err.code.to_string(), "provider.provider_mismatch");

    let mut wrong_model = batch(JobPriority::Background);
    wrong_model.model = "other-model".to_string();
    let err = provider.embed(wrong_model).await.unwrap_err();
    assert_eq!(err.code.to_string(), "provider.model_mismatch");
}

#[tokio::test]
async fn fake_embedding_provider_call_records_do_not_expose_mutable_internals() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 4);

    provider
        .embed(batch(JobPriority::Background))
        .await
        .unwrap();

    let mut calls = provider.calls().await;
    calls.clear();

    assert_eq!(provider.calls().await.len(), 1);
}

#[tokio::test]
async fn fake_embedding_provider_determinism_uses_chunk_id_and_dimensions() {
    let four_dims = FakeEmbeddingProvider::new("fake-embedding", 4)
        .embed(batch(JobPriority::Background))
        .await
        .unwrap()
        .vectors
        .remove(0);
    let same_four_dims = FakeEmbeddingProvider::new("fake-embedding", 4)
        .embed(batch(JobPriority::Background))
        .await
        .unwrap()
        .vectors
        .remove(0);
    let six_dims = FakeEmbeddingProvider::new("fake-embedding", 6)
        .embed(batch(JobPriority::Background))
        .await
        .unwrap()
        .vectors
        .remove(0);

    assert_eq!(four_dims.values, same_four_dims.values);
    assert_ne!(four_dims.values, six_dims.values);
    assert_eq!(six_dims.values.len(), 6);
}

#[tokio::test]
async fn tei_adapter_config_is_reflected_in_capabilities_without_network_calls() {
    let provider = tei::TeiEmbeddingProvider::new(tei::TeiEmbeddingConfig {
        endpoint: "http://tei.local:8080".to_string(),
        model: "qwen3-embedding".to_string(),
        dimensions: 1024,
        timeout: Duration::from_secs(30),
        max_batch_inputs: 64,
        max_input_tokens: 8192,
        max_batch_tokens: 131_072,
        instruction_support: InstructionSupport::QueryAndDocument,
    });

    assert_eq!(provider.config().endpoint, "http://tei.local:8080");
    assert_eq!(provider.config().timeout, Duration::from_secs(30));
    assert_eq!(provider.config().max_batch_inputs, 64);

    let capability = provider.capabilities().await.unwrap();
    let embedding = capability.embedding.unwrap();

    // The TEI provider is now wired: it reports Available/Healthy with no
    // not_wired error and non-zero reservation capacity.
    assert_eq!(capability.provider_id, ProviderId::new("tei"));
    assert_eq!(capability.health, HealthStatus::Healthy);
    assert!(capability.last_error.is_none());
    assert_eq!(capability.reservation_state.available_units, 64);
    assert_eq!(capability.limits.timeout_ms, Some(30_000));
    assert!(capability.features.contains(&"http_client".to_string()));
    assert_eq!(embedding.model_id, "qwen3-embedding");
    assert_eq!(embedding.dimensions, 1024);
    assert_eq!(embedding.batch_limits.max_items, 64);
    assert_eq!(
        embedding.instruction_support,
        InstructionSupport::QueryAndDocument
    );
}

#[tokio::test]
async fn openai_compat_adapter_config_is_reflected_in_capabilities_without_network_calls() {
    let provider =
        openai_compat::OpenAiCompatEmbeddingProvider::new(openai_compat::OpenAiCompatConfig {
            base_url: "https://llm.example.test/v1".to_string(),
            model: "text-embedding-3-large".to_string(),
            dimensions: 3072,
            timeout: Duration::from_secs(45),
            max_batch_inputs: 96,
            max_input_tokens: 8191,
            max_batch_tokens: 196_608,
        });

    assert_eq!(provider.config().base_url, "https://llm.example.test/v1");
    assert_eq!(provider.config().timeout, Duration::from_secs(45));
    assert_eq!(provider.config().max_batch_inputs, 96);

    let capability = provider.capabilities().await.unwrap();
    let embedding = capability.embedding.unwrap();

    assert_eq!(capability.provider_id, ProviderId::new("openai-compat"));
    assert_eq!(capability.health, HealthStatus::Unavailable);
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.not_wired"
    );
    assert_eq!(capability.reservation_state.available_units, 0);
    assert_eq!(capability.limits.timeout_ms, Some(45_000));
    assert_eq!(embedding.model_id, "text-embedding-3-large");
    assert_eq!(embedding.dimensions, 3072);
    assert_eq!(embedding.batch_limits.max_items, 96);
    assert_eq!(embedding.max_batch_tokens, 196_608);
}

// OpenAiCompat remains a shell in this PR — assert its not_wired behavior
// separately now that TEI is a real client. (The TEI half of the old combined
// test moved to the mock-server tests in `tei_client_tests.rs`.)
#[tokio::test]
async fn openai_compat_shell_embed_returns_not_wired_error() {
    let openai =
        openai_compat::OpenAiCompatEmbeddingProvider::new(openai_compat::OpenAiCompatConfig {
            base_url: "https://llm.example.test/v1".to_string(),
            model: "text-embedding-3-large".to_string(),
            dimensions: 3072,
            timeout: Duration::from_secs(45),
            max_batch_inputs: 96,
            max_input_tokens: 8191,
            max_batch_tokens: 196_608,
        });

    let openai_err = openai
        .embed(batch(JobPriority::Background))
        .await
        .unwrap_err();

    assert_eq!(openai_err.code.to_string(), "provider.not_wired");
    assert_eq!(openai_err.provider_id.as_deref(), Some("openai-compat"));
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
async fn fake_embedding_provider_capabilities_reflect_failure_mode() {
    let timeout =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::Timeout);
    assert_eq!(
        timeout.capabilities().await.unwrap().health,
        HealthStatus::Degraded
    );

    let rate_limited =
        FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::RateLimited);
    let capability = rate_limited.capabilities().await.unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert!(capability.cooldown_until.is_some());
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.rate_limited"
    );

    let fatal = FakeEmbeddingProvider::new("fake-embedding", 8).with_mode(FakeEmbeddingMode::Fatal);
    let capability = fatal.capabilities().await.unwrap();
    assert_eq!(capability.health, HealthStatus::Unavailable);
    let error = capability.last_error.unwrap();
    assert_eq!(error.code.to_string(), "provider.fatal");
    assert_eq!(error.provider_id, Some("fake-embedding".to_string()));
    assert!(!error.retryable);

    let err = fatal
        .embed(batch(JobPriority::Interactive))
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "provider.fatal");
    assert!(!err.retryable);
}

#[tokio::test]
async fn fake_embedding_provider_with_cooldown_until_overrides_the_mode_derived_timestamp() {
    // `with_cooldown_until` lets a test simulate a live, "now"-relative
    // cooldown window instead of `FakeEmbeddingMode::RateLimited`'s fixed
    // timestamp.
    let cooldown_until = Timestamp::from(chrono::Utc::now() + chrono::Duration::seconds(45));
    let provider = FakeEmbeddingProvider::new("fake-embedding", 8)
        .with_mode(FakeEmbeddingMode::RateLimited)
        .with_cooldown_until(cooldown_until.clone());

    let capability = provider.capabilities().await.unwrap();
    assert_eq!(capability.health, HealthStatus::Cooling);
    assert_eq!(capability.cooldown_until, Some(cooldown_until));
}

#[tokio::test]
async fn fake_embedding_health_override_cannot_hide_failure_mode() {
    let provider = FakeEmbeddingProvider::new("fake-embedding", 8)
        .with_health(HealthStatus::Healthy)
        .with_mode(FakeEmbeddingMode::Fatal);

    let capability = provider.capabilities().await.unwrap();

    assert_eq!(capability.health, HealthStatus::Unavailable);
    assert_eq!(
        capability.last_error.unwrap().code.to_string(),
        "provider.fatal"
    );
}

#[tokio::test]
async fn reservations_preserve_interactive_capacity_under_background_load() {
    let reservations = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("fake-embedding"),
        provider_kind: ProviderKind::Embedding,
        capacity: 2,
        interactive_reserve: 1,
        cooldown_after_failures: 1,
        cooldown_secs: 30,
    });

    let _background = reservations
        .reserve(JobPriority::Background, 1)
        .await
        .unwrap();
    let interactive = reservations
        .reserve(JobPriority::Interactive, 1)
        .await
        .unwrap();

    assert_eq!(interactive.priority(), JobPriority::Interactive);
    assert_eq!(reservations.snapshot().await.active, 2);

    let denied = reservations
        .reserve(JobPriority::Background, 1)
        .await
        .unwrap_err();
    assert_eq!(denied.code.to_string(), "provider.capacity_exhausted");
}

#[tokio::test]
async fn background_reservations_account_for_requested_units_before_using_reserve() {
    let reservations = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("fake-embedding"),
        provider_kind: ProviderKind::Embedding,
        capacity: 4,
        interactive_reserve: 2,
        cooldown_after_failures: 1,
        cooldown_secs: 30,
    });

    let denied = reservations
        .reserve(JobPriority::Background, 3)
        .await
        .unwrap_err();

    assert_eq!(denied.code.to_string(), "provider.capacity_exhausted");
    assert_eq!(reservations.snapshot().await.active, 0);
}

#[tokio::test]
async fn reservation_drop_releases_capacity_synchronously() {
    let reservations = ProviderReservationManager::new(ProviderReservationConfig {
        provider_id: ProviderId::new("fake-embedding"),
        provider_kind: ProviderKind::Embedding,
        capacity: 1,
        interactive_reserve: 0,
        cooldown_after_failures: 1,
        cooldown_secs: 30,
    });
    {
        let _held = reservations
            .reserve(JobPriority::Interactive, 1)
            .await
            .unwrap();
        assert_eq!(reservations.snapshot().await.active, 1);
    }

    assert_eq!(reservations.snapshot().await.active, 0);
    reservations
        .reserve(JobPriority::Interactive, 1)
        .await
        .unwrap();
}
