use std::fs;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;

use crate::SourceAdapter;
use crate::local_test_support::*;
use crate::upload::{UploadSourceAdapter, UploadSourceProvider, upload_source_identity_from_uri};

struct StagedProvider(Option<ArtifactReadResult>);

#[async_trait]
impl UploadSourceProvider for StagedProvider {
    async fn get(
        &self,
        _source_identity: &str,
    ) -> crate::adapter::Result<Option<ArtifactReadResult>> {
        Ok(self.0.clone())
    }
}

fn upload_plan(path: std::path::PathBuf, scope: SourceScope) -> SourcePlan {
    source_plan_for("upload", SourceKind::Upload, "upload", path, scope)
}

#[tokio::test]
async fn upload_adapter_declares_file_directory_map_scopes() {
    let adapter = UploadSourceAdapter::new();
    let capability = adapter.capabilities().await.unwrap();
    assert_eq!(capability.0.name, "upload");
    assert_eq!(
        capability.0.limits.0.get("source_kind"),
        Some(&serde_json::json!(SourceKind::Upload))
    );
    for scope in [SourceScope::File, SourceScope::Directory, SourceScope::Map] {
        let tag = format!(
            "scope:{}",
            serde_json::to_value(scope).unwrap().as_str().unwrap()
        );
        assert!(
            capability.0.features.contains(&tag),
            "missing scope {scope:?}"
        );
    }
}

#[tokio::test]
async fn upload_single_file_round_trips_to_a_source_document() {
    let adapter = UploadSourceAdapter::new();
    let root = temp_source_dir();
    let file_path = root.join("staged.md");
    fs::write(&file_path, "# staged upload").unwrap();

    let plan = upload_plan(file_path, SourceScope::File);
    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 1);
    assert_eq!(
        manifest.items[0].source_item_key,
        SourceItemKey::from("staged.md")
    );

    let diff = manifest_diff(&plan, manifest.items.clone());
    let acquisition = adapter.acquire(&plan, &diff).await.unwrap();
    assert_eq!(acquisition.fetched_items.len(), 1);

    let staged = adapter.normalize(&plan, acquisition).await.unwrap();
    assert_eq!(staged.data.len(), 1);
    let doc = &staged.data[0];
    assert_eq!(
        doc.metadata.0.get("source_kind"),
        Some(&serde_json::json!("upload"))
    );
    assert_eq!(
        doc.metadata.0.get("staged_upload"),
        Some(&serde_json::json!(true))
    );
    match &doc.content {
        ContentRef::InlineText { text } => assert_eq!(text, "# staged upload"),
        other => panic!("expected inline text content, got {other:?}"),
    }
}

#[tokio::test]
async fn upload_directory_scope_walks_unpacked_archive_contents() {
    let adapter = UploadSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("a.txt"), "one").unwrap();
    fs::create_dir_all(root.join("nested")).unwrap();
    fs::write(root.join("nested/b.txt"), "two").unwrap();

    let plan = upload_plan(root, SourceScope::Directory);
    let manifest = adapter.discover(&plan).await.unwrap();
    let keys: Vec<_> = manifest
        .items
        .iter()
        .map(|item| item.source_item_key.0.clone())
        .collect();
    assert_eq!(keys, vec!["a.txt".to_string(), "nested/b.txt".to_string()]);
    assert_eq!(
        manifest.items[0].metadata.0.get("upload_kind"),
        Some(&serde_json::json!("archive"))
    );
}

#[tokio::test]
async fn upload_map_scope_discovers_without_requiring_acquire() {
    let adapter = UploadSourceAdapter::new();
    let root = temp_source_dir();
    fs::write(root.join("only.txt"), "content").unwrap();

    let plan = upload_plan(root, SourceScope::Map);
    let manifest = adapter.discover(&plan).await.unwrap();
    assert_eq!(manifest.items.len(), 1);
}

#[tokio::test]
async fn upload_adapter_rejects_mismatched_route_adapter() {
    let adapter = UploadSourceAdapter::new();
    let root = temp_source_dir();
    let mut plan = upload_plan(root, SourceScope::Directory);
    plan.route.adapter.name = "local".to_string();

    let err = adapter.discover(&plan).await.unwrap_err();
    assert_eq!(err.code.0, "adapter.upload.mismatch");
}

#[test]
fn upload_identity_is_strict_and_canonical() {
    assert_eq!(
        upload_source_identity_from_uri("upload://upl_abc").unwrap(),
        "upl_abc"
    );
    assert_eq!(
        upload_source_identity_from_uri("artifact://art_abc").unwrap(),
        "art_abc"
    );
    for invalid in ["upload://", "upload://relative", "upload://upl_a/child"] {
        assert!(
            upload_source_identity_from_uri(invalid).is_err(),
            "accepted {invalid}"
        );
    }
}

#[tokio::test]
async fn upload_materialization_resolves_staged_content_without_path_trust() {
    let adapter = UploadSourceAdapter::new();
    let mut plan = upload_plan(std::path::PathBuf::from("ignored"), SourceScope::File);
    plan.request.source = "upload:upl_abc".to_string();
    plan.route.source.canonical_uri = "upload://upl_abc".to_string();
    let mut metadata = MetadataMap::new();
    metadata.insert("filename".to_string(), serde_json::json!("notes.md"));
    let staged = ArtifactReadResult {
        handle: ArtifactHandle {
            artifact_id: ArtifactId::new("art_raw_abc"),
            artifact_kind: ArtifactKind::RawContent,
            uri: None,
        },
        content_type: "text/markdown".to_string(),
        content: Some(ContentRef::InlineText {
            text: "# staged".to_string(),
        }),
        metadata,
    };
    let materialized = adapter
        .materialize(plan, Arc::new(StagedProvider(Some(staged))))
        .await
        .unwrap();
    assert_eq!(materialized.path().file_name().unwrap(), "notes.md");
    assert_eq!(fs::read_to_string(materialized.path()).unwrap(), "# staged");
}

#[tokio::test]
async fn upload_materialization_fails_closed_when_staged_content_is_missing() {
    let adapter = UploadSourceAdapter::new();
    let mut plan = upload_plan(std::path::PathBuf::from("ignored"), SourceScope::File);
    plan.route.source.canonical_uri = "upload://upl_missing".to_string();
    let error = adapter
        .materialize(plan, Arc::new(StagedProvider(None)))
        .await
        .unwrap_err();
    assert_eq!(error.code.0, "adapter.upload.not_found");
}
