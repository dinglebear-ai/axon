use super::*;

#[test]
fn merge_progress_overlays_object_fields() {
    let mut current = serde_json::json!({
        "phase": "cloning",
        "files_done": 1,
        "chunks_embedded": 0,
    });

    merge_progress(
        &mut current,
        serde_json::json!({
            "phase": "embedding_batch",
            "chunks_embedded": 42,
        }),
        uuid::Uuid::nil(),
        "github",
        "owner/repo",
    );

    assert_eq!(current["phase"], "embedding_batch");
    assert_eq!(current["files_done"], 1);
    assert_eq!(current["chunks_embedded"], 42);
}

#[test]
fn merge_progress_preserves_current_fields_and_warns_on_non_object_update() {
    let mut current = serde_json::json!({
        "phase": "embedding_files",
        "files_done": 7,
    });
    let job_id = uuid::Uuid::nil();

    merge_progress(
        &mut current,
        serde_json::json!("not progress"),
        job_id,
        "github",
        "owner/repo",
    );

    assert_eq!(current["phase"], "embedding_files");
    assert_eq!(current["files_done"], 7);
    assert!(current["progress_warning"].as_str().is_some_and(|warning| {
        warning.contains("job_id=00000000-0000-0000-0000-000000000000")
            && warning.contains("source=github")
            && warning.contains("progress update was not a JSON object")
    }));
}

#[test]
fn current_progress_warns_on_invalid_result_json() {
    let job_id = uuid::Uuid::nil();

    let current = current_progress_from_result_json(
        job_id,
        "github",
        "owner/repo",
        Some("{not json".to_string()),
    );

    assert!(
        current["result_json_warning"]
            .as_str()
            .is_some_and(
                |warning| warning.contains("job_id=00000000-0000-0000-0000-000000000000")
                    && warning.contains("source=github")
                    && warning.contains("invalid JSON")
            )
    );
}

#[test]
fn current_progress_warns_on_non_object_result_json() {
    let job_id = uuid::Uuid::nil();

    let current = current_progress_from_result_json(
        job_id,
        "youtube",
        "video-id",
        Some("[\"not\", \"an\", \"object\"]".to_string()),
    );

    assert!(
        current["result_json_warning"]
            .as_str()
            .is_some_and(|warning| warning.contains("source=youtube")
                && warning.contains("array")
                && warning.contains("not a JSON object"))
    );
}

#[test]
fn merge_final_payload_preserves_result_json_warning() {
    let current = serde_json::json!({
        "result_json_warning": "job_id=00000000-0000-0000-0000-000000000000 source=github: stored result_json was invalid JSON",
    });
    let final_payload = serde_json::json!({
        "source": "github",
        "chunks": 3,
        "result_json_warning": "final payload warning",
    });

    let merged = merge_final_payload(current, final_payload);

    assert_eq!(
        merged["result_json_warning"],
        "job_id=00000000-0000-0000-0000-000000000000 source=github: stored result_json was invalid JSON"
    );
    assert_eq!(merged["phase"], "completed");
}

#[test]
fn merge_final_payload_preserves_progress_fields_and_adds_canonical_chunks() {
    let current = serde_json::json!({
        "phase": "embedding_files",
        "files_done": 3563,
        "files_total": 3563,
        "chunks_embedded": 43598,
    });
    let final_payload = serde_json::json!({
        "source": "github",
        "repo": "nexu-io/open-design",
        "chunks": 43598,
    });

    let merged = merge_final_payload(current, final_payload);

    assert_eq!(merged["source"], "github");
    assert_eq!(merged["repo"], "nexu-io/open-design");
    assert_eq!(merged["phase"], "completed");
    assert_eq!(merged["files_done"], 3563);
    assert_eq!(merged["files_total"], 3563);
    assert_eq!(merged["chunks"], 43598);
    assert_eq!(merged["chunks_embedded"], 43598);
}
