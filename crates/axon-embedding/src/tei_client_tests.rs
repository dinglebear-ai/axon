//! Real-client integration tests for [`TeiEmbeddingProvider`] against a mock
//! HTTP server (httpmock). No live TEI is required.

use std::time::Duration;

use axon_api::source::*;
use chrono::Utc;
use httpmock::prelude::*;
use uuid::Uuid;

use crate::provider::EmbeddingProvider;
use crate::tei::{TeiEmbeddingConfig, TeiEmbeddingProvider};

fn config(
    endpoint: String,
    dimensions: u32,
    instruction: InstructionSupport,
) -> TeiEmbeddingConfig {
    TeiEmbeddingConfig {
        endpoint,
        model: "qwen3-embedding".to_string(),
        dimensions,
        timeout: Duration::from_secs(5),
        max_batch_inputs: 64,
        max_input_tokens: 8192,
        max_batch_tokens: 131_072,
        instruction_support: instruction,
    }
}

fn input(chunk_id: &str, text: &str) -> EmbeddingInput {
    EmbeddingInput {
        chunk_id: ChunkId::new(chunk_id),
        text: text.to_string(),
        content_kind: ContentKind::PlainText,
        metadata: MetadataMap::new(),
    }
}

fn batch(items: Vec<EmbeddingInput>, instruction: Option<String>) -> EmbeddingBatch {
    EmbeddingBatch {
        batch_id: BatchId::new(Uuid::from_u128(1)),
        job_id: JobId::new(Uuid::from_u128(2)),
        provider_id: ProviderId::new("tei"),
        model: "qwen3-embedding".to_string(),
        items,
        instruction,
        priority: JobPriority::Background,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn embed_returns_vectors_in_request_order() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed").body_includes("truncate");
            then.status(200).json_body(serde_json::json!([
                [0.1_f32, 0.2_f32],
                [0.3_f32, 0.4_f32],
                [0.5_f32, 0.6_f32],
            ]));
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None));
    let result = provider
        .embed(batch(
            vec![
                input("chunk-a", "first"),
                input("chunk-b", "second"),
                input("chunk-c", "third"),
            ],
            None,
        ))
        .await
        .expect("embed should succeed");

    let ids: Vec<_> = result
        .vectors
        .iter()
        .map(|v| v.chunk_id.0.as_str())
        .collect();
    assert_eq!(ids, vec!["chunk-a", "chunk-b", "chunk-c"]);
    assert_eq!(result.vectors[0].values, vec![0.1_f32, 0.2_f32]);
    assert_eq!(result.dimensions, 2);
    assert_eq!(result.provider_id, ProviderId::new("tei"));
    assert_eq!(result.model, "qwen3-embedding");
    assert_eq!(result.usage.requests, 1);
    assert!(result.warnings.is_empty());
}

#[tokio::test]
async fn embed_prepends_instruction_when_support_enabled() {
    let server = MockServer::start_async().await;
    // Only fires when the request body carries the instruction-prefixed text.
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/embed")
                .body_includes("PREFIX::hello");
            then.status(200)
                .json_body(serde_json::json!([[1.0_f32, 2.0_f32]]));
        })
        .await;

    let provider = TeiEmbeddingProvider::new(config(
        server.base_url(),
        2,
        InstructionSupport::QueryAndDocument,
    ));
    let result = provider
        .embed(batch(
            vec![input("chunk-a", "hello")],
            Some("PREFIX::".to_string()),
        ))
        .await
        .expect("embed should succeed with instruction");

    mock.assert_async().await;
    assert_eq!(result.vectors.len(), 1);
}

#[tokio::test]
async fn embed_ignores_instruction_when_support_none() {
    let server = MockServer::start_async().await;
    // Fires only for the RAW text (no prefix). If the provider wrongly prefixed
    // the input, this mock would not match and the request would 404.
    let mock = server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/embed")
                .body_includes("\"inputs\":[\"hello\"]");
            then.status(200)
                .json_body(serde_json::json!([[1.0_f32, 2.0_f32]]));
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None));
    let result = provider
        .embed(batch(
            vec![input("chunk-a", "hello")],
            Some("PREFIX::".to_string()),
        ))
        .await
        .expect("embed should succeed ignoring instruction");

    mock.assert_async().await;
    assert_eq!(result.vectors.len(), 1);
}

#[tokio::test]
async fn embed_splits_batch_on_413() {
    let server = MockServer::start_async().await;
    // 413 mock registered first (higher priority) — fires only on the full
    // 2-input batch (both inputs present in the body).
    server
        .mock_async(|when, then| {
            when.method(POST)
                .path("/embed")
                .body_includes("split-alpha")
                .body_includes("split-beta");
            then.status(413);
        })
        .await;
    // 200 fallback for the single-input halves after the split.
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32]]));
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None));
    let result = provider
        .embed(batch(
            vec![
                input("chunk-a", "split-alpha"),
                input("chunk-b", "split-beta"),
            ],
            None,
        ))
        .await
        .expect("embed should succeed after 413 split");

    assert_eq!(result.vectors.len(), 2);
    let ids: Vec<_> = result
        .vectors
        .iter()
        .map(|v| v.chunk_id.0.as_str())
        .collect();
    assert_eq!(ids, vec!["chunk-a", "chunk-b"]);
    // 1 full-batch request (413) + 2 single-input requests after split.
    assert_eq!(result.usage.requests, 3);
}

#[tokio::test]
async fn embed_surfaces_dimension_mismatch_warning() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed");
            // Server returns a 3-dim vector though config expects 2.
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32]]));
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None));
    let result = provider
        .embed(batch(vec![input("chunk-a", "hello")], None))
        .await
        .expect("embed should succeed but warn");

    assert_eq!(result.vectors.len(), 1);
    assert_eq!(result.warnings.len(), 1);
    assert_eq!(result.warnings[0].code, "embedding.tei.dimension_mismatch");
}

