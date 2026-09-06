use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axon_api::source::*;
use axon_core::boundary::{ArtifactBytesWriteRequest, ArtifactStore, FakeCoreBoundaries};
use axon_jobs::scheduler::{ProviderCapacityDomain, ProviderScheduler, SchedulerConfig};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};

use crate::reserved_call::{ArtifactCleanupGuard, drain_artifact_cleanup_workers};
use crate::source::prune::CleanupProviderOps;
use crate::source::result_map::IndexCounts;

fn cleanup_source() -> SourceSummary {
    let now = Timestamp("2026-07-31T00:00:00Z".to_string());
    SourceSummary {
        source_id: SourceId::new("src_cleanup_guard"),
        canonical_uri: "file:///cleanup-guard".to_string(),
        display_name: "cleanup guard".to_string(),
        source_kind: SourceKind::Local,
        adapter: AdapterRef {
            name: "test".to_string(),
            version: "1".to_string(),
        },
        authority: AuthorityLevel::UserPinned,
        status: LifecycleStatus::Running,
        counts: SourceCounts {
            items_total: 0,
            items_changed: 0,
            documents_total: 0,
            chunks_total: 0,
            vector_points_total: 0,
            bytes_total: 0,
        },
        created_at: now.clone(),
        updated_at: now,
        tags: Vec::new(),
        watch_id: None,
        graph_node_ids: Vec::new(),
        last_job_id: None,
        last_refreshed_at: None,
        user_label: None,
    }
}

async fn cleanup_ledger() -> Arc<FakeLedgerStore> {
    cleanup_ledger_from(FakeLedgerStore::new()).await
}

async fn cleanup_ledger_from(store: FakeLedgerStore) -> Arc<FakeLedgerStore> {
    let ledger = Arc::new(store);
    ledger.upsert_source(cleanup_source()).await.unwrap();
    ledger
}

struct FailingDeleteStore {
    inner: Arc<FakeCoreBoundaries>,
}

struct ArtifactRecoveryOps(Arc<FakeCoreBoundaries>);

struct CountingDeleteStore {
    inner: Arc<FakeCoreBoundaries>,
    deletes: Arc<AtomicUsize>,
}

#[async_trait::async_trait]
impl ArtifactStore for CountingDeleteStore {
    async fn put(
        &self,
        artifact: ArtifactWriteRequest,
    ) -> axon_core::boundary::Result<ArtifactHandle> {
        self.inner.put(artifact).await
    }
    async fn get(&self, handle: ArtifactHandle) -> axon_core::boundary::Result<ArtifactReadResult> {
        self.inner.get(handle).await
    }
    async fn delete(&self, handle: ArtifactHandle) -> axon_core::boundary::Result<()> {
        self.deletes.fetch_add(1, Ordering::AcqRel);
        self.inner.delete(handle).await
    }
    async fn reset(&self) -> axon_core::boundary::Result<()> {
        self.inner.reset().await
    }
    async fn capabilities(&self) -> axon_core::boundary::Result<ArtifactStoreCapability> {
        self.inner.capabilities().await
    }
}

#[async_trait::async_trait]
impl CleanupProviderOps for ArtifactRecoveryOps {
    async fn vector_delete(
        &self,
        _selector: VectorDeleteSelector,
    ) -> Result<VectorStoreDeleteResult, ApiError> {
        unreachable!("artifact recovery does not delete vectors")
    }

    async fn graph_delete_nodes(
        &self,
        _stable_keys: Vec<String>,
    ) -> Result<GraphDeleteResult, ApiError> {
        unreachable!("artifact recovery does not delete graph nodes")
    }

    async fn graph_delete_edges(
        &self,
        _edge_ids: Vec<GraphEdgeId>,
    ) -> Result<GraphDeleteResult, ApiError> {
        unreachable!("artifact recovery does not delete graph edges")
    }

    async fn artifact_delete(&self, handle: ArtifactHandle) -> Result<(), ApiError> {
        self.0.delete(handle).await
    }
}

fn cleanup_counts() -> IndexCounts {
    IndexCounts {
        job_id: JobId::new(uuid::Uuid::nil()),
        source_id: SourceId::new("src_cleanup_guard"),
        generation: SourceGenerationId::new("gen_uncommitted"),
        items_discovered: 0,
        documents_prepared: 0,
        chunks_prepared: 0,
        vector_points_written: 0,
        removed: 0,
        published_manifest: None,
        graph_candidates: Vec::new(),
        warnings: Vec::new(),
        artifacts: Vec::new(),
        inline: None,
    }
}

