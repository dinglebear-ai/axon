use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine as _;

use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::ingest::git_files::{collect_repo_files, embed_docs};
use crate::ingest::progress::PhaseReporter;
use crate::ingest::subprocess::MAX_INGEST_FILE_BYTES;
use crate::ingest::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};
use crate::vector::ops::input::code::chunk_code;
use crate::vector::ops::{PreparedDoc, chunk_text};

use super::embed::gitlab_payload;
use super::types::{GitLabProject, GitLabTarget};

async fn clone_repo(
    cfg: &Config,
    target: &GitLabTarget,
    branch: &str,
) -> Result<tempfile::TempDir> {
    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path().to_string_lossy().to_string();
    let mut command = tokio::process::Command::new("git");
    if let Some(token) = cfg
        .gitlab_token
        .as_deref()
        .filter(|token| !token.is_empty())
    {
        // Pass auth via env vars so the token is never visible in `ps` output.
        // Mirrors the pattern used for GitHub wiki clones.
        let encoded = base64::engine::general_purpose::STANDARD.encode(format!("oauth2:{token}"));
        command
            .env("GIT_CONFIG_COUNT", "1")
            .env("GIT_CONFIG_KEY_0", "http.extraHeader")
            .env(
                "GIT_CONFIG_VALUE_0",
                format!("Authorization: Basic {encoded}"),
            );
    }
    command
        .args([
            "clone",
            "--depth=1",
            "--branch",
            branch,
            "--single-branch",
            "--",
            &target.clone_url,
            &tmp_path,
        ])
        .env("GIT_TERMINAL_PROMPT", "0");
    let output = run_command_with_timeout(command, SUBPROCESS_TIMEOUT, "gitlab git clone").await?;
    if output.status.success() {
        return Ok(tmp);
    }
    let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if let Some(token) = cfg
        .gitlab_token
        .as_deref()
        .filter(|token| !token.is_empty())
    {
        stderr = stderr.replace(token, "[redacted]");
    }
    bail!("git clone failed for {}: {}", target.namespace_path, stderr);
}

pub(crate) async fn embed_files(
    cfg: &Config,
    target: &GitLabTarget,
    project: &GitLabProject,
    include_source: bool,
    reporter: &PhaseReporter,
) -> Result<usize> {
    let branch = project.default_branch.as_deref().unwrap_or("main");
    reporter
        .report(serde_json::json!({"phase": "cloning", "repo": target.namespace_path}))
        .await;
    let tmp = clone_repo(cfg, target, branch).await?;
    let files = collect_repo_files(tmp.path(), include_source).await?;
    let total = files.len();
    let mut docs = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        let rel = file
            .strip_prefix(tmp.path())?
            .to_string_lossy()
            .replace('\\', "/");
        // S-M2: stat before read — skip files over the ingest size cap
        match tokio::fs::metadata(&file).await {
            Ok(meta) if meta.len() > MAX_INGEST_FILE_BYTES => {
                log_warn(&format!(
                    "command=ingest_gitlab skip_large_file path={rel} size_bytes={}",
                    meta.len()
                ));
                continue;
            }
            Err(e) => {
                log_warn(&format!(
                    "command=ingest_gitlab stat_failed path={rel} err={e}"
                ));
                continue;
            }
            _ => {}
        }
        // Separate I/O errors (hard) from non-UTF-8 (benign skip)
        let bytes = match tokio::fs::read(&file).await {
            Ok(b) => b,
            Err(e) => {
                log_warn(&format!(
                    "command=ingest_gitlab read_failed path={rel} err={e}"
                ));
                continue;
            }
        };
        let content = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                log_warn(&format!("command=ingest_gitlab skip_non_utf8 path={rel}"));
                continue;
            }
        };
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        // Move content into spawn_blocking — avoids cloning large files.
        let chunks = match tokio::task::spawn_blocking(move || {
            chunk_code(&content, &ext).unwrap_or_else(|| chunk_text(&content))
        })
        .await
        {
            Ok(chunks) => chunks,
            Err(e) => {
                log_warn(&format!(
                    "command=ingest_gitlab chunk_panicked path={rel} err={e}"
                ));
                vec![]
            }
        };
        if !chunks.is_empty() {
            docs.push(PreparedDoc::ingest(
                format!("{}/-/blob/{}/{}", target.web_url, branch, rel),
                target.host.clone(),
                chunks,
                "gitlab",
                Some(rel.clone()),
                Some(gitlab_payload(
                    target,
                    project,
                    "file",
                    serde_json::json!({"path": rel, "branch": branch}),
                )),
            ));
        }
        if (index + 1) % 25 == 0 || index + 1 == total {
            reporter
                .report(serde_json::json!({"files_done": index + 1, "files_total": total}))
                .await;
        }
    }
    let chunks = embed_docs(cfg, docs).await?;
    reporter
        .report(serde_json::json!({
            "files_done": total,
            "files_total": total,
            "chunks_embedded": chunks,
            "phase": "embedded_files",
        }))
        .await;
    Ok(chunks)
}
