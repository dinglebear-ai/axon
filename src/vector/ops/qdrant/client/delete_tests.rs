use super::*;

#[test]
fn repo_code_delete_body_is_scoped_to_one_repo_file_points() {
    let body = repo_code_points_delete_body("github", "owner-a", "repo-a");
    let must = body["filter"]["must"]
        .as_array()
        .expect("canonical must array");
    assert_eq!(must.len(), 4);
    assert!(must.contains(&serde_json::json!({
        "key": "provider",
        "match": {"value": "github"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_owner",
        "match": {"value": "owner-a"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_repo",
        "match": {"value": "repo-a"}
    })));
    assert!(must.contains(&serde_json::json!({
        "key": "git_content_kind",
        "match": {"value": "file"}
    })));
}
