use super::{
    IngestResult, SessionDoc, SessionMeta, flatten_session_result, matches_project_filter,
    resolve_collection,
};
use crate::core::config::Config;
use crate::vector::ops::{PreparedDoc, chunk_text};
use futures_util::stream::{FuturesUnordered, StreamExt};
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};
use tokio::fs;

pub(crate) struct ParsedCodexSession {
    pub(crate) text: String,
    pub(crate) turn_count: u32,
    pub(crate) model: Option<String>,
    pub(crate) has_tool_use: bool,
    pub(crate) tools_used: Vec<String>,
    pub(crate) workspace_path: Option<String>,
}

pub(super) async fn collect_codex_docs(
    cfg: &Config,
    multi: &MultiProgress,
) -> IngestResult<Vec<SessionDoc>> {
    let root = super::expand_home("~/.codex/sessions");
    if !fs::try_exists(&root).await.unwrap_or(false) {
        return Ok(vec![]);
    }

    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.yellow} Codex: {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut docs: Vec<SessionDoc> = Vec::new();
    // Track (dir, depth, project_name): depth 1 = direct children of root (project dirs).
    let mut dir_entries: Vec<(PathBuf, usize, String)> = vec![(root, 0, String::new())];
    let mut futures = FuturesUnordered::new();

    while let Some((current_dir, depth, project_name)) = dir_entries.pop() {
        let current_project = if depth == 1 {
            let dir_name = current_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !matches_project_filter(cfg, dir_name) {
                continue;
            }
            dir_name.to_string()
        } else {
            project_name
        };

        let mut read_dir = fs::read_dir(&current_dir).await?;
        while let Some(entry) = read_dir.next_entry().await? {
            let path = entry.path();
            if entry.file_type().await?.is_dir() {
                dir_entries.push((path, depth + 1, current_project.clone()));
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            let meta = fs::metadata(&path).await?;
            let mtime = meta.modified()?;

            let collection = resolve_collection(cfg, "codex");
            let session_meta = SessionMeta {
                agent: "codex",
                project_name: current_project.clone(),
                project_path: None,
                gh_repo: None,
            };
            futures.push(tokio::spawn(async move {
                parse_codex_file(path, collection, mtime, session_meta).await
            }));

            if futures.len() >= 64
                && let Some(res) = futures.next().await
                && let Some(doc) = flatten_session_result(res, "Codex")
            {
                docs.push(doc);
            }
        }
    }

    while let Some(res) = futures.next().await {
        if let Some(doc) = flatten_session_result(res, "Codex") {
            docs.push(doc);
        }
    }

    pb.finish_with_message(format!("scanned {} files", docs.len()));
    Ok(docs)
}

async fn parse_codex_file(
    path: PathBuf,
    collection: String,
    mtime: SystemTime,
    session_meta: SessionMeta,
) -> IngestResult<Option<SessionDoc>> {
    let content = super::read_session_file_limited(&path).await?;
    let parsed = parse_codex_jsonl(&content);
    if parsed.text.trim().is_empty() {
        return Ok(None);
    }
    let chunks = chunk_text(&parsed.text);
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
        "agent": session_meta.agent,
        "project_name": session_meta.project_name,
        "session_id": session_id,
        "session_date": mtime_chrono.to_rfc3339(),
        "turn_count": parsed.turn_count,
        "model": parsed.model,
        "has_tool_use": parsed.has_tool_use,
        "tools_used": parsed.tools_used,
        "workspace_path": parsed.workspace_path,
    });
    let doc = PreparedDoc {
        url,
        domain: "local".to_string(),
        chunks,
        source_type: "codex_session".to_string(),
        content_type: "text",
        title,
        extra: Some(extra),
        extractor_name: None,
    };
    Ok(Some(SessionDoc { doc, collection }))
}

/// Extract session text and metadata from Codex JSONL (pure, no I/O).
pub(crate) fn parse_codex_jsonl(content: &str) -> ParsedCodexSession {
    let mut session_text = String::new();
    let mut turn_count: u32 = 0;
    let mut model: Option<String> = None;
    let mut has_tool_use = false;
    let mut tools_used: HashSet<String> = HashSet::new();
    let mut workspace_path: Option<String> = None;

    for line in content.lines() {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        // Extract session-level metadata from the session_meta header line.
        if val["type"] == "session_meta" {
            if workspace_path.is_none() {
                workspace_path = val["payload"]["cwd"].as_str().map(str::to_string);
            }
            if model.is_none() {
                // Prefer an explicit model name; fall back to model_provider.
                model = val["payload"]["model"]
                    .as_str()
                    .or_else(|| val["payload"]["model_provider"].as_str())
                    .map(str::to_string);
            }
            continue;
        }

        if val["type"] != "response_item" {
            continue;
        }
        let role = val["payload"]["role"].as_str().unwrap_or("unknown");
        if let Some(arr) = val["payload"]["content"].as_array() {
            let mut combined = String::new();
            for item in arr {
                let item_type = item["type"].as_str().unwrap_or("");
                if matches!(item_type, "function_call" | "tool_call" | "tool_use") {
                    has_tool_use = true;
                    // function_call items store the name directly; tool_use items may
                    // nest it under "function.name".
                    let name = item["name"]
                        .as_str()
                        .or_else(|| item["function"]["name"].as_str());
                    if let Some(n) = name {
                        tools_used.insert(n.to_string());
                    }
                }
                if let Some(t) = item["text"].as_str() {
                    combined.push_str(&super::redact_session_text(t));
                    combined.push('\n');
                } else if let Some(t) = item["input_text"].as_str() {
                    combined.push_str(&super::redact_session_text(t));
                    combined.push('\n');
                }
            }
            if !combined.trim().is_empty() {
                session_text.push_str(&format!("\n\n### {}:\n{}", role.to_uppercase(), combined));
                if role == "user" {
                    turn_count += 1;
                }
            }
        }
    }

    let mut tools_list: Vec<String> = tools_used.into_iter().collect();
    tools_list.sort();

    ParsedCodexSession {
        text: session_text,
        turn_count,
        model,
        has_tool_use,
        tools_used: tools_list,
        workspace_path,
    }
}

#[cfg(test)]
#[path = "codex_tests.rs"]
mod tests;
