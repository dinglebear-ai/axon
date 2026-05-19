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
