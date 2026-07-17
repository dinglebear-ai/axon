use super::*;
use crate::test_support::NoopServiceRuntime;
use axon_api::source::ContentRef;
use base64::Engine as _;
use std::sync::Arc;

fn context() -> (tempfile::TempDir, ServiceContext) {
    let temp = tempfile::tempdir().unwrap();
    let cfg = Arc::new(axon_core::config::Config {
        output_dir: temp.path().to_path_buf(),
        ..Default::default()
    });
    let ctx = ServiceContext::from_runtime(cfg, Arc::new(NoopServiceRuntime));
    (temp, ctx)
}

fn create_request(bytes: &[u8]) -> UploadCreateRequest {
    UploadCreateRequest {
        filename: "notes.md".to_string(),
        content_type: "text/markdown".to_string(),
        size_bytes: bytes.len() as u64,
        purpose: UploadPurpose::SourceArtifact,
        sha256: Some(sha256_hex(bytes)),
        source_hint: Some("project-notes".to_string()),
        source_id: None,
        metadata: MetadataMap::new(),
    }
}

#[tokio::test]
async fn upload_identity_is_durably_mapped_to_distinct_artifact_identity() {
    let (_temp, ctx) = context();
    let bytes = b"# Durable upload\n";
    let created = create_upload(&ctx, create_request(bytes)).await.unwrap();
    assert!(created.upload_id.0.starts_with("upl_"));

    let received = put_upload_content(&ctx, created.upload_id.clone(), bytes.to_vec(), None, None)
        .await
        .unwrap();
    assert_eq!(received.status, UploadStatusKind::Received);

    let completed = complete_upload(
        &ctx,
        created.upload_id.clone(),
        UploadCompleteRequest::default(),
    )
    .await
    .unwrap();
    assert!(completed.artifact_id.0.starts_with("art_"));
    assert_ne!(completed.artifact_id.0, created.upload_id.0);
    assert_eq!(
        completed.source_ref,
        format!("upload://{}", created.upload_id.0)
    );

    let status = get_upload(&ctx, created.upload_id.clone()).await.unwrap();
    assert_eq!(status.artifact_id, Some(completed.artifact_id.clone()));
    let resolved = resolve_upload_artifact(&ctx, &created.upload_id.0)
        .await
        .unwrap()
        .unwrap();
    let ContentRef::InlineBytes { bytes_base64, .. } = resolved.content.unwrap() else {
        panic!("expected byte artifact")
    };
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(bytes_base64)
            .unwrap(),
        bytes
    );
}

#[tokio::test]
async fn sensitive_upload_metadata_is_rejected_fail_closed() {
    let (_temp, ctx) = context();
    let mut request = create_request(b"secret-safe");
    request.metadata.insert(
        "authorization".to_string(),
        serde_json::json!("Bearer do-not-persist"),
    );
    let error = create_upload(&ctx, request).await.unwrap_err();
    assert_eq!(error.code.0, "redaction.failed");
}

#[tokio::test]
async fn persisted_upload_records_redact_abort_reasons_and_retain_audit_events() {
    let (_temp, ctx) = context();
    let request = create_request(b"secret-safe");
    let created = create_upload(&ctx, request).await.unwrap();
    abort_upload(
        &ctx,
        created.upload_id.clone(),
        UploadAbortRequest {
            reason: Some("token=do-not-persist".to_string()),
        },
    )
    .await
    .unwrap();

    let record = load_record(&upload_root(&ctx), &created.upload_id)
        .await
        .unwrap();
    let persisted = serde_json::to_string(&record).unwrap();
    assert!(!persisted.contains("do-not-persist"));
    assert_eq!(record.status, UploadStatusKind::Aborted);
    assert_eq!(
        record
            .audit_events
            .iter()
            .map(|event| event.event.as_str())
            .collect::<Vec<_>>(),
        ["upload.create", "upload.abort"]
    );
}

