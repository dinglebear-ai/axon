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
    );

    assert_eq!(current["phase"], "embedding_batch");
    assert_eq!(current["files_done"], 1);
    assert_eq!(current["chunks_embedded"], 42);
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
