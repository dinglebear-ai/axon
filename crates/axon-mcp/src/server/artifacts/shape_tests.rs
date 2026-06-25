use super::*;

#[test]
fn json_shape_preview_short_strings_are_verbatim() {
    let val = serde_json::json!({
        "name": "axon",
        "count": 42,
        "items": [1, 2, 3],
        "nested": { "key": "value" },
    });
    let preview = json_shape_preview(&val);
    assert_eq!(preview["name"], "axon");
    assert_eq!(preview["count"], 42);
    assert_eq!(preview["items"]["total"], 3);
    assert!(preview["items"]["sample"].is_array());
    assert!(preview["nested"].is_object());
    assert_eq!(preview["nested"]["key"], "value");
}

#[test]
fn json_shape_preview_long_strings_are_summarized() {
    let long = "x".repeat(101);
    let val = serde_json::json!({ "body": long });
    let preview = json_shape_preview(&val);
    assert_eq!(preview["body"], "<string 101>");
}

#[test]
fn json_shape_preview_status_histogram() {
    let val = serde_json::json!({
        "jobs": [
            {"id": 1, "status": "completed"},
            {"id": 2, "status": "running"},
            {"id": 3, "status": "completed"},
            {"id": 4, "status": "failed"},
        ]
    });
    let preview = json_shape_preview(&val);
    let jobs = &preview["jobs"];
    assert_eq!(jobs["total"], 4);
    assert_eq!(jobs["by_status"]["completed"], 2);
    assert_eq!(jobs["by_status"]["running"], 1);
    assert_eq!(jobs["by_status"]["failed"], 1);
}

#[test]
fn status_histogram_returns_none_for_non_object_arrays() {
    let arr = vec![
        serde_json::json!(1),
        serde_json::json!(2),
        serde_json::json!(3),
    ];
    assert!(status_histogram(&arr).is_none());
}

#[test]
fn clip_inline_json_array_truncates_at_item_boundaries() {
    let items: Vec<_> = (0..5)
        .map(|i| serde_json::json!({"id": i, "text": "x".repeat(200)}))
        .collect();
    let val = serde_json::Value::Array(items);
    let (clipped, truncated) = clip_inline_json(&val, 600);
    assert!(truncated, "should be truncated");
    let arr = clipped.as_array().expect("must be array");
    let last = arr.last().expect("must have items");
    assert!(
        last.get("__truncated__").is_some(),
        "must have truncation marker"
    );
    for item in &arr[..arr.len() - 1] {
        assert!(item.get("id").is_some(), "item must be complete object");
    }
}

#[test]
fn clip_inline_json_object_truncates_long_string_fields() {
    let long_val = "x".repeat(600);
    let val = serde_json::json!({
        "query": "short",
        "answer": long_val,
        "count": 42,
    });
    let (clipped, truncated) = clip_inline_json(&val, 300);
    assert!(truncated, "should be truncated");
    assert!(clipped.get("query").is_some());
    assert!(clipped.get("answer").is_some());
    assert!(clipped.get("count").is_some());
    let answer = &clipped["answer"];
    assert!(answer.is_object(), "long string must become head object");
    assert!(answer.get("__head__").is_some(), "must have __head__ field");
    assert!(
        answer.get("__total_chars__").is_some(),
        "must have __total_chars__"
    );
    assert_eq!(clipped["query"], "short");
    assert_eq!(clipped["count"], 42);
}

#[test]
fn clip_inline_json_does_not_produce_clipped_json_wrapper() {
    let large_obj = serde_json::json!({
        "a": "x".repeat(5000),
        "b": "y".repeat(5000),
        "c": "z".repeat(5000),
    });
    let (clipped, _) = clip_inline_json(&large_obj, 100);
    let serialized = serde_json::to_string(&clipped).unwrap();
    assert!(
        !serialized.contains("clipped_json"),
        "must not produce clipped_json wrapper"
    );
}

#[test]
fn clip_inline_json_small_payload_is_unchanged() {
    let val = serde_json::json!({"key": "value", "n": 42});
    let (clipped, truncated) = clip_inline_json(&val, 10_000);
    assert!(!truncated);
    assert_eq!(clipped, val);
}

#[test]
fn json_shape_preview_non_status_array_shows_sample_items() {
    let val = serde_json::json!({
        "results": [
            {"url": "https://a.com", "score": 0.95, "title": "A"},
            {"url": "https://b.com", "score": 0.91, "title": "B"},
            {"url": "https://c.com", "score": 0.88, "title": "C"},
        ]
    });
    let preview = json_shape_preview(&val);
    let results = &preview["results"];
    assert_eq!(results["total"], 3);
    let sample = results["sample"].as_array().expect("sample must be array");
    assert_eq!(sample.len(), 2, "sample shows first 2 items");
    assert!(sample[0].get("url").is_some());
}

#[test]
fn json_shape_preview_status_array_unchanged() {
    let val = serde_json::json!([
        {"status": "completed"}, {"status": "running"}, {"status": "completed"}
    ]);
    let preview = json_shape_preview(&val);
    assert_eq!(preview["total"], 3);
    assert!(preview.get("by_status").is_some());
    assert!(
        preview.get("sample").is_none(),
        "status arrays don't show sample"
    );
}
