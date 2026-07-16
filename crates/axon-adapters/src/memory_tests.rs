use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;

use crate::SourceAdapter;
use crate::memory::{
    MemorySourceAccess, MemorySourceAdapter, MemorySourceProvider, memory_id_from_uri,
};

struct Provider(Option<MemoryRecord>);

#[async_trait]
impl MemorySourceProvider for Provider {
    async fn get(&self, _memory_id: MemoryId) -> crate::adapter::Result<Option<MemoryRecord>> {
        Ok(self.0.clone())
    }
}

#[test]
fn memory_uri_requires_one_canonical_memory_id() {
    assert_eq!(memory_id_from_uri("memory://mem_abc").unwrap().0, "mem_abc");
    for invalid in [
        "memory://",
        "memory://abc",
        "memory://mem_a/child",
        "memory://mem_a?x=1",
        "memory://mem_a#fragment",
    ] {
        assert!(memory_id_from_uri(invalid).is_err(), "accepted {invalid}");
    }
}

#[tokio::test]
async fn materialization_fails_closed_for_missing_and_unauthorized_memory() {
    let plan = plan();
    let missing = MemorySourceAdapter::new(
        Arc::new(Provider(None)),
        MemorySourceAccess {
            visibility_ceiling: Visibility::Internal,
            allow_sensitive: false,
        },
    );
    assert_eq!(
        missing.materialize(plan.clone()).await.unwrap_err().code,
        "adapter.memory.not_found"
    );

    let sensitive = MemorySourceAdapter::new(
        Arc::new(Provider(Some(record(Visibility::Sensitive)))),
        MemorySourceAccess {
            visibility_ceiling: Visibility::Internal,
            allow_sensitive: false,
        },
    );
    assert_eq!(
        sensitive.materialize(plan).await.unwrap_err().code,
        "adapter.memory.visibility_denied"
    );
}

#[tokio::test]
async fn adapter_projects_one_record_through_discover_acquire_normalize() {
    let plan = plan();
    let adapter = MemorySourceAdapter::new(
        Arc::new(Provider(Some(record(Visibility::Internal)))),
        MemorySourceAccess {
            visibility_ceiling: Visibility::Internal,
            allow_sensitive: false,
        },
    );
    let materialized = adapter.materialize(plan.clone()).await.unwrap();
    let manifest = adapter.discover(&materialized.plan).await.unwrap();
    assert_eq!(manifest.items.len(), 1);
    assert_eq!(manifest.items[0].item_kind, ItemKind::MemoryRecord);
    assert_eq!(manifest.items[0].canonical_uri, "memory://mem_abc");

    let diff = SourceManifestDiff {
        header: header(plan.job_id),
        source_id: plan.route.source.source_id.clone(),
        previous_generation: None,
        next_generation: SourceGenerationId::new("gen_1"),
        added: manifest.items.clone(),
        modified: Vec::new(),
        removed: Vec::new(),
        unchanged: Vec::new(),
        skipped: Vec::new(),
        failed: Vec::new(),
        counts: DiffCounts {
            added: 1,
            modified: 0,
            removed: 0,
            unchanged: 0,
            skipped: 0,
            failed: 0,
        },
    };
    let acquisition = adapter.acquire(&materialized.plan, &diff).await.unwrap();
    let normalized = adapter
        .normalize(&materialized.plan, acquisition)
        .await
        .unwrap();
    assert_eq!(normalized.data.len(), 1);
    let document = &normalized.data[0];
    assert_eq!(
        document.content,
        ContentRef::InlineText {
            text: "remember this".to_string()
        }
    );
    assert_eq!(
        document.metadata.get("memory_status"),
        Some(&serde_json::json!("active"))
    );
    assert!(
        document
            .metadata
            .contains_key(axon_parse::vertical::VERTICAL_GRAPH_CANDIDATES_METADATA_KEY)
    );
}

