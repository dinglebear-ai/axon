use std::path::Path;

use anyhow::{Result, bail};
use base64::Engine as _;

use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::ingest::git_files::embed_docs;
use crate::ingest::progress::PhaseReporter;
use crate::ingest::subprocess::MAX_INGEST_FILE_BYTES;
use crate::ingest::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};
use crate::vector::ops::PreparedDoc;
use crate::vector::ops::file_ingest::{
    SelectionPolicy, chunk_file, chunking_method, collect_files,
};
use crate::vector::ops::input::code::code_symbol_extraction_status;

use super::embed::gitlab_file_chunk_payload;
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
    let files = collect_files(tmp.path(), SelectionPolicy::Allowlist { include_source })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
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
        // Move content + ext into spawn_blocking; return content + ext so
        // code_symbol_extraction_status can run after on the calling thread.
        let (code_chunks, content, ext) = match tokio::task::spawn_blocking(move || {
            let chunks = chunk_file(&content, &ext);
            (chunks, content, ext)
        })
        .await
        {
            Ok(triple) => triple,
            Err(e) => {
                tracing::error!(
                    namespace = %target.namespace_path,
                    path = %rel,
                    err = %e,
                    "ingest_gitlab: chunk_file task panicked"
                );
                continue;
            }
        };
        if code_chunks.is_empty() {
            continue;
        }
        let symbol_status = code_symbol_extraction_status(&content, &ext, &code_chunks);
        for (idx, chunk) in code_chunks.into_iter().enumerate() {
            let method = chunking_method(&ext, &chunk);
            docs.push(PreparedDoc::ingest(
                format!(
                    "{}/-/blob/{}/{}#L{}-L{}#{}",
                    target.web_url, branch, rel, chunk.start_line, chunk.end_line, idx
                ),
                target.host.clone(),
                vec![chunk.text.clone()],
                "gitlab",
                Some(rel.clone()),
                Some(gitlab_file_chunk_payload(
                    target,
                    project,
                    &rel,
                    branch,
                    &chunk,
                    method,
                    symbol_status,
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

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