#[tokio::test]
async fn embed_rejects_zero_dimensions_without_network() {
    let provider = TeiEmbeddingProvider::new(config(
        "http://127.0.0.1:1".to_string(),
        0,
        InstructionSupport::None,
    ));
    let err = provider
        .embed(batch(vec![input("chunk-a", "hello")], None))
        .await
        .unwrap_err();
    assert_eq!(err.code.to_string(), "provider.invalid_dimensions");
    assert_eq!(err.provider_id.as_deref(), Some("tei"));
}

#[tokio::test]
async fn embed_surfaces_status_error_without_leaking_endpoint() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed");
            then.status(400);
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None));
    let err = provider
        .embed(batch(vec![input("chunk-a", "hello")], None))
        .await
        .unwrap_err();

    assert_eq!(err.code.to_string(), "embedding.tei.status");
    assert_eq!(err.provider_id.as_deref(), Some("tei"));
    // Endpoint is redacted to the opaque marker — the host/port never leak.
    assert_eq!(
        err.details.get("endpoint").map(String::as_str),
        Some("configured")
    );
    assert!(!err.message.contains(&server.base_url()));
    assert!(!err.message.contains("127.0.0.1"));
}

#[tokio::test]
async fn derive_embedding_identity_uses_info_model_and_probe_dimensions() {
    let server = MockServer::start_async().await;
    // `/info` reports the true model_id (with the org prefix the seed lacks).
    server
        .mock_async(|when, then| {
            when.method(GET).path("/info");
            then.status(200)
                .json_body(serde_json::json!({ "model_id": "Qwen/Qwen3-Embedding-0.6B" }));
        })
        .await;
    // The probe embed returns a 4-dim vector → derived dimensions = 4.
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32, 0.3_f32, 0.4_f32]]));
        })
        .await;

    // Seed the provider with the short model + wrong dims; derivation overrides.
    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 1024, InstructionSupport::None));
    let identity = provider
        .derive_embedding_identity()
        .await
        .expect("derive identity");

    assert_eq!(identity.model, "Qwen/Qwen3-Embedding-0.6B");
    assert_eq!(identity.dimensions, 4);
}

#[tokio::test]
async fn derive_embedding_identity_falls_back_to_config_model_when_info_lacks_model_id() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/info");
            then.status(200).json_body(serde_json::json!({}));
        })
        .await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32]]));
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None));
    let identity = provider
        .derive_embedding_identity()
        .await
        .expect("derive identity");

    // No model_id in /info → keep the configured seed model, but dimensions
    // still come from the live probe.
    assert_eq!(identity.model, "qwen3-embedding");
    assert_eq!(identity.dimensions, 2);
}

#[tokio::test]
async fn derive_embedding_identity_errors_when_info_unreachable() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(GET).path("/info");
            then.status(500);
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None));
    let err = provider.derive_embedding_identity().await.unwrap_err();
    // The status carries the opaque endpoint marker, never the raw host.
    assert!(!err.message.contains("127.0.0.1"));
}

/// End-to-end: retry-exhaustion on the real embedding path (not just the
/// legacy per-family runner) produces a cooling `ApiError`, and the live
/// `capabilities()` snapshot reflects it until a subsequent success clears
/// it — F5-10..13/V01/V03 in the provider-contract audit.
///
/// `with_max_attempts(1)` makes the first 503 the exhausting attempt so the
/// test does not wait out the real (multi-second) exponential backoff.
#[tokio::test]
async fn embed_retry_exhaustion_cools_the_provider_and_capabilities_report_it_live() {
    let server = MockServer::start_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed");
            then.status(503);
        })
        .await;

    let provider =
        TeiEmbeddingProvider::new(config(server.base_url(), 2, InstructionSupport::None))
            .with_max_attempts(1);

    let healthy = provider.capabilities().await.expect("capabilities");
    assert_eq!(healthy.health, HealthStatus::Healthy);
    assert!(healthy.cooldown_until.is_none());

    let err = provider
        .embed(batch(vec![input("chunk-a", "first")], None))
        .await
        .expect_err("persistent 503 must exhaust retries");
    let cooling = err
        .provider_cooling()
        .expect("retry-exhausted embed errors must carry ProviderCooling metadata");
    assert!(cooling.cooldown_until > Utc::now());

    // capabilities() is now LIVE, not a static always-healthy snapshot: it
    // reflects the just-recorded failure with the same provider and a
    // populated `cooldown_until`, and stops reporting available capacity.
    let cooling_caps = provider.capabilities().await.expect("capabilities");
    assert_eq!(cooling_caps.health, HealthStatus::Cooling);
    assert!(cooling_caps.cooldown_until.is_some());
    assert_eq!(cooling_caps.reservation_state.available_units, 0);

    // A subsequent successful embed — on the SAME provider/health tracker —
    // clears the cooldown back to healthy.
    server.reset_async().await;
    server
        .mock_async(|when, then| {
            when.method(POST).path("/embed");
            then.status(200)
                .json_body(serde_json::json!([[0.1_f32, 0.2_f32]]));
        })
        .await;
    provider
        .embed(batch(vec![input("chunk-b", "second")], None))
        .await
        .expect("embed against a now-healthy server succeeds");
    let recovered_caps = provider.capabilities().await.expect("capabilities");
    assert_eq!(recovered_caps.health, HealthStatus::Healthy);
    assert!(recovered_caps.cooldown_until.is_none());
}