#[tokio::test]
async fn terminal_memory_discovers_empty_manifest_for_ledger_cleanup() {
    let plan = plan();
    let mut archived = record(Visibility::Internal);
    archived.status = MemoryStatus::Archived;
    let adapter = MemorySourceAdapter::new(
        Arc::new(Provider(Some(archived))),
        MemorySourceAccess {
            visibility_ceiling: Visibility::Internal,
            allow_sensitive: false,
        },
    );

    let materialized = adapter.materialize(plan).await.unwrap();
    let manifest = adapter.discover(&materialized.plan).await.unwrap();
    assert!(manifest.items.is_empty());
    assert_eq!(
        manifest.metadata.get("memory_status"),
        Some(&serde_json::json!("archived"))
    );
}

fn plan() -> SourcePlan {
    let job_id = JobId::new(uuid::Uuid::nil());
    let source = ResolvedSource {
        source: "memory://mem_abc".to_string(),
        canonical_uri: "memory://mem_abc".to_string(),
        source_id: SourceId::new("src_memory_abc"),
        source_kind: SourceKind::Memory,
        adapter: AdapterRef {
            name: "memory".to_string(),
            version: "1".to_string(),
        },
        default_scope: SourceScope::Api,
        available_scopes: vec![SourceScope::Api],
        authority: AuthorityLevel::UserPinned,
        confidence: 1.0,
        reason: "test".to_string(),
        graph: Vec::new(),
        warnings: Vec::new(),
        metadata: MetadataMap::new(),
    };
    let route = RoutePlan {
        source,
        adapter: AdapterRef {
            name: "memory".to_string(),
            version: "1".to_string(),
        },
        scope: SourceScope::Api,
        safety_class: SafetyClass::AuthenticatedNetwork,
        execution_affinity: ExecutionAffinity::Worker,
        provider_requirements: Vec::new(),
        credential_requirements: Vec::new(),
        option_schema_id: "adapter:memory:options:v1".to_string(),
        validated_options: AdapterOptions::default(),
        chunking_hints: vec![ChunkHint {
            profile: ChunkProfile::AtomicMetadata,
            reason: "memory".to_string(),
            options: MetadataMap::new(),
        }],
        parser_hints: Vec::new(),
        graph_fact_kinds: vec!["memory".to_string()],
        watch_supported: false,
        refresh_supported: true,
    };
    SourcePlan {
        job_id,
        request: SourceRequest::new("memory://mem_abc"),
        route,
        stage_plan: Vec::new(),
        limits: EffectiveLimits {
            request: SourceLimits::default(),
            adapter_defaults: SourceLimits::default(),
            config_defaults: SourceLimits::default(),
            effective: SourceLimits::default(),
        },
        config_snapshot_id: ConfigSnapshotId::new("cfg"),
        provider_reservations: Vec::new(),
    }
}

fn record(visibility: Visibility) -> MemoryRecord {
    MemoryRecord {
        memory_id: MemoryId::new("mem_abc"),
        memory_type: MemoryType::Fact,
        status: MemoryStatus::Active,
        body: "remember this".to_string(),
        confidence: 0.9,
        salience: 0.8,
        scope: MemoryScope {
            kind: "project".to_string(),
            value: "axon".to_string(),
        },
        history: vec![MemoryHistoryEvent {
            status: MemoryStatus::Active,
            message: "created".to_string(),
            timestamp: Timestamp::from(chrono::Utc::now()),
        }],
        visibility,
        title: Some("Memory".to_string()),
        links: Vec::new(),
        decay: None,
        embedding_refs: Vec::new(),
        superseded_by: None,
        contradicts: None,
    }
}

fn header(job_id: JobId) -> StageResultHeader {
    StageResultHeader {
        job_id,
        stage_id: StageId::new(uuid::Uuid::nil()),
        phase: PipelinePhase::Diffing,
        status: LifecycleStatus::Completed,
        started_at: Timestamp::from(chrono::Utc::now()),
        completed_at: Some(Timestamp::from(chrono::Utc::now())),
        counts: StageCounts {
            items_total: Some(1),
            items_done: 1,
            documents_total: Some(1),
            documents_done: 1,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}
