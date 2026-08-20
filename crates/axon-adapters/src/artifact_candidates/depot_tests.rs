use super::*;
use axon_api::source::{
    ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION, ArtifactCandidate, ArtifactCandidateBatch,
    ArtifactCandidateSinkStatus, JobId, SourceGenerationId, SourceId, Timestamp,
};
use axon_core::http::LoopbackGuard;
use httpmock::prelude::*;
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

fn fixture_candidate() -> (serde_json::Value, ArtifactCandidate) {
    let value: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../axon-api/tests/fixtures/artifact-registry/artifact-candidate-v1.json"
    )))
    .expect("frozen Depot candidate fixture JSON");
    let candidate = serde_json::from_value(value.clone()).expect("frozen candidate fixture shape");
    (value, candidate)
}

fn batch(candidates: Vec<ArtifactCandidate>) -> ArtifactCandidateBatch {
    ArtifactCandidateBatch {
        contract_version: ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string(),
        delivery_id: "delivery-test".to_string(),
        idempotency_key: "idem-test".to_string(),
        job_id: JobId::from(Uuid::nil()),
        source_id: SourceId::from("src_test"),
        generation: SourceGenerationId::from("1"),
        produced_at: Timestamp("2026-08-20T05:00:00Z".to_string()),
        candidates,
    }
}

fn sink(server: &MockServer) -> DepotArtifactCandidateSink {
    DepotArtifactCandidateSink::new(&server.base_url(), "test-write-token")
        .expect("Depot HTTP sink")
}

#[tokio::test]
async fn depot_sink_posts_exact_frozen_v1_candidate_with_transport_only_auth() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (fixture, candidate) = fixture_candidate();
    let intake = server.mock(|when, then| {
        when.method(POST)
            .path("/api/operations/depot.artifacts.intake_candidate")
            .header("authorization", "Bearer test-write-token")
            .json_body(json!({"candidate": fixture.clone()}));
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({"result": {"candidate": fixture.clone()}}));
    });

    let result = sink(&server)
        .submit(batch(vec![candidate]))
        .await
        .expect("candidate intake succeeds");

    intake.assert_calls(1);
    assert_eq!(result.status, ArtifactCandidateSinkStatus::Accepted);
    assert_eq!(
        (result.attempted, result.accepted, result.rejected),
        (1, 1, 0)
    );
    assert!(result.warnings.is_empty());
}

#[tokio::test]
async fn depot_sink_capability_forces_single_candidate_sequential_delivery() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let capability = sink(&server).capabilities().await.expect("capability");
    assert_eq!(capability.name, "depot-http");
    assert_eq!(capability.max_batch_size, 1);
    assert_eq!(depot::DEPOT_MAX_IN_FLIGHT, 1);
    assert!(capability.supports_idempotency);
    assert_eq!(
        capability.contract_versions,
        vec![ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string()]
    );
}

#[tokio::test]
async fn depot_sink_rejects_multi_candidate_batch_before_network_delivery() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (_, candidate) = fixture_candidate();
    let result = sink(&server)
        .submit(batch(vec![candidate.clone(), candidate]))
        .await
        .expect("oversized sink batch degrades to rejection");
    assert_eq!(result.status, ArtifactCandidateSinkStatus::Rejected);
    assert_eq!(
        (result.attempted, result.accepted, result.rejected),
        (2, 0, 2)
    );
    assert_eq!(result.warnings.len(), 1);
    assert!(!result.warnings[0].retryable);
}

async fn rejected_http_status(status: u16, error: &str) -> ArtifactCandidateSinkResult {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (_, candidate) = fixture_candidate();
    let rejected = server.mock(|when, then| {
        when.method(POST)
            .path("/api/operations/depot.artifacts.intake_candidate")
            .header("authorization", "Bearer test-write-token");
        then.status(status)
            .header("content-type", "application/json")
            .json_body(json!({"error": error}));
    });
    let result = sink(&server)
        .submit(batch(vec![candidate]))
        .await
        .expect("4xx returns a non-retryable rejection receipt");
    rejected.assert_calls(1);
    result
}

#[tokio::test]
async fn depot_sink_classifies_auth_scope_and_candidate_4xx_as_non_retryable_rejections() {
    for (status, expected_code) in [
        (401, "source.artifact_candidate.depot.unauthorized"),
        (403, "source.artifact_candidate.depot.insufficient_scope"),
        (422, "source.artifact_candidate.depot.rejected"),
    ] {
        let result = rejected_http_status(status, "candidate rejected").await;
        assert_eq!(result.status, ArtifactCandidateSinkStatus::Rejected);
        assert_eq!(
            (result.attempted, result.accepted, result.rejected),
            (1, 0, 1)
        );
        assert_eq!(result.warnings[0].code, expected_code);
        assert!(!result.warnings[0].retryable);
    }
}

