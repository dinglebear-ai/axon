use super::{
    IngestResult, SessionDoc, flatten_session_result, matches_project_filter, resolve_collection,
};
use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::vector::ops::{PreparedDoc, chunk_text};
use futures_util::stream::{FuturesUnordered, StreamExt};
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tokio::fs;

pub(super) async fn collect_gemini_docs(
    cfg: &Config,
    multi: &MultiProgress,
) -> IngestResult<Vec<SessionDoc>> {
    let gemini_root = super::expand_home("~/.gemini");
    let projects_map = load_gemini_projects(&gemini_root).await;

    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.magenta} Gemini: {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut docs: Vec<SessionDoc> = Vec::new();
    let mut futures: GeminiFutures = FuturesUnordered::new();

    for root in [gemini_root.join("history"), gemini_root.join("tmp")] {
        if !fs::try_exists(&root).await.unwrap_or(false) {
            continue;
        }
        enqueue_gemini_dir(cfg, &projects_map, root, &mut futures, &mut docs).await?;
    }

    while let Some(res) = futures.next().await {
        if let Some(doc) = flatten_session_result(res, "Gemini") {
            docs.push(doc);
        }
    }

    pb.finish_with_message(format!("scanned {} files", docs.len()));
    Ok(docs)
}

type GeminiFutures = FuturesUnordered<tokio::task::JoinHandle<IngestResult<Option<SessionDoc>>>>;

async fn enqueue_gemini_dir(
    cfg: &Config,
    projects_map: &HashMap<String, String>,
    root: PathBuf,
    futures: &mut GeminiFutures,
    docs: &mut Vec<SessionDoc>,
) -> IngestResult<()> {
    let mut read_dir = fs::read_dir(root).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let path = entry.path();
        let file_type = match entry.file_type().await {
            Ok(kind) => kind,
            Err(error) => {
                log_warn(&format!(
                    "gemini: skipping unreadable directory entry {}: {error}",
                    path.display()
                ));
                continue;
            }
        };
        if !file_type.is_dir() {
            continue;
        }
        let dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let project_name = resolve_project_name(&path, dir_name, projects_map).await;
        if !matches_project_filter(cfg, &project_name) {
            continue;
        }

        let collection = resolve_collection(cfg, &project_name);
        let chats_dir = path.join("chats");
        if !fs::try_exists(&chats_dir).await.unwrap_or(false) {
            continue;
        }
        enqueue_gemini_chat_files(chats_dir, collection, futures, docs).await?;
    }
    Ok(())
}

async fn resolve_project_name(
    path: &Path,
    dir_name: &str,
    projects_map: &HashMap<String, String>,
) -> String {
    if let Some(mapped) = projects_map.get(dir_name) {
        return mapped.clone();
    }
    let root_file = path.join(".project_root");
    if let Ok(root_path) = fs::read_to_string(root_file).await
        && let Some(mapped) = projects_map.get(root_path.trim())
    {
        return mapped.clone();
    }
    dir_name.to_string()
}

async fn enqueue_gemini_chat_files(
    chats_dir: PathBuf,
    collection: String,
    futures: &mut GeminiFutures,
    docs: &mut Vec<SessionDoc>,
) -> IngestResult<()> {
    let mut chats_read = fs::read_dir(chats_dir).await?;
    while let Some(chat_entry) = chats_read.next_entry().await? {
        let chat_path = chat_entry.path();
        if chat_path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let meta = fs::metadata(&chat_path).await?;
        let mtime = match meta.modified() {
            Ok(t) => t,
            Err(e) => {
                log_warn(&format!(
                    "cannot read mtime for {}: {e}",
                    chat_path.display()
                ));
                continue;
            }
        };

        let coll_clone = collection.clone();
        futures.push(tokio::spawn(async move {
            process_gemini_file(chat_path, coll_clone, mtime).await
        }));

        if futures.len() >= 32
            && let Some(res) = futures.next().await
            && let Some(doc) = flatten_session_result(res, "Gemini")
        {
            docs.push(doc);
        }
    }
    Ok(())
}

async fn load_gemini_projects(root: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let projects_file = root.join("projects.json");
    if let Ok(content) = fs::read_to_string(projects_file).await
        && let Ok(val) = serde_json::from_str::<Value>(&content)
        && let Some(projects) = val["projects"].as_object()
    {
        for (path, name) in projects {
            if let Some(n) = name.as_str() {
                map.insert(path.clone(), n.to_string());
                if let Some(last) = path.split('/').next_back() {
                    map.insert(last.to_string(), n.to_string());
                }
            }
        }
    }
    map
}

async fn process_gemini_file(
    path: PathBuf,
    collection: String,
    mtime: SystemTime,
) -> IngestResult<Option<SessionDoc>> {
    let content = fs::read_to_string(&path).await?;
    let session_text = parse_gemini_json(&content)?;
    if session_text.trim().is_empty() {
        return Ok(None);
    }
    let chunks = chunk_text(&session_text);
    if chunks.is_empty() {
        return Ok(None);
    }
    let url = format!("file://{}", path.display());
    let title = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string);
    let session_id = path
        .file_stem()
        .and_then(|n| n.to_str())
        .map(str::to_string);
    let mtime_chrono: chrono::DateTime<chrono::Utc> = mtime.into();
    let extra = serde_json::json!({
        "agent": "gemini",
        "session_id": session_id,
        "session_date": mtime_chrono.to_rfc3339(),
    });
    let doc = PreparedDoc {
        url,
        domain: "localhost".to_string(),
        chunks,
        source_type: "sessions".to_string(),
        content_type: "text",
        title,
        extra: Some(extra),
    };
    Ok(Some(SessionDoc { doc, collection }))
}

/// Parse Gemini chat JSON into session text (pure, no I/O).
pub(crate) fn parse_gemini_json(content: &str) -> IngestResult<String> {
    let val: Value = serde_json::from_str(content)?;
    let mut session_text = String::new();
    if let Some(messages) = val["messages"].as_array() {
        for msg in messages {
            let role = msg["type"].as_str().unwrap_or("unknown");
            if let Some(content_arr) = msg["content"].as_array() {
                let mut combined = String::new();
                for item in content_arr {
                    if let Some(t) = item["text"].as_str() {
                        combined.push_str(t);
                        combined.push('\n');
                    }
                }
                if !combined.trim().is_empty() {
                    session_text.push_str(&format!(
                        "\n\n### {}:\n{}",
                        role.to_uppercase(),
                        combined
                    ));
                }
            }
        }
    }
    Ok(session_text)
}

#[cfg(test)]
mod tests {
    use super::{load_gemini_projects, parse_gemini_json};
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
}
