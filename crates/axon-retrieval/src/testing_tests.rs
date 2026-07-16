use std::sync::Arc;

use axon_api::mcp_schema::AskRequest;
use axon_api::source::{
    CapabilityBase, ChunkId, DocumentId, HealthStatus, MetadataMap, PublishGenerationRequest,
    PublishGenerationResult, PublishPlan, RedactionMetadata, RedactionStatus, RetrievalCapability,
    SourceGenerationId, SourceId, SourceItemKey, SourceRange, StageCounts, StageResultHeader,
    Timestamp, Visibility,
};

use crate::boundary::RetrievalEngine as BoundaryRetrievalEngine;
use crate::citation::Citation;
use crate::context::ContextBundle;
use crate::plan::RetrievalPlan;
use crate::publish::GenerationPublisher;
use crate::query::{QueryRequest, RetrievalMatch, RetrievalRequest, RetrievalResult};
use crate::testing::{
    FakeGenerationPublisher, FakeGenerationPublisherMode, FakeRetrievalEngine, FakeRetrievalMode,
};

fn empty_range() -> SourceRange {
    SourceRange {
        line_start: None,
        line_end: None,
        byte_start: None,
        byte_end: None,
        char_start: None,
        char_end: None,
        time_start_ms: None,
        time_end_ms: None,
        dom_selector: None,
        json_pointer: None,
        yaml_path: None,
        xml_xpath: None,
        csv_row: None,
        session_turn_id: None,
        turn_start: None,
        turn_end: None,
    }
}

fn sample_citation() -> Citation {
    Citation {
        source_id: SourceId::new("src-docs"),
        source_item_key: SourceItemKey::new("chunk-a"),
        generation: SourceGenerationId::new("1"),
        document_id: DocumentId::new("doc-chunk-a"),
        chunk_id: ChunkId::new("chunk-a"),
        job_id: axon_api::source::JobId::new(uuid::Uuid::from_u128(1)),
        canonical_uri: "https://example.com/chunk-a".to_string(),
        range: SourceRange {
            line_start: Some(1),
            line_end: Some(3),
            ..empty_range()
        },
        redaction: RedactionMetadata {
            redaction_status: RedactionStatus::Clean,
            redaction_version: "2026-07-16".to_string(),
            visibility: Visibility::Public,
            redacted_field_count: 0,
            dropped_field_count: 0,
            detector_count: 0,
            detector_names: Vec::new(),
        },
    }
}

fn sample_result() -> RetrievalResult {
    let citation = sample_citation();
    let matches = vec![RetrievalMatch {
        chunk_id: ChunkId::new("chunk-a"),
        document_id: DocumentId::new("doc-chunk-a"),
        source_id: SourceId::new("src-docs"),
        score: 0.9,
        canonical_uri: "https://example.com/chunk-a".to_string(),
        text: "Alpha chunk body".to_string(),
        citation: citation.clone(),
    }];
    let context = ContextBundle::from_matches(&matches, 4096, 512);
    RetrievalResult {
        plan: RetrievalPlan {
            collection: "axon-test".to_string(),
            limit: 3,
            source_id: Some(SourceId::new("src-docs")),
            generation: None,
            allowed_visibility: vec![Visibility::Public],
            namespace_filters: Vec::new(),
            excluded_source_kinds: Vec::new(),
            byte_budget: 4096,
            token_budget: 512,
        },
        citations: vec![citation],
        context,
        matches,
    }
}

fn sample_request() -> RetrievalRequest {
    RetrievalRequest {
        query: "alpha".to_string(),
        collection: "axon-test".to_string(),
        limit: 3,
        source_id: None,
        generation: None,
        namespace_filters: Vec::new(),
        excluded_source_kinds: Vec::new(),
        byte_budget: 4096,
        token_budget: 512,
    }
}

fn empty_ask_request(query: Option<&str>) -> AskRequest {
    AskRequest {
        query: query.map(ToString::to_string),
        ..AskRequest::default()
    }
}

#[tokio::test]
async fn retrieve_returns_seeded_result_and_records_call() {
    let fake = FakeRetrievalEngine::new(sample_result());

    let result = BoundaryRetrievalEngine::retrieve(&fake, sample_request())
        .await
        .expect("fake retrieve succeeds in Success mode");

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].chunk_id, ChunkId::new("chunk-a"));
    assert_eq!(fake.calls().len(), 1);
    assert_eq!(fake.calls()[0].query, "alpha");
}

#[tokio::test]
async fn query_composes_matches_and_citations_from_seeded_result() {
    let fake = FakeRetrievalEngine::new(sample_result());

    let result = fake
        .query(QueryRequest {
            query: "alpha".to_string(),
            collection: "axon-test".to_string(),
            limit: 3,
            namespace_filters: Vec::new(),
        })
        .await
        .expect("fake query succeeds in Success mode");

    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.citations.len(), 1);
    assert_eq!(result.citations[0], sample_citation());
}

#[tokio::test]
async fn build_ask_context_composes_context_bundle_and_citations() {
    let fake = FakeRetrievalEngine::new(sample_result());

    let ask_context = fake
        .build_ask_context(empty_ask_request(Some("alpha")))
        .await
        .expect("fake build_ask_context succeeds in Success mode");

    assert_eq!(ask_context.citations.len(), 1);
    assert!(ask_context.context.text.contains("Alpha chunk body"));
    assert_eq!(ask_context.retrieval.matches.len(), 1);
}

