use super::{
    IngestResult, SessionStateTracker, embed_with_retry, expand_home, handle_spawn_result,
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

pub(super) async fn ingest_claude_sessions(
    cfg: &Config,
    state: &SessionStateTracker,
    multi: &MultiProgress,
) -> IngestResult<usize> {
    let root = expand_home("~/.claude/projects");
    if !fs::try_exists(&root).await.unwrap_or(false) {
        return Ok(0);
    }

    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} Claude: {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut total = 0;
    let mut read_dir = fs::read_dir(root)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut futures = FuturesUnordered::new();

    while let Some(entry) = read_dir
        .next_entry()
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?
    {
        let path = entry.path();
        if !entry.file_type().await?.is_dir() {
            continue;
        }
        let project_dir_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");
        let clean_name = clean_claude_project_name(project_dir_name);
        if !matches_project_filter(cfg, &clean_name) {
            continue;
        }

        let collection = resolve_collection(cfg, &clean_name);
        let mut sub_read = fs::read_dir(&path)
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        while let Some(sub_entry) = sub_read
            .next_entry()
            .await
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
        {
            let sub_path = sub_entry.path();
            if sub_path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            let meta = fs::metadata(&sub_path)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            let mtime = meta
                .modified()
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            if state.should_skip(&sub_path, mtime, meta.len()).await {
                continue;
            }

            let cfg_clone = cfg.clone();
            let coll_clone = collection.clone();
            let size = meta.len();
            futures.push(tokio::spawn(async move {
                let res = process_claude_file(&cfg_clone, sub_path.clone(), coll_clone).await;
                (sub_path, mtime, size, res)
            }));

            if futures.len() >= 32
                && let Some(res) = futures.next().await
            {
                total += handle_spawn_result(res, state, "Claude").await;
            }
        }
    }

    while let Some(res) = futures.next().await {
        total += handle_spawn_result(res, state, "Claude").await;
    }

    pb.finish_with_message(format!("indexed {} chunks", total));
    Ok(total)
}

fn clean_claude_project_name(dir_name: &str) -> String {
    if !dir_name.contains('-') {
        return dir_name.to_string();
    }
    let parts: Vec<&str> = dir_name.trim_start_matches('-').split('-').collect();
    if parts.len() >= 2 {
        let last = parts.last().unwrap();
        let prev = parts[parts.len() - 2];
        if matches!(*last, "rust" | "rs" | "git" | "main" | "master" | "src") {
            format!("{}-{}", prev, last)
        } else {
            last.to_string()
        }
    } else {
        parts.last().unwrap_or(&dir_name).to_string()
    }
}

async fn process_claude_file(
    cfg: &Config,
    path: PathBuf,
    collection: String,
) -> IngestResult<usize> {
    let content = fs::read_to_string(&path)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let session_text = parse_claude_jsonl(&content);

    let mut session_cfg = cfg.clone();
    session_cfg.collection = collection;

    let url = format!("file://{}", path.display());
    let title = path.file_name().and_then(|n| n.to_str());
    embed_with_retry(&session_cfg, &session_text, &url, "claude_session", title).await
}

/// Extract session text from Claude JSONL (pure, no I/O).
pub(crate) fn parse_claude_jsonl(content: &str) -> String {
    let mut session_text = String::new();
    for line in content.lines() {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let role = if val["type"] == "user" {
            "user"
        } else if val["type"] == "assistant" {
            "assistant"
        } else {
            continue;
        };
        let msg_content = &val["message"]["content"];
        let text = if msg_content.is_string() {
            msg_content.as_str().unwrap().to_string()
        } else if let Some(arr) = msg_content.as_array() {
            let mut combined = String::new();
            for item in arr {
                if let Some(t) = item["text"].as_str() {
                    combined.push_str(t);
                    combined.push('\n');
                }
            }
            combined
        } else {
            continue;
        };
        if !text.trim().is_empty() {
            session_text.push_str(&format!("\n\n### {}:\n{}", role.to_uppercase(), text));
        }
    }
    session_text
}

#[cfg(test)]
mod tests {
    use super::{clean_claude_project_name, parse_claude_jsonl};

    // --- clean_claude_project_name ---

    #[test]
    fn clean_name_no_hyphen_returns_as_is() {
        assert_eq!(clean_claude_project_name("myproject"), "myproject");
        assert_eq!(clean_claude_project_name("axon"), "axon");
    }

    #[test]
    fn clean_name_non_special_last_segment_returned() {
        // "foo-bar": last="bar", not a known suffix, so returns "bar"
        assert_eq!(clean_claude_project_name("foo-bar"), "bar");
    }

    #[test]
    fn clean_name_known_suffix_rust() {
        // last="rust" is a known suffix, prev="axon", so returns "axon-rust"
        assert_eq!(
            clean_claude_project_name("workspace-axon-rust"),
            "axon-rust"
        );
    }

    #[test]
    fn clean_name_known_suffix_rs() {
        assert_eq!(
            clean_claude_project_name("home-jmagar-myapp-rs"),
            "myapp-rs"
        );
    }

    #[test]
    fn clean_name_known_suffix_git() {
        assert_eq!(clean_claude_project_name("project-repo-git"), "repo-git");
    }

    #[test]
    fn clean_name_known_suffix_main() {
        assert_eq!(
            clean_claude_project_name("org-service-main"),
            "service-main"
        );
    }

    #[test]
    fn clean_name_leading_hyphen_stripped_before_split() {
        // trim_start_matches('-') strips leading hyphens before splitting
        assert_eq!(clean_claude_project_name("-home-jmagar-axon"), "axon");
    }

    // --- parse_claude_jsonl ---

    #[test]
    fn parse_valid_claude_jsonl_string_content() {
        let jsonl = "{\"type\":\"user\",\"message\":{\"content\":\"Hello?\"}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":\"Sure!\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(result.contains("### USER:"));
        assert!(result.contains("Hello?"));
        assert!(result.contains("### ASSISTANT:"));
        assert!(result.contains("Sure!"));
    }

    #[test]
    fn parse_valid_claude_jsonl_array_content() {
        let jsonl = "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"What is Rust?\"}]}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"A systems language.\"}]}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(result.contains("What is Rust?"));
        assert!(result.contains("A systems language."));
    }

    #[test]
    fn parse_claude_jsonl_skips_unknown_type() {
        let jsonl = "{\"type\":\"system\",\"message\":{\"content\":\"Hidden\"}}\n\
                     {\"type\":\"user\",\"message\":{\"content\":\"Visible\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(!result.contains("Hidden"));
        assert!(result.contains("Visible"));
    }

    #[test]
    fn parse_claude_jsonl_malformed_lines_no_panic() {
        let jsonl = "not valid json\n\
                     {\"broken\":\n\
                     {\"type\":\"user\",\"message\":{\"content\":\"Fine\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(result.contains("Fine"));
    }

    #[test]
    fn parse_claude_jsonl_empty_input_returns_empty() {
        assert!(parse_claude_jsonl("").trim().is_empty());
    }

    #[test]
    fn parse_claude_jsonl_whitespace_only_content_skipped() {
        let jsonl = "{\"type\":\"user\",\"message\":{\"content\":\"   \"}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":\"Real\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(!result.contains("### USER:"));
        assert!(result.contains("Real"));
    }

    #[test]
    fn parse_claude_jsonl_missing_content_field_skipped() {
        let jsonl = "{\"type\":\"user\",\"message\":{}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":\"OK\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(!result.contains("### USER:"));
        assert!(result.contains("OK"));
    }
}
