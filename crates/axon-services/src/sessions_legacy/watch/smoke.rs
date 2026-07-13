use super::validate::{SessionWatchRoots, validate_session_file_path};
use crate::sessions_legacy::checkpoint::checkpoint_success_exists_for_path_hash;
use anyhow::{Result, anyhow};
use axon_core::config::Config;
use serde::Serialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Serialize)]
pub struct SessionWatchSmokeReport {
    pub transcript_path: PathBuf,
    pub probe_text: String,
    pub ingested: bool,
    pub evidence: String,
}

pub async fn smoke_watch(
    cfg: &Config,
    pool: &sqlx::SqlitePool,
    timeout_secs: u64,
) -> Result<SessionWatchSmokeReport> {
    let root = crate::sessions_legacy::expand_home("~/.codex/sessions/axon-smoke-watch");
    std::fs::create_dir_all(&root)?;
    let probe_text = format!(
        "axon-session-watch-smoke-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let transcript_path = root.join(format!("smoke-{}.jsonl", std::process::id()));
    std::fs::write(
        &transcript_path,
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "role": "user",
                "content": [{ "type": "input_text", "text": probe_text }]
            }
        })
        .to_string()
            + "\n",
    )?;

    let roots = SessionWatchRoots::from_config(cfg)?;
    let validated = validate_session_file_path(&roots, &transcript_path)?;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout_secs.min(60));
    while tokio::time::Instant::now() < deadline {
        if checkpoint_success_exists_for_path_hash(pool, &validated.path_hash).await? {
            return Ok(SessionWatchSmokeReport {
                transcript_path,
                probe_text,
                ingested: true,
                evidence: "checkpoint_success".to_string(),
            });
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    Err(anyhow!(
        "session watch smoke probe was not ingested before timeout; verify session-watch-service is running"
    ))
}
