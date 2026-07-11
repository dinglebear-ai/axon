use super::*;
use std::io::Write;
use tempfile::TempDir;

// --- parse_gemini_json ---

#[test]
fn parse_valid_gemini_json_happy_path() {
    let json = r#"{"messages":[{"type":"human","content":[{"text":"What is the capital of France?"}]},{"type":"model","content":[{"text":"Paris."}]}]}"#;
    let result = parse_gemini_json(json).expect("should parse");
    assert!(result.contains("### HUMAN:"));
    assert!(result.contains("What is the capital of France?"));
    assert!(result.contains("### MODEL:"));
    assert!(result.contains("Paris."));
}

#[test]
fn parse_gemini_json_multiple_text_items_concatenated() {
    let json =
        r#"{"messages":[{"type":"model","content":[{"text":"First. "},{"text":"Second."}]}]}"#;
    let result = parse_gemini_json(json).expect("should parse");
    assert!(result.contains("First."));
    assert!(result.contains("Second."));
}

#[test]
fn parse_gemini_json_malformed_returns_err_not_panic() {
    assert!(
        parse_gemini_json("this is not json").is_err(),
        "malformed JSON must return Err"
    );
}

#[test]
fn parse_gemini_json_empty_messages_array() {
    let json = r#"{"messages":[]}"#;
    let result = parse_gemini_json(json).expect("should parse");
    assert!(result.trim().is_empty());
}

#[test]
fn parse_gemini_json_no_messages_key() {
    let json = r#"{"conversations":[]}"#;
    let result = parse_gemini_json(json).expect("should parse");
    assert!(result.trim().is_empty());
}

#[test]
fn parse_gemini_json_whitespace_only_content_skipped() {
    let json = r#"{"messages":[{"type":"human","content":[{"text":"   "}]},{"type":"model","content":[{"text":"Real response"}]}]}"#;
    let result = parse_gemini_json(json).expect("should parse");
    assert!(!result.contains("### HUMAN:"));
    assert!(result.contains("Real response"));
}

#[test]
fn parse_gemini_json_missing_type_falls_back_to_unknown() {
    let json = r#"{"messages":[{"content":[{"text":"Mystery"}]}]}"#;
    let result = parse_gemini_json(json).expect("should parse");
    assert!(result.contains("### UNKNOWN:"));
    assert!(result.contains("Mystery"));
}

// --- load_gemini_projects ---

#[tokio::test]
async fn load_gemini_projects_happy_path() {
    let dir = TempDir::new().expect("temp dir");
    let json = r#"{"projects":{"/home/user/workspace/my-project":"my-project","/home/user/workspace/axon-rust":"axon-rust"}}"#;
    let p = dir.path().join("projects.json");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(json.as_bytes()).unwrap();
    drop(f);

    let map = load_gemini_projects(dir.path()).await;
    assert_eq!(
        map.get("/home/user/workspace/my-project"),
        Some(&"my-project".to_string())
    );
    // Last path segment is also inserted as a key
    assert_eq!(map.get("my-project"), Some(&"my-project".to_string()));
    assert_eq!(map.get("axon-rust"), Some(&"axon-rust".to_string()));
}

#[tokio::test]
async fn load_gemini_projects_missing_file_returns_empty_map() {
    let dir = TempDir::new().expect("temp dir");
    let map = load_gemini_projects(dir.path()).await;
    assert!(map.is_empty(), "missing projects.json yields empty map");
}

#[tokio::test]
async fn load_gemini_projects_malformed_json_returns_empty_map() {
    let dir = TempDir::new().expect("temp dir");
    let p = dir.path().join("projects.json");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(b"not json").unwrap();
    drop(f);

    let map = load_gemini_projects(dir.path()).await;
    assert!(map.is_empty(), "malformed JSON yields empty map");
}

#[tokio::test]
async fn load_gemini_projects_non_string_name_ignored() {
    let dir = TempDir::new().expect("temp dir");
    let json = r#"{"projects":{"/home/user/good":"good-name","/home/user/bad":42}}"#;
    let p = dir.path().join("projects.json");
    let mut f = std::fs::File::create(&p).unwrap();
    f.write_all(json.as_bytes()).unwrap();
    drop(f);

    let map = load_gemini_projects(dir.path()).await;
    assert!(map.contains_key("good"), "valid string entry present");
    assert!(!map.contains_key("bad"), "non-string entry skipped");
}