#[async_trait::async_trait]
impl ArtifactStore for FailingDeleteStore {
    async fn put(
        &self,
        artifact: ArtifactWriteRequest,
    ) -> axon_core::boundary::Result<ArtifactHandle> {
        self.inner.put(artifact).await
    }

    async fn put_bytes(
        &self,
        artifact: ArtifactBytesWriteRequest,
    ) -> axon_core::boundary::Result<ArtifactHandle> {
        self.inner.put_bytes(artifact).await
    }

    async fn get(&self, handle: ArtifactHandle) -> axon_core::boundary::Result<ArtifactReadResult> {
        self.inner.get(handle).await
    }

    async fn delete(&self, _handle: ArtifactHandle) -> axon_core::boundary::Result<()> {
        Err(ApiError::new(
            "artifact.delete_failed",
            ErrorStage::Cleaning,
            "injected artifact delete failure",
        ))
    }

    async fn reset(&self) -> axon_core::boundary::Result<()> {
        self.inner.reset().await
    }

    async fn capabilities(&self) -> axon_core::boundary::Result<ArtifactStoreCapability> {
        self.inner.capabilities().await
    }
}

fn export_document(url: &str, key: &str, text: &str) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::new(format!("doc_{key}")),
        source_id: SourceId::new("src_export"),
        source_item_key: SourceItemKey::new(key),
        canonical_uri: url.to_string(),
        content_kind: ContentKind::Markdown,
        content: ContentRef::InlineText { text: text.into() },
        metadata: MetadataMap::new(),
        title: None,
        language: None,
        path: None,
        mime_type: Some("text/markdown".to_string()),
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}

#[tokio::test]
async fn durable_export_is_usable_before_generation_publication() {
    let temp = tempfile::tempdir().expect("tempdir");
    super::initialize_durable_export_dir(temp.path())
        .await
        .expect("initialize checkpoint");
    super::checkpoint_durable_export_dir(
        temp.path(),
        &[export_document(
            "https://example.com/guide",
            "guide",
            "# Durable guide\n",
        )],
    )
    .await
    .expect("checkpoint document");

    let manifest = tokio::fs::read_to_string(temp.path().join("manifest.jsonl"))
        .await
        .expect("manifest exists without publication");
    let entry: serde_json::Value =
        serde_json::from_str(manifest.trim()).expect("valid checkpoint JSONL");
    let relative = entry["relative_path"].as_str().expect("relative path");
    assert_eq!(entry["url"], "https://example.com/guide");
    assert_eq!(
        tokio::fs::read_to_string(temp.path().join(relative))
            .await
            .expect("manifest never precedes content"),
        "# Durable guide\n"
    );
}

#[tokio::test]
async fn initializing_next_generation_discards_stale_manifest_not_content() {
    let temp = tempfile::tempdir().expect("tempdir");
    super::initialize_durable_export_dir(temp.path())
        .await
        .unwrap();
    super::checkpoint_durable_export_dir(
        temp.path(),
        &[export_document("https://example.com/old", "old", "old")],
    )
    .await
    .unwrap();
    let old_manifest: serde_json::Value = serde_json::from_str(
        tokio::fs::read_to_string(temp.path().join("manifest.jsonl"))
            .await
            .unwrap()
            .trim(),
    )
    .unwrap();
    let old_content = temp
        .path()
        .join(old_manifest["relative_path"].as_str().unwrap());

    super::initialize_durable_export_dir(temp.path())
        .await
        .unwrap();

    assert_eq!(
        tokio::fs::read(temp.path().join("manifest.jsonl"))
            .await
            .unwrap(),
        b""
    );
    assert!(
        old_content.exists(),
        "generation reset must not delete content"
    );
}

async fn stored_artifact(core: &FakeCoreBoundaries, suffix: &str) -> ArtifactRef {
    let handle = core
        .put(ArtifactWriteRequest {
            kind: ArtifactKind::NormalizedContent,
            content_type: "text/plain".to_string(),
            content: ContentRef::InlineText {
                text: format!("artifact-{suffix}"),
            },
            source_id: Some(SourceId::new("src_cleanup_guard")),
            job_id: Some(JobId::new(uuid::Uuid::nil())),
            metadata: MetadataMap::new(),
        })
        .await
        .expect("store artifact");
    ArtifactRef {
        artifact_id: handle.artifact_id,
        artifact_kind: handle.artifact_kind,
        uri: handle.uri.unwrap_or_default(),
        size_bytes: None,
        content_hash: None,
        created_at: Timestamp("2026-07-31T00:00:00Z".to_string()),
    }
}

