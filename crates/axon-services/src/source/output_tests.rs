use std::sync::Arc;
use std::time::Duration;

use axon_api::source::*;
use axon_core::boundary::{ArtifactStore, FakeCoreBoundaries};
use axon_ledger::store::{FakeLedgerStore, LedgerStore};

use crate::reserved_call::ArtifactCleanupGuard;

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
