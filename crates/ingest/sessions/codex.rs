use super::{
    IngestResult, SessionStateTracker, embed_with_retry, handle_spawn_result,
    matches_project_filter, resolve_collection,
};
use crate::crates::core::config::Config;
use futures_util::stream::{FuturesUnordered, StreamExt};
use indicatif::MultiProgress;
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs;

pub(super) async fn ingest_codex_sessions(
    cfg: &Config,
    state: &SessionStateTracker,
    multi: &MultiProgress,
) -> IngestResult<usize> {
    let root = super::expand_home("~/.codex/sessions");
    if !root.exists() {
        return Ok(0);
    }

    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.yellow} Codex: {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut total = 0;
    // Track (dir, depth): depth 1 = direct children of root (project dirs) — apply filter.
    let mut dir_entries: Vec<(PathBuf, usize)> = vec![(root, 0)];
    let mut futures = FuturesUnordered::new();

    while let Some((current_dir, depth)) = dir_entries.pop() {
        if depth == 1 {
            // Direct children of root are project directories — apply the project filter.
            let dir_name = current_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if !matches_project_filter(cfg, dir_name) {
                continue;
            }
        }

        let mut read_dir = fs::read_dir(&current_dir)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        while let Some(entry) = read_dir
            .next_entry()
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
        {
            let path = entry.path();
            if path.is_dir() {
                dir_entries.push((path, depth + 1));
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            let meta = fs::metadata(&path)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let mtime = meta
                .modified()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            if state.should_skip(&path, mtime, meta.len()).await {
                continue;
            }

            let collection = resolve_collection(cfg, "codex");
            let cfg_clone = cfg.clone();
            let size = meta.len();
            futures.push(tokio::spawn(async move {
                let res = process_codex_file(&cfg_clone, path.clone(), collection).await;
                (path, mtime, size, res)
            }));

            if futures.len() >= 32
                && let Some(res) = futures.next().await
            {
                total += handle_spawn_result(res, state, "Codex").await;
            }
        }
    }

    while let Some(res) = futures.next().await {
        total += handle_spawn_result(res, state, "Codex").await;
    }

    pb.finish_with_message(format!("indexed {} chunks", total));
    Ok(total)
}

async fn process_codex_file(
    cfg: &Config,
    path: PathBuf,
    collection: String,
) -> IngestResult<usize> {
    let content = fs::read_to_string(&path)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let session_text = parse_codex_jsonl(&content);

    let mut session_cfg = cfg.clone();
    session_cfg.collection = collection;

    let url = format!("file://{}", path.display());
    let title = path.file_name().and_then(|n| n.to_str());
    embed_with_retry(&session_cfg, &session_text, &url, "codex_session", title).await
}

/// Extract session text from Codex JSONL (pure, no I/O).
pub(crate) fn parse_codex_jsonl(content: &str) -> String {
    let mut session_text = String::new();
    for line in content.lines() {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if val["type"] != "response_item" {
            continue;
        }
        let role = val["payload"]["role"].as_str().unwrap_or("unknown");
        if let Some(arr) = val["payload"]["content"].as_array() {
            let mut combined = String::new();
            for item in arr {
                if let Some(t) = item["text"].as_str() {
                    combined.push_str(t);
                    combined.push('\n');
                } else if let Some(t) = item["input_text"].as_str() {
                    combined.push_str(t);
                    combined.push('\n');
                }
            }
            if !combined.trim().is_empty() {
                session_text.push_str(&format!("\n\n### {}:\n{}", role.to_uppercase(), combined));
            }
        }
    }
    session_text
}

#[cfg(test)]
mod tests {
    use super::parse_codex_jsonl;

    // --- parse_codex_jsonl ---

    #[test]
    fn parse_valid_codex_jsonl_text_field() {
        let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"How do I use async/await?\"}]}}\n\
                     {\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Use the async keyword.\"}]}}";
        let result = parse_codex_jsonl(jsonl);
        assert!(result.contains("### USER:"));
        assert!(result.contains("How do I use async/await?"));
        assert!(result.contains("### ASSISTANT:"));
        assert!(result.contains("Use the async keyword."));
    }

    #[test]
    fn parse_valid_codex_jsonl_input_text_field() {
        // input_text is the alternate key name for user input blocks
        let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"input_text\":\"Explain ownership\"}]}}";
        let result = parse_codex_jsonl(jsonl);
        assert!(result.contains("Explain ownership"));
    }

    #[test]
    fn parse_codex_jsonl_skips_non_response_item_types() {
        let jsonl = "{\"type\":\"session_start\",\"payload\":{\"id\":\"sess-abc\"}}\n\
                     {\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Hello!\"}]}}\n\
                     {\"type\":\"session_end\",\"payload\":{}}";
        let result = parse_codex_jsonl(jsonl);
        assert!(!result.contains("sess-abc"));
        assert!(result.contains("Hello!"));
    }

    #[test]
    fn parse_codex_jsonl_malformed_lines_no_panic() {
        let jsonl = "this is not json\n\
                     {\"incomplete\":\n\
                     {\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"Valid\"}]}}";
        let result = parse_codex_jsonl(jsonl);
        assert!(result.contains("Valid"));
    }

    #[test]
    fn parse_codex_jsonl_empty_input_returns_empty() {
        assert!(parse_codex_jsonl("").trim().is_empty());
    }

    #[test]
    fn parse_codex_jsonl_multiple_blocks_concatenated() {
        let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Part A. \"},{\"text\":\"Part B.\"}]}}";
        let result = parse_codex_jsonl(jsonl);
        assert!(result.contains("Part A."));
        assert!(result.contains("Part B."));
    }

    #[test]
    fn parse_codex_jsonl_whitespace_only_content_skipped() {
        let jsonl = "{\"type\":\"response_item\",\"payload\":{\"role\":\"user\",\"content\":[{\"text\":\"   \"}]}}\n\
                     {\"type\":\"response_item\",\"payload\":{\"role\":\"assistant\",\"content\":[{\"text\":\"Answer\"}]}}";
        let result = parse_codex_jsonl(jsonl);
        assert!(!result.contains("### USER:"));
        assert!(result.contains("Answer"));
    }

    #[test]
    fn parse_codex_jsonl_unknown_role_falls_back_to_unknown() {
        let jsonl =
            "{\"type\":\"response_item\",\"payload\":{\"content\":[{\"text\":\"Mystery\"}]}}";
        let result = parse_codex_jsonl(jsonl);
        assert!(result.contains("### UNKNOWN:"));
        assert!(result.contains("Mystery"));
    }
}
