//! Phase-1 stage-result fixtures: a success, degraded, and failed variant for
//! every stage-result DTO, each round-tripped through serde.
//!
//! The failed variant of each stage result carries an error on its
//! `StageResultHeader` (a `SourceError`), and every failed fixture is paired
//! with a shared `axon_error::ApiError` value that is asserted to round-trip —
//! proving the shared error shape embeds cleanly alongside the stage results.

use chrono::Utc;

use super::*;

fn job_id() -> JobId {
    JobId(uuid::Uuid::nil())
}

fn stage_id() -> StageId {
    StageId(uuid::Uuid::nil())
}

fn now() -> Timestamp {
    Timestamp::from(Utc::now())
}

fn counts() -> StageCounts {
    StageCounts {
        items_total: Some(2),
        items_done: 2,
        documents_total: Some(2),
        documents_done: 2,
        chunks_total: Some(8),
        chunks_done: 8,
        bytes_total: Some(100),
        bytes_done: 100,
    }
}

fn source_error() -> SourceError {
    SourceError {
        code: "source.acquire.fetch_failed".to_string(),
        severity: Severity::Failed,
        message: "fetch failed".to_string(),
        source_item_key: Some(SourceItemKey::from("src/lib.rs")),
        retryable: true,
        provider_id: Some(ProviderId::from("tei")),
        cause: None,
    }
}

/// A `StageResultHeader` at the given phase/status, with an error when failed.
fn header(phase: PipelinePhase, status: LifecycleStatus, failed: bool) -> StageResultHeader {
    StageResultHeader {
        job_id: job_id(),
        stage_id: stage_id(),
        phase,
        status,
        started_at: now(),
        completed_at: Some(now()),
        counts: counts(),
        warnings: Vec::new(),
        error: if failed { Some(source_error()) } else { None },
    }
}

/// The three lifecycle variants exercised by every fixture.
fn variants(phase: PipelinePhase) -> [(StageResultHeader, bool); 3] {
    [
        (header(phase, LifecycleStatus::Completed, false), false),
        (
            header(phase, LifecycleStatus::CompletedDegraded, false),
            false,
        ),
        (header(phase, LifecycleStatus::Failed, true), true),
    ]
}

/// Assert a stage-result value round-trips through serde.
fn round_trip<T>(value: &T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_value(value).expect("serialize");
    let back: T = serde_json::from_value(json).expect("deserialize");
    assert_eq!(&back, value);
}

/// The shared error shape paired with every failed stage fixture.
fn shared_api_error() -> ApiError {
    ApiError::new(
        "source.acquire.fetch_failed",
        ErrorStage::Fetching,
        "fetch failed",
    )
    .with_source_id("src_local")
    .with_job_id("job_1")
}

fn adapter() -> AdapterRef {
    AdapterRef {
        name: "local".to_string(),
        version: "0.1.0".to_string(),
    }
}

fn manifest() -> SourceManifest {
    SourceManifest {
        source_id: SourceId::from("src_local"),
        generation: SourceGenerationId::from("gen_1"),
        adapter: adapter(),
        scope: SourceScope::Directory,
        items: Vec::new(),
        created_at: now(),
        metadata: MetadataMap::default(),
    }
}

#[test]
fn failed_stage_fixtures_pair_with_shared_api_error() {
    // The shared ApiError embedded alongside a failed stage result round-trips.
    round_trip(&shared_api_error());
}

#[test]
fn stage_execution_result_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Fetching) {
        let value = StageExecutionResult {
            header,
            data: counts(),
        };
        round_trip(&value);
    }
}

#[test]
fn authorization_result_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Authorizing) {
        let value = AuthorizationResult {
            header,
            source_id: Some(SourceId::from("src_local")),
            decision: SecurityDecision {
                allowed: true,
                scope: "docs".to_string(),
                reason: "policy allows".to_string(),
                redactions: Vec::new(),
                warnings: Vec::new(),
            },
            caller: CallerContext {
                actor: Some("cli".to_string()),
                transport: TransportKind::Cli,
                scopes: vec!["axon:read".to_string()],
                visibility_ceiling: Visibility::Internal,
            },
        };
        round_trip(&value);
    }
}

#[test]
fn lease_result_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Leasing) {
        let value = LeaseResult {
            header,
            lease_key: "lease_1".to_string(),
            acquired: true,
            owner: "worker_1".to_string(),
            expires_at: now(),
        };
        round_trip(&value);
    }
}

