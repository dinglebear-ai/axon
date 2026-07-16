use super::*;
use crate::test_support::NoopServiceRuntime;
use std::sync::Arc;

#[test]
fn artifact_ids_are_opaque_and_path_free() {
    for valid in ["artifact_report_abc123", "artifact_raw_a-b"] {
        assert!(validate_artifact_id(valid).is_ok(), "{valid}");
    }
    for invalid in [
        "",
        "report_abc",
        "artifact_../secret",
        "artifact_%2fsecret",
        "artifact_a.json",
        "/artifact_report_abc",
    ] {
        assert!(validate_artifact_id(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn download_disposition_cannot_inject_headers() {
    assert_eq!(
        safe_disposition("re\"port\r\n.json"),
        "attachment; filename=\"re_port__.json\""
    );
}

#[test]
fn unsupported_content_refs_fail_closed() {
    let result = content_bytes(Some(ContentRef::External {
        uri: "https://example.com/private".to_string(),
        integrity: None,
    }));
    assert!(result.is_err());
}

#[test]
fn manifest_content_paths_cannot_escape_the_store() {
    let manifest = StoredArtifactManifest {
        handle: ArtifactHandle {
            artifact_id: ArtifactId::new("artifact_report_abc"),
            artifact_kind: ArtifactKind::Report,
            uri: None,
        },
        content_type: "application/json".to_string(),
        content_path: "../secret".to_string(),
        metadata: MetadataMap::new(),
    };
    assert!(validate_manifest(&manifest).is_err());
}

#[tokio::test]
async fn typed_service_lists_and_reads_metadata_by_opaque_id() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("artifacts");
    tokio::fs::create_dir_all(&root).await.unwrap();
    tokio::fs::write(root.join("artifact_report_abc.bin"), b"report")
        .await
        .unwrap();
    tokio::fs::write(
        root.join("artifact_report_abc.json"),
        serde_json::to_vec(&serde_json::json!({
            "handle": {
                "artifact_id": "artifact_report_abc",
                "artifact_kind": "report",
                "uri": "file:///private/path"
            },
            "content_type": "application/json",
            "content_path": "artifact_report_abc.bin",
            "content_kind": "inline_bytes",
            "metadata": { "label": "report.json", "producer_refs": ["job:test"] }
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let cfg = Arc::new(axon_core::config::Config {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    });
    let ctx = ServiceContext::from_runtime(cfg, Arc::new(NoopServiceRuntime));

    let page = list_artifacts(
        &ctx,
        ArtifactListRequest {
            source_id: None,
            job_id: None,
            kind: None,
            limit: Some(10),
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].artifact_id.0, "artifact_report_abc");
    assert_eq!(page.items[0].size_bytes, 6);

    let detail = get_artifact(&ctx, ArtifactId::new("artifact_report_abc"))
        .await
        .unwrap();
    assert_eq!(
        detail.content_url,
        "/v1/artifacts/artifact_report_abc/content"
    );
    assert_eq!(detail.producer_refs, ["job:test"]);
    assert_eq!(detail.summary.label.as_deref(), Some("report.json"));
}