#[tokio::test]
async fn cleanup_guard_removes_artifacts_from_an_uncommitted_generation() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger: Arc<dyn LedgerStore> = cleanup_ledger().await;
    let artifact = stored_artifact(core.as_ref(), "uncommitted").await;
    let mut guard = ArtifactCleanupGuard::new_for_test(
        core.clone(),
        ledger.clone(),
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();
    guard.finish().await.unwrap();

    assert!(
        core.get(ArtifactHandle {
            artifact_id: artifact.artifact_id.clone(),
            artifact_kind: artifact.artifact_kind,
            uri: Some(artifact.uri.clone()),
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn track_surfaces_journal_failure_before_guard_takes_drop_ownership() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger: Arc<dyn LedgerStore> = cleanup_ledger().await;
    let artifact = stored_artifact(core.as_ref(), "track-persist-failure").await;
    let mut guard = ArtifactCleanupGuard::new_for_test(
        core.clone(),
        ledger,
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.inject_next_track_persist_failure_for_test(vec![artifact.clone()]);

    let error = guard
        .track(std::slice::from_ref(&artifact))
        .await
        .expect_err("journal failure must stop artifact ownership transfer");

    assert_eq!(error.code.0, "artifact.cleanup_journal_failed");
    assert_eq!(guard.tracked_artifact_count_for_test(), 0);
    drop(guard);
    assert_eq!(drain_artifact_cleanup_workers(), 0);
    assert!(
        core.get(ArtifactHandle {
            artifact_id: artifact.artifact_id,
            artifact_kind: artifact.artifact_kind,
            uri: Some(artifact.uri),
        })
        .await
        .is_err(),
        "failed journal admission synchronously rolls back the artifact"
    );
}

#[tokio::test]
async fn cancellation_after_disarm_remove_never_deletes_published_artifact() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger: Arc<dyn LedgerStore> = cleanup_ledger().await;
    let artifact = stored_artifact(core.as_ref(), "disarm-cancel").await;
    let mut guard = ArtifactCleanupGuard::new_for_test(
        core.clone(),
        ledger,
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();

    let join = tokio::spawn(async move {
        guard.disarm_then_panic_after_remove_for_test().await;
    })
    .await;
    assert!(join.is_err(), "injected cancellation unwinds the guard");
    assert_eq!(drain_artifact_cleanup_workers(), 0);
    core.get(ArtifactHandle {
        artifact_id: artifact.artifact_id,
        artifact_kind: artifact.artifact_kind,
        uri: Some(artifact.uri),
    })
    .await
    .expect("published artifact survives cancellation after journal removal");
}

#[test]
fn cleanup_guard_drop_without_a_tokio_runtime_uses_drained_fallback() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger: Arc<dyn LedgerStore> = runtime.block_on(cleanup_ledger());
    let artifact = runtime.block_on(stored_artifact(core.as_ref(), "fallback"));
    let mut guard = ArtifactCleanupGuard::new_for_test(
        core.clone(),
        ledger.clone(),
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    runtime
        .block_on(guard.track(std::slice::from_ref(&artifact)))
        .unwrap();
    drop(runtime);

    drop(guard);
    assert_eq!(drain_artifact_cleanup_workers(), 0);

    let verify = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(
        verify
            .block_on(core.get(ArtifactHandle {
                artifact_id: artifact.artifact_id,
                artifact_kind: artifact.artifact_kind,
                uri: Some(artifact.uri),
            }))
            .is_err()
    );
}

#[tokio::test]
async fn disarmed_cleanup_guard_preserves_published_artifacts() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger: Arc<dyn LedgerStore> = cleanup_ledger().await;
    let artifact = stored_artifact(core.as_ref(), "published").await;
    {
        let mut guard = ArtifactCleanupGuard::new_for_test(
            core.clone(),
            ledger,
            SourceId::new("src_cleanup_guard"),
            SourceGenerationId::new("gen_published"),
        );
        guard.track(std::slice::from_ref(&artifact)).await.unwrap();
        guard.disarm().await.unwrap();
    }
    core.get(ArtifactHandle {
        artifact_id: artifact.artifact_id.clone(),
        artifact_kind: artifact.artifact_kind,
        uri: Some(artifact.uri),
    })
    .await
    .expect("disarmed guard preserves artifact");
}

#[tokio::test]
async fn cleanup_guard_keeps_durable_debt_when_artifact_delete_fails() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger = cleanup_ledger().await;
    let artifact = stored_artifact(core.as_ref(), "delete-failure").await;
    let store: Arc<dyn ArtifactStore> = Arc::new(FailingDeleteStore {
        inner: core.clone(),
    });
    let mut guard = ArtifactCleanupGuard::new_for_test(
        store,
        ledger.clone(),
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();

    guard.finish().await.unwrap();

    core.get(ArtifactHandle {
        artifact_id: artifact.artifact_id,
        artifact_kind: artifact.artifact_kind,
        uri: Some(artifact.uri),
    })
    .await
    .expect("failed delete preserves artifact for retry");
    let debt = ledger
        .list_pending_cleanup_debt(SourceId::new("src_cleanup_guard"))
        .await
        .unwrap();
    assert_eq!(debt.len(), 1);
    assert_eq!(debt[0].kind, CleanupDebtKind::ArtifactDelete);
    assert_eq!(debt[0].attempts, 1);
    assert_eq!(
        debt[0].last_error.as_ref().map(|error| error.code.as_str()),
        Some("artifact.delete_failed")
    );
}

#[tokio::test]
async fn cleanup_guard_surfaces_confirmation_failure_and_retains_retry_ownership() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger =
        cleanup_ledger_from(FakeLedgerStore::new().with_committed_generation_failure()).await;
    let artifact = stored_artifact(core.as_ref(), "confirmation-failure").await;
    let mut guard = ArtifactCleanupGuard::new_for_test(
        core.clone(),
        ledger.clone(),
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();

    let error = guard
        .finish()
        .await
        .expect_err("confirmation failure is surfaced");

    assert_eq!(error.code.0, "ledger.committed_generation_failed");
    assert_eq!(guard.tracked_artifact_count_for_test(), 0);
    core.get(ArtifactHandle {
        artifact_id: artifact.artifact_id,
        artifact_kind: artifact.artifact_kind,
        uri: Some(artifact.uri),
    })
    .await
    .expect("unconfirmed artifact is preserved for retry");
    ledger.clear_injected_failure();
    assert_eq!(drain_artifact_cleanup_workers(), 0);
    guard.disarm().await.unwrap();
}

#[tokio::test]
async fn cleanup_guard_surfaces_debt_write_failure_and_retains_retry_ownership() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger =
        cleanup_ledger_from(FakeLedgerStore::new().with_cleanup_debt_write_failure()).await;
    let artifact = stored_artifact(core.as_ref(), "debt-write-failure").await;
    let mut guard = ArtifactCleanupGuard::new_for_test(
        core.clone(),
        ledger.clone(),
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();

    let error = guard
        .finish()
        .await
        .expect_err("debt write failure is surfaced");

    assert_eq!(error.code.0, "ledger.cleanup_debt_write_failed");
    assert_eq!(guard.tracked_artifact_count_for_test(), 0);
    core.get(ArtifactHandle {
        artifact_id: artifact.artifact_id,
        artifact_kind: artifact.artifact_kind,
        uri: Some(artifact.uri),
    })
    .await
    .expect("unjournaled artifact is preserved for retry");
    ledger.clear_injected_failure();
    assert_eq!(drain_artifact_cleanup_workers(), 0);
    guard.disarm().await.unwrap();
}

#[tokio::test]
async fn failed_finish_transfers_once_and_shutdown_reports_then_recovers_unresolved_work() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger =
        cleanup_ledger_from(FakeLedgerStore::new().with_cleanup_debt_write_failure()).await;
    let artifact = stored_artifact(core.as_ref(), "tracked-retry").await;
    let mut guard = ArtifactCleanupGuard::new_for_test(
        core.clone(),
        ledger.clone(),
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();

    guard.finish().await.expect_err("first handoff fails");
    assert_eq!(guard.tracked_artifact_count_for_test(), 0);
    drop(guard);
    assert_eq!(drain_artifact_cleanup_workers(), 1);

    ledger.clear_injected_failure();
    assert_eq!(drain_artifact_cleanup_workers(), 0);
    assert!(
        core.get(ArtifactHandle {
            artifact_id: artifact.artifact_id,
            artifact_kind: artifact.artifact_kind,
            uri: Some(artifact.uri),
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn partial_cleanup_retries_only_the_unresolved_suffix() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let deletes = Arc::new(AtomicUsize::new(0));
    let store: Arc<dyn ArtifactStore> = Arc::new(CountingDeleteStore {
        inner: core.clone(),
        deletes: deletes.clone(),
    });
    let ledger =
        cleanup_ledger_from(FakeLedgerStore::new().with_cleanup_debt_failure_after(1)).await;
    let artifacts = vec![
        stored_artifact(core.as_ref(), "partial-a").await,
        stored_artifact(core.as_ref(), "partial-b").await,
        stored_artifact(core.as_ref(), "partial-c").await,
    ];
    let mut guard = ArtifactCleanupGuard::new_for_test(
        store,
        ledger,
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(&artifacts).await.unwrap();

    guard
        .finish()
        .await
        .expect_err("second debt write fails once");
    drop(guard);
    assert_eq!(drain_artifact_cleanup_workers(), 0);
    assert_eq!(deletes.load(Ordering::Acquire), 3);
}

#[tokio::test]
async fn artifact_delete_debt_is_recovered_by_the_autonomous_drain_path() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger = cleanup_ledger().await;
    let artifact = stored_artifact(core.as_ref(), "restart-drain").await;
    let store: Arc<dyn ArtifactStore> = Arc::new(FailingDeleteStore {
        inner: core.clone(),
    });
    let mut guard = ArtifactCleanupGuard::new_for_test(
        store,
        ledger.clone(),
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();
    guard.finish().await.unwrap();

    let summary = crate::source::prune::drain_cleanup_debt_with_provider_ops(
        ledger.as_ref(),
        &ArtifactRecoveryOps(core.clone()),
        None,
        None,
        None,
        None,
        "unused-for-artifacts",
        &cleanup_counts(),
    )
    .await;

    assert_eq!(summary.resolved, 1);
    assert_eq!(summary.failed, 0);
    assert!(
        ledger
            .list_pending_cleanup_debt(SourceId::new("src_cleanup_guard"))
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        core.get(ArtifactHandle {
            artifact_id: artifact.artifact_id,
            artifact_kind: artifact.artifact_kind,
            uri: Some(artifact.uri),
        })
        .await
        .is_err()
    );
}

#[tokio::test]
async fn cleanup_guard_artifact_delete_participates_in_scheduler() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger = cleanup_ledger().await;
    let artifact = stored_artifact(core.as_ref(), "scheduled").await;
    let database = tempfile::NamedTempFile::new().unwrap();
    let pool = axon_jobs::store::open_sqlite_pool(&database.path().to_string_lossy())
        .await
        .expect("migrated scheduler database");
    sqlx::query(
        "INSERT INTO jobs (job_id, kind, status, phase, priority, created_at, updated_at) \
         VALUES (?, 'source', 'running', 'cleaning', 'background', datetime('now'), datetime('now'))",
    )
    .bind(uuid::Uuid::nil().to_string())
    .execute(&pool)
    .await
    .expect("seed scheduler job");
    sqlx::query(
        "INSERT INTO job_stages (stage_id, job_id, phase, status, required) \
         VALUES (?, ?, 'cleaning', 'running', 1)",
    )
    .bind(
        StageId::for_job_stage(JobId::new(uuid::Uuid::nil()), "cleaning", 0)
            .0
            .to_string(),
    )
    .bind(uuid::Uuid::nil().to_string())
    .execute(&pool)
    .await
    .expect("seed scheduler stage");
    let scheduler = Arc::new(
        ProviderScheduler::new(
            pool.clone(),
            ProviderCapacityDomain {
                kind: ProviderKind::Artifact,
                instance_id: "artifact-test".to_string(),
                authority_id: "artifact-test-authority".to_string(),
            },
            SchedulerConfig::new(1, 0, 4, 4).expect("valid artifact capacity"),
        )
        .unwrap(),
    );
    let mut guard = ArtifactCleanupGuard::new_for_test_with_scheduler(
        core,
        ledger,
        scheduler,
        SourceId::new("src_cleanup_guard"),
        SourceGenerationId::new("gen_uncommitted"),
    );
    guard.track(std::slice::from_ref(&artifact)).await.unwrap();

    guard.finish().await.unwrap();

    let reservations: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM provider_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        reservations, 1,
        "artifact deletion must reserve its provider lane"
    );
}
