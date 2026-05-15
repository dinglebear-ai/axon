use crate::core::config::Config;
use crate::core::paths::{axon_data_base_dir, ensure_private_dir};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::io::Write;
use std::path::PathBuf;

const DEFAULT_SESSION: &str = "default";
const MAX_SESSION_NAME_LEN: usize = 64;
const MAX_FOLLOW_UP_TURNS: usize = 6;
const MAX_TURN_CHARS: usize = 8_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct AskTurn {
    pub schema: String,
    pub created_at: DateTime<Utc>,
    pub collection: String,
    pub user: String,
    pub assistant: String,
}

pub(crate) fn selected_session_name(cfg: &Config) -> String {
    cfg.ask_session
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(DEFAULT_SESSION)
        .to_string()
}

pub(crate) fn reset_session(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let path = session_path(cfg)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!("failed to reset ask session {}: {err}", path.display()).into()),
    }
}

pub(crate) fn follow_up_query(
    cfg: &Config,
    question: &str,
) -> Result<Option<String>, Box<dyn Error>> {
    let turns = load_recent_turns(cfg, MAX_FOLLOW_UP_TURNS)?;
    if turns.is_empty() {
        return Ok(None);
    }

    let mut out = String::new();
    out.push_str("This is a follow-up question in a local Axon ask session.\n");
    out.push_str("Use the previous conversation only to resolve references, intent, and scope.\n");
    out.push_str(
        "Still ground factual claims in the retrieved source context and cite sources.\n\n",
    );
    out.push_str("Previous conversation:\n");
    for turn in turns {
        out.push_str("User: ");
        out.push_str(&bounded(&turn.user, MAX_TURN_CHARS));
        out.push_str("\nAssistant: ");
        out.push_str(&bounded(&turn.assistant, MAX_TURN_CHARS));
        out.push_str("\n\n");
    }
    out.push_str("Current follow-up question: ");
    out.push_str(question);
    Ok(Some(out))
}

pub(crate) fn append_turn(
    cfg: &Config,
    user: &str,
    assistant: &str,
) -> Result<PathBuf, Box<dyn Error>> {
    let path = session_path(cfg)?;
    if let Some(parent) = path.parent() {
        ensure_private_dir(parent)?;
    }
    let event = AskTurn {
        schema: "axon.ask.turn.v1".to_string(),
        created_at: Utc::now(),
        collection: cfg.collection.clone(),
        user: user.to_string(),
        assistant: assistant.to_string(),
    };
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    writeln!(file, "{}", serde_json::to_string(&event)?)?;
    Ok(path)
}

fn load_recent_turns(cfg: &Config, limit: usize) -> Result<Vec<AskTurn>, Box<dyn Error>> {
    let path = session_path(cfg)?;
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    let mut turns = content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(serde_json::from_str::<AskTurn>)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|err| format!("failed to read ask session {}: {err}", path.display()))?;
    let keep_from = turns.len().saturating_sub(limit);
    Ok(turns.split_off(keep_from))
}

fn session_path(cfg: &Config) -> Result<PathBuf, Box<dyn Error>> {
    let session = sanitize_session_name(&selected_session_name(cfg))?;
    Ok(axon_data_base_dir()
        .join("ask-sessions")
        .join(format!("{session}.jsonl")))
}

fn sanitize_session_name(name: &str) -> Result<String, Box<dyn Error>> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_SESSION.to_string());
    }
    if trimmed.len() > MAX_SESSION_NAME_LEN {
        return Err(format!("ask session name is longer than {MAX_SESSION_NAME_LEN} chars").into());
    }
    let safe = trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'));
    if !safe || trimmed == "." || trimmed == ".." {
        return Err(
            "ask session name may contain only ASCII letters, numbers, '.', '-' and '_'".into(),
        );
    }
    Ok(trimmed.to_string())
}

fn bounded(text: &str, max_chars: usize) -> String {
    let mut out: String = text.chars().take(max_chars).collect();
    if text.chars().count() > max_chars {
        out.push_str("\n[truncated]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{bounded, sanitize_session_name};

    #[test]
    fn sanitize_session_name_accepts_safe_names() {
        assert_eq!(
            sanitize_session_name("rust.tests-1").unwrap(),
            "rust.tests-1"
        );
    }

    #[test]
    fn sanitize_session_name_rejects_path_like_names() {
        assert!(sanitize_session_name("../secret").is_err());
        assert!(sanitize_session_name("bad/name").is_err());
        assert!(sanitize_session_name("..").is_err());
    }

    #[test]
    fn bounded_truncates_by_chars() {
        assert_eq!(bounded("abcdef", 3), "abc\n[truncated]");
        assert_eq!(bounded("abc", 3), "abc");
    }
}
