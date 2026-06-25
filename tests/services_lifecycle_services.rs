use axon_services::crawl::{map_crawl_job_result, map_crawl_start_result};
use axon_services::embed::{map_embed_job_result, map_embed_start_result};
use axon_services::extract::{map_extract_job_result, map_extract_start_result};
use axon_services::ingest::map_ingest_result;
use axon_services::screenshot::map_screenshot_result;
use std::path::Path;

// --- crawl mapping helpers ---

#[test]
fn maps_crawl_start_job_ids() {
    let jobs = vec![
        (
            "https://docs.example.com".to_string(),
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
        ),
        (
            "https://api.example.com".to_string(),
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8".to_string(),
        ),
    ];
    let result = map_crawl_start_result(Path::new("/tmp/axon-output"), &jobs);
    assert_eq!(
        result.job_ids,
        vec![
            "550e8400-e29b-41d4-a716-446655440000".to_string(),
            "6ba7b810-9dad-11d1-80b4-00c04fd430c8".to_string(),
        ]
    );
    assert_eq!(result.jobs.len(), 2);
    assert_eq!(result.jobs[0].url, "https://docs.example.com");
    assert_eq!(
        result.jobs[0].output_dir,
        "/tmp/axon-output/domains/docs.example.com/550e8400-e29b-41d4-a716-446655440000"
    );
    assert_eq!(
        result.jobs[0].predicted_paths,
        vec![
            "/tmp/axon-output/domains/docs.example.com/550e8400-e29b-41d4-a716-446655440000/manifest.jsonl".to_string(),
            "/tmp/axon-output/domains/docs.example.com/550e8400-e29b-41d4-a716-446655440000/markdown".to_string(),
            "/tmp/axon-output/domains/docs.example.com/550e8400-e29b-41d4-a716-446655440000/audit/docs-example-com-diff-report.json".to_string(),
        ]
    );
}

#[test]
fn maps_crawl_start_empty_ids() {
    let result = map_crawl_start_result(Path::new("/tmp/axon-output"), &[]);
    assert!(result.job_ids.is_empty());
    assert!(result.jobs.is_empty());
    assert_eq!(result.output_dir, None);
    assert!(result.predicted_paths.is_empty());
}

#[test]
fn maps_crawl_job_result_preserves_payload() {
    let payload = serde_json::json!({
        "job_id": "550e8400-e29b-41d4-a716-446655440000",
        "status": "completed",
        "pages_crawled": 42,
        "url": "https://docs.example.com"
    });
    let result = map_crawl_job_result(payload.clone());
    assert_eq!(result.payload, payload);
}

#[test]
fn maps_crawl_job_result_null_payload() {
    let payload = serde_json::Value::Null;
    let result = map_crawl_job_result(payload.clone());
    assert_eq!(result.payload, serde_json::Value::Null);
}

// --- embed mapping helpers ---

#[test]
fn maps_embed_start_result_with_job_id() {
    let job_id = "550e8400-e29b-41d4-a716-446655440001".to_string();
    let result = map_embed_start_result(job_id.clone());
    assert_eq!(result.job_id, job_id);
}

#[test]
fn maps_embed_job_result_preserves_payload() {
    let payload = serde_json::json!({
        "job_id": "abc",
        "status": "running",
        "points_embedded": 100
    });
    let result = map_embed_job_result(payload.clone());
    assert_eq!(result.payload, payload);
}

// --- extract mapping helpers ---

#[test]
fn maps_extract_start_result_with_job_id() {
    let job_id = "550e8400-e29b-41d4-a716-446655440002".to_string();
    let result = map_extract_start_result(job_id.clone());
    assert_eq!(result.job_id, job_id);
}

#[test]
fn maps_extract_job_result_preserves_payload() {
    let payload = serde_json::json!({
        "job_id": "xyz",
        "status": "pending",
        "urls": ["https://example.com"],
        "total_items": 0
    });
    let result = map_extract_job_result(payload.clone());
    assert_eq!(result.payload, payload);
}

