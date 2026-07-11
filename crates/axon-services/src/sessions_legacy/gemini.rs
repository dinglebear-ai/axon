use super::{
    IngestResult, SessionDoc, flatten_session_result, matches_project_filter, resolve_collection,
};
use axon_core::config::Config;
use axon_core::logging::log_warn;
use axon_vector::ops::prepare_plain_text_source;
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

pub(super) async fn collect_gemini_file_doc(
    cfg: &Config,
    path: PathBuf,
) -> IngestResult<Option<SessionDoc>> {
    let gemini_root = super::expand_home("~/.gemini");
    let projects_map = load_gemini_projects(&gemini_root).await;
    let project_dir = gemini_project_dir_for_file(&path)
        .ok_or_else(|| anyhow::anyhow!("gemini session file is outside a project chats dir"))?;
    let dir_name = project_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown");
    let project_name = resolve_project_name(&project_dir, dir_name, &projects_map).await;
    if !matches_project_filter(cfg, &project_name) {
        return Ok(None);
    }
    let meta = fs::metadata(&path).await?;
    let mtime = meta.modified()?;
    let collection = resolve_collection(cfg, &project_name);
    process_gemini_file(path, collection, mtime, Some(project_name)).await
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
        enqueue_gemini_chat_files(chats_dir, collection, project_name, futures, docs).await?;
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
    project_name: String,
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
        let project_name = project_name.clone();
        futures.push(tokio::spawn(async move {
            process_gemini_file(chat_path, coll_clone, mtime, Some(project_name)).await
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
    project_name: Option<String>,
) -> IngestResult<Option<SessionDoc>> {
    let content = super::read_session_file_limited(&path).await?;
    let session_text = parse_gemini_json(&content)?;
    if session_text.trim().is_empty() {
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
        "project_name": project_name,
        "session_id": session_id,
        "session_date": mtime_chrono.to_rfc3339(),
    });
    let doc = prepare_plain_text_source(
        url.clone(),
        "local".to_string(),
        session_text.clone(),
        "gemini_session",
        title,
        Some(extra),
    );
    if doc.is_empty() {
        return Ok(None);
    }
    Ok(Some(SessionDoc {
        doc,
        collection,
        raw_text: session_text,
    }))
}

fn gemini_project_dir_for_file(path: &Path) -> Option<PathBuf> {
    let chats_dir = path.parent()?;
    if chats_dir.file_name().and_then(|name| name.to_str()) != Some("chats") {
        return None;
    }
    chats_dir.parent().map(Path::to_path_buf)
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
                        super::redact_session_text(&combined)
                    ));
                }
            }
        }
    }
    Ok(session_text)
}

#[cfg(test)]
#[path = "gemini_tests.rs"]
mod tests;
