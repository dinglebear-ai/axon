use super::{
    IngestResult, SessionDoc, SessionMeta, expand_home, flatten_session_result,
    matches_project_filter, resolve_collection,
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

pub(crate) struct ParsedClaudeSession {
    pub(crate) text: String,
    pub(crate) turn_count: u32,
    pub(crate) model: Option<String>,
    pub(crate) has_tool_use: bool,
    pub(crate) tools_used: Vec<String>,
    pub(crate) workspace_path: Option<String>,
    pub(crate) git_branch: Option<String>,
    pub(crate) last_message_at: Option<String>,
}

pub(super) async fn collect_claude_docs(
    cfg: &Config,
    multi: &MultiProgress,
) -> IngestResult<Vec<SessionDoc>> {
    let root = expand_home("~/.claude/projects");
    if !fs::try_exists(&root).await.unwrap_or(false) {
        return Ok(vec![]);
    }

    let pb = multi.add(ProgressBar::new_spinner());
    pb.set_style(
        ProgressStyle::default_spinner()
            .template("{spinner:.cyan} Claude: {msg}")
            .unwrap(),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut docs: Vec<SessionDoc> = Vec::new();
    let mut read_dir = fs::read_dir(root).await?;
    let mut futures = FuturesUnordered::new();

    while let Some(entry) = read_dir.next_entry().await? {
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
        // Resolve actual project path and git remote once per project dir.
        let project_path_opt = super::decode_claude_project_path(project_dir_name);
        let gh_repo = match project_path_opt {
            Some(ref pp) => super::read_git_remote_origin(pp).await,
            None => None,
        };
        let project_path_str = project_path_opt.map(|p| p.to_string_lossy().into_owned());

        let mut sub_read = fs::read_dir(&path).await?;
        while let Some(sub_entry) = sub_read.next_entry().await? {
            let sub_path = sub_entry.path();
            if sub_path.extension().is_none_or(|ext| ext != "jsonl") {
                continue;
            }
            let meta = fs::metadata(&sub_path).await?;
            let mtime = meta.modified()?;

            let coll_clone = collection.clone();
            let session_meta = SessionMeta {
                agent: "claude",
                project_name: clean_name.clone(),
                project_path: project_path_str.clone(),
                gh_repo: gh_repo.clone(),
            };
            futures.push(tokio::spawn(async move {
                parse_claude_file(sub_path, coll_clone, mtime, session_meta).await
            }));

            // drain backpressure to avoid unbounded future accumulation
            if futures.len() >= 64
                && let Some(res) = futures.next().await
                && let Some(doc) = flatten_session_result(res, "Claude")
            {
                docs.push(doc);
            }
        }
    }

    while let Some(res) = futures.next().await {
        if let Some(doc) = flatten_session_result(res, "Claude") {
            docs.push(doc);
        }
    }

    pb.finish_with_message(format!("scanned {} files", docs.len()));
    Ok(docs)
}

async fn parse_claude_file(
    path: PathBuf,
    collection: String,
    mtime: SystemTime,
    session_meta: SessionMeta,
) -> IngestResult<Option<SessionDoc>> {
    let content = super::read_session_file_limited(&path).await?;
    let parsed = parse_claude_jsonl(&content);
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
        "project_path": session_meta.project_path,
        "gh_repo": session_meta.gh_repo,
        "session_id": session_id,
        "session_date": mtime_chrono.to_rfc3339(),
        "turn_count": parsed.turn_count,
        "model": parsed.model,
        "has_tool_use": parsed.has_tool_use,
        "tools_used": parsed.tools_used,
        "workspace_path": parsed.workspace_path,
        "git_branch": parsed.git_branch,
        "last_message_at": parsed.last_message_at,
    });
    let doc = PreparedDoc {
        url,
        domain: "local".to_string(),
        chunks,
        source_type: "claude_session".to_string(),
        content_type: "text",
        title,
        extra: Some(extra),
        extractor_name: None,
        structured: None,
    };
    Ok(Some(SessionDoc { doc, collection }))
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

/// Extract session text and metadata from Claude JSONL (pure, no I/O).
pub(crate) fn parse_claude_jsonl(content: &str) -> ParsedClaudeSession {
    let mut session_text = String::new();
    let mut turn_count: u32 = 0;
    let mut model: Option<String> = None;
    let mut has_tool_use = false;
    let mut tools_used: HashSet<String> = HashSet::new();
    let mut workspace_path: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut last_message_at: Option<String> = None;

    for line in content.lines() {
        let Ok(val) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        // Capture workspace metadata from any line (first occurrence wins).
        if workspace_path.is_none() {
            workspace_path = val["cwd"].as_str().map(str::to_string);
        }
        if git_branch.is_none() {
            git_branch = val["gitBranch"].as_str().map(str::to_string);
        }

        // Skip meta-only lines (isMeta: true) for content / turn counting.
        if val["isMeta"].as_bool().unwrap_or(false) {
            continue;
        }

        let role = if val["type"] == "user" {
            "user"
        } else if val["type"] == "assistant" {
            "assistant"
        } else {
            continue;
        };

        // Track timestamp of the last user/assistant message.
        if let Some(ts) = val["timestamp"].as_str() {
            last_message_at = Some(ts.to_string());
        }

        // Extract model from the first assistant message that carries it.
        if role == "assistant" && model.is_none() {
            model = val["message"]["model"].as_str().map(str::to_string);
        }

        let msg_content = &val["message"]["content"];
        let text = if msg_content.is_string() {
            msg_content.as_str().unwrap().to_string()
        } else if let Some(arr) = msg_content.as_array() {
            let mut combined = String::new();
            for item in arr {
                if item["type"].as_str() == Some("tool_use") {
                    has_tool_use = true;
                    if let Some(name) = item["name"].as_str() {
                        tools_used.insert(name.to_string());
                    }
                }
                if let Some(t) = item["text"].as_str() {
                    combined.push_str(&super::redact_session_text(t));
                    combined.push('\n');
                }
            }
            combined
        } else {
            continue;
        };

        if !text.trim().is_empty() {
            session_text.push_str(&format!(
                "\n\n### {}:\n{}",
                role.to_uppercase(),
                super::redact_session_text(&text)
            ));
            if role == "user" {
                turn_count += 1;
            }
        }
    }

    let mut tools_list: Vec<String> = tools_used.into_iter().collect();
    tools_list.sort();

    ParsedClaudeSession {
        text: session_text,
        turn_count,
        model,
        has_tool_use,
        tools_used: tools_list,
        workspace_path,
        git_branch,
        last_message_at,
    }
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
        assert!(result.text.contains("### USER:"));
        assert!(result.text.contains("Hello?"));
        assert!(result.text.contains("### ASSISTANT:"));
        assert!(result.text.contains("Sure!"));
    }

    #[test]
    fn parse_valid_claude_jsonl_array_content() {
        let jsonl = "{\"type\":\"user\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"What is Rust?\"}]}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"A systems language.\"}]}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(result.text.contains("What is Rust?"));
        assert!(result.text.contains("A systems language."));
    }

    #[test]
    fn parse_claude_jsonl_skips_unknown_type() {
        let jsonl = "{\"type\":\"system\",\"message\":{\"content\":\"Hidden\"}}\n\
                     {\"type\":\"user\",\"message\":{\"content\":\"Visible\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(!result.text.contains("Hidden"));
        assert!(result.text.contains("Visible"));
    }

    #[test]
    fn parse_claude_jsonl_malformed_lines_no_panic() {
        let jsonl = "not valid json\n\
                     {\"broken\":\n\
                     {\"type\":\"user\",\"message\":{\"content\":\"Fine\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(result.text.contains("Fine"));
    }

    #[test]
    fn parse_claude_jsonl_empty_input_returns_empty() {
        assert!(parse_claude_jsonl("").text.trim().is_empty());
    }

    #[test]
    fn parse_claude_jsonl_whitespace_only_content_skipped() {
        let jsonl = "{\"type\":\"user\",\"message\":{\"content\":\"   \"}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":\"Real\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(!result.text.contains("### USER:"));
        assert!(result.text.contains("Real"));
    }

    #[test]
    fn parse_claude_jsonl_missing_content_field_skipped() {
        let jsonl = "{\"type\":\"user\",\"message\":{}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":\"OK\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(!result.text.contains("### USER:"));
        assert!(result.text.contains("OK"));
    }

    #[test]
    fn parse_claude_jsonl_turn_count_counts_user_messages() {
        let jsonl = "{\"type\":\"user\",\"message\":{\"content\":\"Q1\"}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":\"A1\"}}\n\
                     {\"type\":\"user\",\"message\":{\"content\":\"Q2\"}}\n\
                     {\"type\":\"assistant\",\"message\":{\"content\":\"A2\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert_eq!(result.turn_count, 2);
    }

    #[test]
    fn parse_claude_jsonl_model_extracted_from_assistant() {
        let jsonl = "{\"type\":\"assistant\",\"message\":{\"model\":\"claude-sonnet-4-6\",\"content\":\"Hello\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert_eq!(result.model.as_deref(), Some("claude-sonnet-4-6"));
    }

    #[test]
    fn parse_claude_jsonl_tool_use_detected() {
        let jsonl = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"tool_use\",\"name\":\"Glob\",\"input\":{}},{\"type\":\"text\",\"text\":\"Done\"}]}}";
        let result = parse_claude_jsonl(jsonl);
        assert!(result.has_tool_use);
        assert!(result.tools_used.contains(&"Glob".to_string()));
    }

    #[test]
    fn parse_claude_jsonl_tools_used_is_sorted_and_deduplicated() {
        let jsonl = "{\"type\":\"assistant\",\"message\":{\"content\":[\
            {\"type\":\"tool_use\",\"name\":\"Read\"},\
            {\"type\":\"tool_use\",\"name\":\"Glob\"},\
            {\"type\":\"tool_use\",\"name\":\"Read\"},\
            {\"type\":\"text\",\"text\":\"done\"}\
        ]}}";
        let result = parse_claude_jsonl(jsonl);
        assert_eq!(result.tools_used, vec!["Glob", "Read"]);
    }

    #[test]
    fn parse_claude_jsonl_workspace_and_branch_extracted() {
        let jsonl = "{\"type\":\"user\",\"cwd\":\"/home/user/project\",\"gitBranch\":\"main\",\"message\":{\"content\":\"Hi\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert_eq!(result.workspace_path.as_deref(), Some("/home/user/project"));
        assert_eq!(result.git_branch.as_deref(), Some("main"));
    }

    #[test]
    fn parse_claude_jsonl_last_message_at_is_latest_timestamp() {
        let jsonl = "{\"type\":\"user\",\"timestamp\":\"2024-01-01T10:00:00Z\",\"message\":{\"content\":\"Hi\"}}\n\
                     {\"type\":\"assistant\",\"timestamp\":\"2024-01-01T10:00:05Z\",\"message\":{\"content\":\"Hello\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert_eq!(
            result.last_message_at.as_deref(),
            Some("2024-01-01T10:00:05Z")
        );
    }

    #[test]
    fn parse_claude_jsonl_meta_lines_skipped_for_turns() {
        let jsonl = "{\"type\":\"user\",\"isMeta\":true,\"message\":{\"content\":\"meta\"}}\n\
                     {\"type\":\"user\",\"message\":{\"content\":\"real\"}}";
        let result = parse_claude_jsonl(jsonl);
        assert_eq!(result.turn_count, 1);
        assert!(!result.text.contains("meta"));
        assert!(result.text.contains("real"));
    }
}