#[test]
fn source_acquisition_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Fetching) {
        let value = SourceAcquisition {
            header,
            source_id: SourceId::from("src_local"),
            generation: SourceGenerationId::from("gen_1"),
            adapter: adapter(),
            scope: SourceScope::Directory,
            manifest: manifest(),
            fetched_items: Vec::new(),
            artifacts: Vec::new(),
        };
        round_trip(&value);
    }
}

#[test]
fn source_manifest_diff_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Diffing) {
        let value = SourceManifestDiff {
            header,
            source_id: SourceId::from("src_local"),
            previous_generation: Some(SourceGenerationId::from("gen_0")),
            next_generation: SourceGenerationId::from("gen_1"),
            added: Vec::new(),
            modified: Vec::new(),
            removed: Vec::new(),
            unchanged: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
            counts: DiffCounts {
                added: 0,
                modified: 0,
                removed: 0,
                unchanged: 0,
                skipped: 0,
                failed: 0,
            },
        };
        round_trip(&value);
    }
}

#[test]
fn source_enrichment_fixtures_round_trip() {
    let statuses = [
        (LifecycleStatus::Completed, EnrichmentStatus::Completed),
        (
            LifecycleStatus::CompletedDegraded,
            EnrichmentStatus::Degraded,
        ),
        (LifecycleStatus::Failed, EnrichmentStatus::Failed),
    ];
    for (idx, (header, _failed)) in variants(PipelinePhase::Enriching).into_iter().enumerate() {
        let value = SourceEnrichment {
            header,
            source_id: SourceId::from("src_local"),
            source_item_key: SourceItemKey::from("src/lib.rs"),
            enrichment_kind: EnrichmentKind::Metadata,
            status: statuses[idx].1,
            metadata: MetadataMap::default(),
            parse_hints: Vec::new(),
            chunk_hints: Vec::new(),
            graph_candidates: Vec::new(),
            artifacts: Vec::new(),
        };
        round_trip(&value);
    }
}

#[test]
fn parse_result_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Parsing) {
        let value = ParseResult {
            header,
            document_id: DocumentId::from("doc_1"),
            facts: Vec::new(),
            graph_candidates: Vec::new(),
            parser_id: "markdown".to_string(),
            parser_version: "1.0.0".to_string(),
        };
        round_trip(&value);
    }
}

#[test]
fn graph_write_result_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Graphing) {
        let value = GraphWriteResult {
            header,
            source_id: SourceId::from("src_local"),
            candidates_seen: 4,
            nodes_upserted: 3,
            edges_upserted: 2,
            evidence_records: 5,
            warnings: Vec::new(),
        };
        round_trip(&value);
    }
}

#[test]
fn vector_store_write_result_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Upserting) {
        let value = VectorStoreWriteResult {
            header,
            collection: "axon".to_string(),
            points_attempted: 8,
            points_written: 8,
            payload_indexes_created: vec!["seed_url".to_string()],
            usage: ProviderUsage {
                input_tokens: None,
                output_tokens: None,
                requests: 1,
                duration_ms: 42,
            },
        };
        round_trip(&value);
    }
}

#[test]
fn publish_generation_result_fixtures_round_trip() {
    for (header, _failed) in variants(PipelinePhase::Publishing) {
        let value = PublishGenerationResult {
            header,
            source_id: SourceId::from("src_local"),
            generation: SourceGenerationId::from("gen_1"),
            published_at: now(),
            document_count: 2,
            chunk_count: 8,
            vector_point_count: 8,
            cleanup_debt: Vec::new(),
        };
        round_trip(&value);
    }
}

#[test]
fn cleanup_debt_result_fixtures_round_trip() {
    let statuses = [
        LifecycleStatus::Completed,
        LifecycleStatus::CompletedDegraded,
        LifecycleStatus::Failed,
    ];
    for (idx, (header, _failed)) in variants(PipelinePhase::Cleaning).into_iter().enumerate() {
        let value = CleanupDebtResult {
            header,
            debt_id: CleanupDebtId::from("debt_1"),
            kind: CleanupDebtKind::VectorDelete,
            status: statuses[idx],
            items_attempted: 3,
            items_cleaned: if idx == 2 { 1 } else { 3 },
            next_retry_at: if idx == 2 { Some(now()) } else { None },
        };
        round_trip(&value);
    }
}