#[tokio::test]
async fn fatal_mode_returns_error_and_still_records_call() {
    let fake = FakeRetrievalEngine::new(sample_result()).with_mode(FakeRetrievalMode::Fatal);

    let err = BoundaryRetrievalEngine::retrieve(&fake, sample_request())
        .await
        .expect_err("Fatal mode must return an error");

    assert_eq!(err.code.0, "retrieval.fake_fatal");
    assert!(!err.retryable);
    assert_eq!(fake.calls().len(), 1, "call is recorded even on failure");
}

#[tokio::test]
async fn degraded_mode_still_succeeds_but_reports_degraded_health() {
    let fake = FakeRetrievalEngine::new(sample_result()).with_mode(FakeRetrievalMode::Degraded);

    let result = BoundaryRetrievalEngine::retrieve(&fake, sample_request())
        .await
        .expect("Degraded mode still returns a result");
    assert_eq!(result.matches.len(), 1);

    let capability = fake
        .capabilities()
        .await
        .expect("capabilities() never errors");
    assert_eq!(capability.0.health, HealthStatus::Degraded);
}

#[tokio::test]
async fn capability_override_replaces_default_capability() {
    let override_capability = RetrievalCapability(CapabilityBase {
        name: "custom".to_string(),
        version: "9.9.9".to_string(),
        owner_crate: "axon-retrieval".to_string(),
        health: HealthStatus::Cooling,
        features: vec!["override".to_string()],
        limits: MetadataMap::new(),
    });
    let fake = FakeRetrievalEngine::new(sample_result())
        .with_capability_override(override_capability.clone());

    let capability = fake
        .capabilities()
        .await
        .expect("capabilities() never errors");

    assert_eq!(capability, override_capability);
}

#[test]
fn fake_retrieval_engine_satisfies_retrieval_engine_trait_object() {
    let fake: Arc<dyn BoundaryRetrievalEngine> =
        Arc::new(FakeRetrievalEngine::new(sample_result()));
    drop(fake);
}

fn sample_publish_plan() -> PublishPlan {
    PublishPlan {
        source_id: SourceId::new("src-docs"),
        generation: SourceGenerationId::from("gen_1"),
        previous_generation: None,
        ready: true,
        estimated_document_count: 3,
        estimated_chunk_count: 12,
        cleanup_debt_preview: Vec::new(),
        warnings: Vec::new(),
    }
}

fn sample_publish_result() -> PublishGenerationResult {
    PublishGenerationResult {
        header: StageResultHeader {
            job_id: Default::default(),
            stage_id: Default::default(),
            phase: axon_api::source::PipelinePhase::Publishing,
            status: axon_api::source::LifecycleStatus::Completed,
            started_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
            completed_at: Some(Timestamp("2026-07-01T00:00:01Z".to_string())),
            counts: StageCounts {
                items_total: None,
                items_done: 0,
                documents_total: None,
                documents_done: 0,
                chunks_total: None,
                chunks_done: 0,
                bytes_total: None,
                bytes_done: 0,
            },
            warnings: Vec::new(),
            error: None,
        },
        source_id: SourceId::new("src-docs"),
        generation: SourceGenerationId::from("gen_1"),
        published_at: Timestamp("2026-07-01T00:00:01Z".to_string()),
        document_count: 3,
        chunk_count: 12,
        vector_point_count: 12,
        cleanup_debt: Vec::new(),
    }
}

fn sample_publish_request() -> PublishGenerationRequest {
    PublishGenerationRequest {
        source_id: SourceId::new("src-docs"),
        generation: SourceGenerationId::from("gen_1"),
        expected_previous_generation: None,
    }
}

#[tokio::test]
async fn fake_generation_publisher_success_mode_records_calls_and_returns_seeded_plan() {
    let fake = FakeGenerationPublisher::new(sample_publish_plan(), sample_publish_result());

    let plan = fake
        .validate_publish(sample_publish_request())
        .await
        .expect("fake validate_publish succeeds in Success mode");

    assert!(plan.ready);
    assert_eq!(fake.calls().len(), 1);

    let result = fake
        .publish_generation(sample_publish_request())
        .await
        .expect("fake publish_generation succeeds in Success mode");
    assert_eq!(result.generation, SourceGenerationId::from("gen_1"));
    assert_eq!(fake.calls().len(), 2);
}

#[tokio::test]
async fn fake_generation_publisher_fatal_mode_returns_error_and_still_records_call() {
    let fake = FakeGenerationPublisher::new(sample_publish_plan(), sample_publish_result())
        .with_mode(FakeGenerationPublisherMode::Fatal);

    let err = fake
        .publish_generation(sample_publish_request())
        .await
        .expect_err("Fatal mode must return an error");

    assert_eq!(err.code.0, "retrieval.publish.fake_fatal");
    assert!(!err.retryable);
    assert_eq!(fake.calls().len(), 1, "call is recorded even on failure");
}

#[test]
fn fake_generation_publisher_satisfies_generation_publisher_trait_object() {
    let fake: Arc<dyn GenerationPublisher> = Arc::new(FakeGenerationPublisher::new(
        sample_publish_plan(),
        sample_publish_result(),
    ));
    drop(fake);
}