#[tokio::test]
async fn direct_artifact_and_upload_ids_resolve_without_identity_conflation() {
    let (_temp, ctx) = context();
    let bytes = b"direct";
    let created = create_upload(&ctx, create_request(bytes)).await.unwrap();
    put_upload_content(&ctx, created.upload_id.clone(), bytes.to_vec(), None, None)
        .await
        .unwrap();
    let completed = complete_upload(&ctx, created.upload_id.clone(), Default::default())
        .await
        .unwrap();
    assert!(
        resolve_upload_artifact(&ctx, &created.upload_id.0)
            .await
            .unwrap()
            .is_some()
    );
    assert!(
        resolve_upload_artifact(&ctx, &completed.artifact_id.0)
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn hash_size_and_path_contracts_fail_closed() {
    let (_temp, ctx) = context();
    let mut invalid = create_request(b"x");
    invalid.filename = "../secret".to_string();
    assert_eq!(
        create_upload(&ctx, invalid).await.unwrap_err().code.0,
        "upload.filename_invalid"
    );

    let created = create_upload(&ctx, create_request(b"abc")).await.unwrap();
    assert_eq!(
        put_upload_content(&ctx, created.upload_id, b"ab".to_vec(), None, None)
            .await
            .unwrap_err()
            .code
            .0,
        "upload.size_mismatch"
    );
}

#[tokio::test]
async fn abort_removes_staged_content_and_list_preserves_audit_status() {
    let (_temp, ctx) = context();
    let bytes = b"discard";
    let created = create_upload(&ctx, create_request(bytes)).await.unwrap();
    put_upload_content(&ctx, created.upload_id.clone(), bytes.to_vec(), None, None)
        .await
        .unwrap();
    let aborted = abort_upload(
        &ctx,
        created.upload_id.clone(),
        UploadAbortRequest {
            reason: Some("caller canceled".to_string()),
        },
    )
    .await
    .unwrap();
    assert!(aborted.deleted);
    assert!(!part_path(&upload_root(&ctx), &created.upload_id).exists());
    let page = list_uploads(
        &ctx,
        UploadListRequest {
            status: Some(UploadStatusKind::Aborted),
            limit: None,
            cursor: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].status, UploadStatusKind::Aborted);
}

#[tokio::test]
async fn retention_expiry_deletes_the_promoted_artifact_and_mapping() {
    let (_temp, ctx) = context();
    let bytes = b"short lived";
    let created = create_upload(&ctx, create_request(bytes)).await.unwrap();
    put_upload_content(&ctx, created.upload_id.clone(), bytes.to_vec(), None, None)
        .await
        .unwrap();
    let completed = complete_upload(&ctx, created.upload_id.clone(), Default::default())
        .await
        .unwrap();
    let root = upload_root(&ctx);
    let mut record = load_record(&root, &created.upload_id).await.unwrap();
    record.retention_until = Some(Timestamp::from(Utc::now() - Duration::seconds(1)));
    save_record(&root, &record).await.unwrap();

    let status = get_upload(&ctx, created.upload_id.clone()).await.unwrap();
    assert_eq!(status.status, UploadStatusKind::Expired);
    assert!(status.artifact_id.is_none());
    assert!(
        resolve_upload_artifact(&ctx, &completed.artifact_id.0)
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn indexed_pagination_is_sorted_bounded_and_cursor_stable() {
    let (_temp, ctx) = context();
    for suffix in 0..5 {
        let mut request = create_request(b"x");
        request.filename = format!("note-{suffix}.md");
        create_upload(&ctx, request).await.unwrap();
    }

    let mut cursor = None;
    let mut ids = Vec::new();
    loop {
        let page = list_uploads(
            &ctx,
            UploadListRequest {
                status: Some(UploadStatusKind::Pending),
                limit: Some(2),
                cursor,
            },
        )
        .await
        .unwrap();
        assert!(page.items.len() <= 2);
        ids.extend(page.items.into_iter().map(|status| status.upload_id.0));
        let Some(next_cursor) = page.next_cursor else {
            break;
        };
        cursor = Some(next_cursor);
    }

    assert_eq!(ids.len(), 5);
    assert!(
        ids.windows(2)
            .all(|pair| pair[0].as_str() < pair[1].as_str())
    );
    assert!(upload_root(&ctx).join(INDEX_FILE).is_file());
}

#[tokio::test]
async fn cleanup_waits_for_the_upload_lock_before_expiring_a_record() {
    let (_temp, ctx) = context();
    let created = create_upload(&ctx, create_request(b"x")).await.unwrap();
    let root = upload_root(&ctx);
    let mut record = load_record(&root, &created.upload_id).await.unwrap();
    record.expires_at = Timestamp::from(Utc::now() - Duration::seconds(1));
    save_record(&root, &record).await.unwrap();

    let lock = acquire_lock(&root, &created.upload_id).await.unwrap();
    let cleanup_ctx = ctx.clone();
    let cleanup = tokio::spawn(async move { cleanup_expired_uploads(&cleanup_ctx).await });
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert!(
        !cleanup.is_finished(),
        "cleanup bypassed the per-upload lock"
    );
    drop(lock);

    assert_eq!(cleanup.await.unwrap().unwrap(), 1);
    assert_eq!(
        get_upload(&ctx, created.upload_id).await.unwrap().status,
        UploadStatusKind::Expired
    );
}

#[tokio::test]
async fn failed_promotion_record_commit_is_compensated_and_retryable() {
    let (_temp, ctx) = context();
    let bytes = b"retry promotion";
    let created = create_upload(&ctx, create_request(bytes)).await.unwrap();
    put_upload_content(&ctx, created.upload_id.clone(), bytes.to_vec(), None, None)
        .await
        .unwrap();
    fail_next_record_save(&created.upload_id);

    let error = complete_upload(&ctx, created.upload_id.clone(), Default::default())
        .await
        .unwrap_err();
    assert_eq!(error.code.0, "upload.test_record_save_failed");
    let artifact_entries = std::fs::read_dir(ctx.cfg.output_dir.join("artifacts"))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("art_"))
        .count();
    assert_eq!(artifact_entries, 0, "promotion left orphan artifact files");

    let completed = complete_upload(&ctx, created.upload_id.clone(), Default::default())
        .await
        .unwrap();
    tokio::fs::write(part_path(&upload_root(&ctx), &created.upload_id), bytes)
        .await
        .unwrap();
    let retried = complete_upload(&ctx, created.upload_id.clone(), Default::default())
        .await
        .unwrap();
    assert_eq!(retried.artifact_id, completed.artifact_id);
    assert!(!part_path(&upload_root(&ctx), &created.upload_id).exists());
}

#[tokio::test]
async fn abort_is_idempotent_without_duplicate_audit_events() {
    let (_temp, ctx) = context();
    let created = create_upload(&ctx, create_request(b"x")).await.unwrap();
    for _ in 0..2 {
        abort_upload(
            &ctx,
            created.upload_id.clone(),
            UploadAbortRequest::default(),
        )
        .await
        .unwrap();
    }

    let record = load_record(&upload_root(&ctx), &created.upload_id)
        .await
        .unwrap();
    assert_eq!(
        record
            .audit_events
            .iter()
            .filter(|event| event.event == "upload.abort")
            .count(),
        1
    );
}