// --- ingest mapping helpers ---

#[test]
fn maps_ingest_result_github() {
    let payload = serde_json::json!({
        "source": "github",
        "repo": "owner/name",
        "chunks": 87
    });
    let result = map_ingest_result(payload.clone());
    assert_eq!(result.payload, payload);
}

#[test]
fn maps_ingest_result_reddit() {
    let payload = serde_json::json!({
        "source": "reddit",
        "target": "rust",
        "chunks": 23
    });
    let result = map_ingest_result(payload.clone());
    assert_eq!(result.payload, payload);
}

#[test]
fn maps_ingest_result_youtube() {
    let payload = serde_json::json!({
        "source": "youtube",
        "url": "https://youtube.com/watch?v=abc",
        "chunks": 5
    });
    let result = map_ingest_result(payload.clone());
    assert_eq!(result.payload["source"], "youtube");
    assert_eq!(result.payload["chunks"], 5);
}

#[test]
fn maps_ingest_result_sessions() {
    let payload = serde_json::json!({
        "source": "sessions",
        "chunks": 12
    });
    let result = map_ingest_result(payload.clone());
    assert_eq!(result.payload, payload);
}

#[test]
fn maps_ingest_result_null_payload() {
    let result = map_ingest_result(serde_json::Value::Null);
    assert_eq!(result.payload, serde_json::Value::Null);
}

// --- screenshot mapping helper ---

#[test]
fn maps_screenshot_result_to_typed_contract() {
    let payload = serde_json::json!({
        "url": "https://example.com",
        "path": "/output/screenshots/0001-example-com.png",
        "size_bytes": 98765
    });
    let result = map_screenshot_result(&payload).unwrap();
    assert_eq!(result.url, "https://example.com");
    assert_eq!(result.path, "/output/screenshots/0001-example-com.png");
    assert_eq!(result.size_bytes, 98765);
}

#[test]
fn maps_screenshot_result_rejects_raw_null_payload() {
    assert!(map_screenshot_result(&serde_json::Value::Null).is_err());
}

#[test]
fn maps_screenshot_result_rejects_missing_required_fields() {
    let payload = serde_json::json!({
        "url": "https://docs.rust-lang.org",
        "path": "/screenshots/0001-docs-rust-lang-org.png"
    });
    assert!(map_screenshot_result(&payload).is_err());
}

#[test]
fn maps_screenshot_result_fields() {
    let payload = serde_json::json!({
        "url": "https://docs.rust-lang.org",
        "path": "/screenshots/0001-docs-rust-lang-org.png",
        "size_bytes": 42000
    });
    let result = map_screenshot_result(&payload).unwrap();
    assert_eq!(result.url, "https://docs.rust-lang.org");
    assert_eq!(result.size_bytes, 42000);
}

#[test]
fn ingest_service_exposes_start_status_cancel_list_cleanup_clear_recover() {
    let _ = axon_services::ingest::ingest_start;
    let _ = axon_services::ingest::ingest_status;
    let _ = axon_services::ingest::ingest_cancel;
    let _ = axon_services::ingest::ingest_list;
    let _ = axon_services::ingest::ingest_cleanup;
    let _ = axon_services::ingest::ingest_clear;
    let _ = axon_services::ingest::ingest_recover;
}

#[test]
fn classify_target_returns_ingest_source_for_github_slug() {
    let src = axon_services::ingest::classify_target("owner/repo", true).expect("must classify");
    assert!(matches!(
        src,
        axon_jobs::ingest::IngestSource::Github { .. }
    ));
}

#[test]
fn classify_target_rejects_unknown_target() {
    let err = axon_services::ingest::classify_target("not-a-target", false).unwrap_err();
    assert!(err.to_string().contains("cannot determine ingest source"));
}
