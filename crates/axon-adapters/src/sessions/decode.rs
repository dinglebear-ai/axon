//! Pure session-transcript decoders. Ported from the legacy
//! `axon-ingest::sessions::{claude, codex, gemini}` parsers, minus the
//! live-filesystem scanning and progress reporting — these functions take
//! already-read file content and return extracted text + metadata.
//!
//! Format detection is by provider name (stamped by the caller from the
//! `session:<provider>:<id>` target) combined with file extension:
//! - `claude` / `codex`: JSONL, one JSON object per line
//! - `gemini`: a single JSON document with a `messages` array

use serde_json::Value;
use std::collections::HashSet;

/// Provider-agnostic decode result. `malformed_lines` counts JSONL lines (or,
/// for Gemini, malformed message entries) that failed to parse but did not
/// abort the decode — callers may treat a nonzero count on an otherwise-empty
/// result as a hard failure ("degraded" fixture), matching legacy behavior.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DecodedSession {
    pub text: String,
    pub turn_count: u32,
    pub model: Option<String>,
    pub has_tool_use: bool,
    pub tools_used: Vec<String>,
    pub workspace_path: Option<String>,
    pub git_branch: Option<String>,
    pub last_message_at: Option<String>,
    pub malformed_lines: u32,
}

/// Redact secret-shaped tokens from transcript text before it is embedded.
/// Ported verbatim (behaviorally) from `axon-ingest::sessions::redact_session_text`.
pub fn redact_session_text(input: &str) -> String {
    input
        .split_whitespace()
        .map(redact_session_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_session_token(token: &str) -> String {
    let trimmed = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-');
    let lower = trimmed.to_ascii_lowercase();
    let secret_like = lower.starts_with("sk-")
        || lower.starts_with("ghp_")
        || lower.starts_with("github_pat_")
        || lower.starts_with("atk_")
        || lower.contains("api_key")
        || lower.contains("apikey")
        || lower.contains("access_token")
        || (trimmed.len() >= 24
            && trimmed.chars().any(|c| c.is_ascii_alphabetic())
            && trimmed.chars().any(|c| c.is_ascii_digit()));
    if secret_like {
        token.replace(trimmed, "[redacted-secret]")
    } else {
        token.to_string()
    }
}

/// Decode a Claude Code session export (JSONL, one turn per line).
pub fn decode_claude_jsonl(content: &str) -> DecodedSession {
    let mut out = DecodedSession::default();
    let mut tools_used: HashSet<String> = HashSet::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let val = match serde_json::from_str::<Value>(line) {
            Ok(val) => val,
            Err(_) => {
                out.malformed_lines += 1;
                continue;
            }
        };

        if out.workspace_path.is_none() {
            out.workspace_path = val["cwd"].as_str().map(str::to_string);
        }
        if out.git_branch.is_none() {
            out.git_branch = val["gitBranch"].as_str().map(str::to_string);
        }
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

        if let Some(ts) = val["timestamp"].as_str() {
            out.last_message_at = Some(ts.to_string());
        }
        if role == "assistant" && out.model.is_none() {
            out.model = val["message"]["model"].as_str().map(str::to_string);
        }

        let msg_content = &val["message"]["content"];
        let text = extract_content_text(
            msg_content,
            &mut out.has_tool_use,
            &mut tools_used,
            "tool_use",
        );
        append_turn(&mut out, role, &text);
    }

    finalize(&mut out, tools_used);
    out
}

/// Decode a Codex session export (JSONL, `session_meta` header + `response_item` turns).
pub fn decode_codex_jsonl(content: &str) -> DecodedSession {
    let mut out = DecodedSession::default();
    let mut tools_used: HashSet<String> = HashSet::new();

    for line in content.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let val = match serde_json::from_str::<Value>(line) {
            Ok(val) => val,
            Err(_) => {
                out.malformed_lines += 1;
                continue;
            }
        };

        if val["type"] == "session_meta" {
            if out.workspace_path.is_none() {
                out.workspace_path = val["payload"]["cwd"].as_str().map(str::to_string);
            }
            if out.model.is_none() {
                out.model = val["payload"]["model"]
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
        let Some(arr) = val["payload"]["content"].as_array() else {
            continue;
        };
        let mut combined = String::new();
        for item in arr {
            let item_type = item["type"].as_str().unwrap_or("");
            if matches!(item_type, "function_call" | "tool_call" | "tool_use") {
                out.has_tool_use = true;
                let name = item["name"]
                    .as_str()
                    .or_else(|| item["function"]["name"].as_str());
                if let Some(n) = name {
                    tools_used.insert(n.to_string());
                }
            }
            if let Some(t) = item["text"].as_str() {
                combined.push_str(t);
                combined.push('\n');
            } else if let Some(t) = item["input_text"].as_str() {
                combined.push_str(t);
                combined.push('\n');
            }
        }
        append_turn(&mut out, role, &combined);
    }

    finalize(&mut out, tools_used);
    out
}

/// Decode a Gemini CLI chat export (single JSON document, `messages` array).
///
/// Returns `Err` when the content is not valid JSON at all (the legacy
/// behavior for `parse_gemini_json`); a well-formed document with no
/// recognizable messages decodes to an empty `DecodedSession` rather than
/// an error.
pub fn decode_gemini_json(content: &str) -> Result<DecodedSession, String> {
    let val: Value = serde_json::from_str(content).map_err(|err| err.to_string())?;
    let mut out = DecodedSession::default();

    if let Some(messages) = val["messages"].as_array() {
        for msg in messages {
            let role = msg["type"].as_str().unwrap_or("unknown");
            let Some(content_arr) = msg["content"].as_array() else {
                continue;
            };
            let mut combined = String::new();
            for item in content_arr {
                if let Some(t) = item["text"].as_str() {
                    combined.push_str(t);
                    combined.push('\n');
                }
            }
            append_turn(&mut out, role, &combined);
        }
    }
    Ok(out)
}

fn extract_content_text(
    msg_content: &Value,
    has_tool_use: &mut bool,
    tools_used: &mut HashSet<String>,
    tool_marker: &str,
) -> String {
    if let Some(text) = msg_content.as_str() {
        return text.to_string();
    }
    let Some(arr) = msg_content.as_array() else {
        return String::new();
    };
    let mut combined = String::new();
    for item in arr {
        if item["type"].as_str() == Some(tool_marker) {
            *has_tool_use = true;
            if let Some(name) = item["name"].as_str() {
                tools_used.insert(name.to_string());
            }
        }
        if let Some(t) = item["text"].as_str() {
            combined.push_str(t);
            combined.push('\n');
        }
    }
    combined
}

fn append_turn(out: &mut DecodedSession, role: &str, text: &str) {
    if text.trim().is_empty() {
        return;
    }
    out.text.push_str(&format!(
        "\n\n### {}:\n{}",
        role.to_uppercase(),
        redact_session_text(text)
    ));
    if role == "user" {
        out.turn_count += 1;
    }
}

fn finalize(out: &mut DecodedSession, tools_used: HashSet<String>) {
    let mut tools_list: Vec<String> = tools_used.into_iter().collect();
    tools_list.sort();
    out.tools_used = tools_list;
}

#[cfg(test)]
#[path = "decode_tests.rs"]
mod tests;