#[tokio::test]
async fn depot_sink_429_is_retryable_and_caps_retry_after_without_local_retry() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (_, candidate) = fixture_candidate();
    let limited = server.mock(|when, then| {
        when.method(POST)
            .path("/api/operations/depot.artifacts.intake_candidate");
        then.status(429).header("retry-after", "999");
    });
    let error = sink(&server)
        .submit(batch(vec![candidate]))
        .await
        .expect_err("429 is retryable provider failure");
    limited.assert_calls(1);
    assert_eq!(
        error.code.0,
        "adapter.artifact_candidate.depot.rate_limited"
    );
    assert!(error.retryable);
    assert_eq!(error.provider_id.as_deref(), Some("depot"));
    assert_eq!(error.retry_after_ms, Some(300_000));
}

#[tokio::test]
async fn depot_sink_503_is_retryable_without_hidden_local_retry() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (_, candidate) = fixture_candidate();
    let unavailable = server.mock(|when, then| {
        when.method(POST)
            .path("/api/operations/depot.artifacts.intake_candidate");
        then.status(503);
    });
    let error = sink(&server)
        .submit(batch(vec![candidate]))
        .await
        .expect_err("503 is retryable provider failure");
    unavailable.assert_calls(1);
    assert_eq!(error.code.0, "adapter.artifact_candidate.depot.unavailable");
    assert!(error.retryable);
    assert_eq!(error.provider_id.as_deref(), Some("depot"));
}

#[tokio::test]
async fn depot_sink_rejects_success_response_that_does_not_echo_submitted_candidate() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (fixture, candidate) = fixture_candidate();
    let mut changed = fixture.clone();
    changed["sourceProvider"] = json!("other");
    let intake = server.mock(|when, then| {
        when.method(POST)
            .path("/api/operations/depot.artifacts.intake_candidate");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({"result": {"candidate": changed}}));
    });
    let result = sink(&server)
        .submit(batch(vec![candidate]))
        .await
        .expect("mismatched echo degrades to rejection");
    intake.assert_calls(1);
    assert_eq!(result.status, ArtifactCandidateSinkStatus::Rejected);
    assert_eq!(
        result.warnings[0].code,
        "source.artifact_candidate.depot.echo_mismatch"
    );
}

#[tokio::test]
async fn depot_sink_does_not_follow_redirects_with_bearer_auth() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (fixture, candidate) = fixture_candidate();
    let redirect = server.mock(|when, then| {
        when.method(POST)
            .path("/api/operations/depot.artifacts.intake_candidate")
            .header("authorization", "Bearer test-write-token");
        then.status(307).header("location", "/redirect-target");
    });
    let redirected = server.mock(|when, then| {
        when.method(POST)
            .path("/redirect-target")
            .header("authorization", "Bearer test-write-token");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({"result": {"candidate": fixture}}));
    });

    let result = sink(&server)
        .submit(batch(vec![candidate]))
        .await
        .expect("redirect is a non-retryable protocol rejection");

    redirect.assert_calls(1);
    redirected.assert_calls(0);
    assert_eq!(result.status, ArtifactCandidateSinkStatus::Rejected);
    assert_eq!(
        result.warnings[0].code,
        "source.artifact_candidate.depot.protocol_status"
    );
}

#[tokio::test]
async fn depot_sink_serializes_concurrent_submissions_across_clones() {
    let _loopback = LoopbackGuard::allow();
    let server = MockServer::start();
    let (fixture, candidate) = fixture_candidate();
    let intake = server.mock(|when, then| {
        when.method(POST)
            .path("/api/operations/depot.artifacts.intake_candidate");
        then.status(200)
            .header("content-type", "application/json")
            .json_body(json!({"result": {"candidate": fixture}}));
    });
    let sink = sink(&server);
    let held_permit = sink
        .in_flight
        .clone()
        .acquire_owned()
        .await
        .expect("test holds the only Depot delivery permit");
    let concurrent_sink = sink.clone();
    let task = tokio::spawn(async move { concurrent_sink.submit(batch(vec![candidate])).await });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(!task.is_finished());
    intake.assert_calls(0);

    drop(held_permit);
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("submission proceeds once the shared permit is released")
        .expect("submission task joins")
        .expect("candidate intake succeeds");
    intake.assert_calls(1);
    assert_eq!(result.status, ArtifactCandidateSinkStatus::Accepted);
}

#[test]
fn depot_sink_rejects_missing_or_whitespace_padded_bearer_tokens() {
    for token in ["", "   "] {
        let error = DepotArtifactCandidateSink::new("https://depot.example.test", token)
            .err()
            .expect("missing Depot bearer token rejected");
        assert_eq!(
            error.code.0,
            "adapter.artifact_candidate.depot.token_missing"
        );
    }
    for token in [" token", "token ", "\ttoken"] {
        let error = DepotArtifactCandidateSink::new("https://depot.example.test", token)
            .err()
            .expect("whitespace-padded Depot bearer token rejected");
        assert_eq!(
            error.code.0,
            "adapter.artifact_candidate.depot.token_invalid"
        );
    }
}

#[test]
fn depot_sink_base_url_rejects_embedded_credentials_query_and_fragment() {
    for url in [
        "https://user:pass@example.test",
        "https://example.test?token=secret",
        "https://example.test#fragment",
        "file:///tmp/depot",
        "https://",
    ] {
        let error = DepotArtifactCandidateSink::new(url, "token")
            .err()
            .expect("unsafe Depot base URL rejected");
        assert_eq!(error.code.0, "adapter.artifact_candidate.depot.url_invalid");
    }
}
