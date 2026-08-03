use std::sync::Arc;
use std::time::Duration;

use axon_api::source::*;
use axon_core::boundary::{ArtifactStore, FakeCoreBoundaries};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};

use crate::reserved_call::ArtifactCleanupGuard;

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
    let ledger: Arc<dyn LedgerStore> = Arc::new(FakeLedgerStore::new());
    let artifact = stored_artifact(core.as_ref(), "uncommitted").await;
    {
        let mut guard = ArtifactCleanupGuard::new_for_test(
            core.clone(),
            ledger,
            SourceId::new("src_cleanup_guard"),
            SourceGenerationId::new("gen_uncommitted"),
        );
        guard.track(std::slice::from_ref(&artifact));
    }

    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            let result = core
                .get(ArtifactHandle {
                    artifact_id: artifact.artifact_id.clone(),
                    artifact_kind: artifact.artifact_kind,
                    uri: Some(artifact.uri.clone()),
                })
                .await;
            if result.is_err() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("cleanup guard deleted artifact");
}

#[tokio::test]
async fn disarmed_cleanup_guard_preserves_published_artifacts() {
    let core = Arc::new(FakeCoreBoundaries::new());
    let ledger: Arc<dyn LedgerStore> = Arc::new(FakeLedgerStore::new());
    let artifact = stored_artifact(core.as_ref(), "published").await;
    {
        let mut guard = ArtifactCleanupGuard::new_for_test(
            core.clone(),
            ledger,
            SourceId::new("src_cleanup_guard"),
            SourceGenerationId::new("gen_published"),
        );
        guard.track(std::slice::from_ref(&artifact));
        guard.disarm();
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    core.get(ArtifactHandle {
        artifact_id: artifact.artifact_id.clone(),
        artifact_kind: artifact.artifact_kind,
        uri: Some(artifact.uri),
    })
    .await
    .expect("disarmed guard preserves artifact");
}
