use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use reqwest::Url;

use crate::core::config::Config;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info, log_warn};
use crate::ingest::git_files::collect_repo_files;
use crate::ingest::git_payload::{ContentKind, GitPayload, build_git_payload};
use crate::ingest::progress::PhaseReporter;
use crate::ingest::subprocess::{
    MAX_INGEST_FILE_BYTES, SUBPROCESS_TIMEOUT, run_command_with_timeout,
};
use crate::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use crate::vector::ops::input::code::chunk_code;
use crate::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericGitTarget {
    pub clone_url: String,
    pub web_url: String,
    pub host: String,
    pub name: String,
}

pub fn normalize_generic_git_target(input: &str) -> Result<String> {
    Ok(parse_generic_git_target(input)?.clone_url)
}

pub fn parse_generic_git_target(input: &str) -> Result<GenericGitTarget> {
    let raw = input.trim();
    let raw = raw.strip_prefix("git:").unwrap_or(raw).trim();
    let url = Url::parse(raw)?;
    if url.scheme() != "https" {
        bail!("generic git ingest requires an https clone URL");
    }
    validate_url(url.as_str())?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("generic git target is missing host"))?
        .to_ascii_lowercase();
    let path = url.path().trim_matches('/');
    if path.is_empty() {
        bail!("generic git target is missing repository path");
    }
    let name = path
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".git")
        .to_string();
    let web_url = url.as_str().trim_end_matches(".git").to_string();
    Ok(GenericGitTarget {
        clone_url: url.to_string(),
        web_url,
        host,
        name,
    })
}

async fn clone_repo(target: &GenericGitTarget) -> Result<tempfile::TempDir> {
    let tmp = tempfile::tempdir()?;
    let tmp_path = tmp.path().to_string_lossy().to_string();
    let mut command = tokio::process::Command::new("git");
    command
        .args(["clone", "--depth=1", "--", &target.clone_url, &tmp_path])
        .env("GIT_TERMINAL_PROMPT", "0");
    let output = run_command_with_timeout(command, SUBPROCESS_TIMEOUT, "generic git clone").await?;
    if output.status.success() {
        return Ok(tmp);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    bail!("git clone failed for {}: {}", target.clone_url, stderr);
}

async fn current_branch(repo_root: &Path) -> Option<String> {
    let mut command = tokio::process::Command::new("git");
    command.args([
        "-C",
        &repo_root.to_string_lossy(),
        "rev-parse",
        "--abbrev-ref",
        "HEAD",
    ]);
    let output = run_command_with_timeout(command, SUBPROCESS_TIMEOUT, "git branch")
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

pub async fn ingest_generic_git(
    cfg: &Config,
    target: &str,
    include_source: bool,
    reporter: PhaseReporter,
) -> Result<usize> {
    ingest_git_repository(cfg, target, include_source, reporter, "git", "git").await
}

pub(crate) async fn ingest_git_repository(
    cfg: &Config,
    target: &str,
    include_source: bool,
    reporter: PhaseReporter,
    source_type: &str,
    provider: &str,
) -> Result<usize> {
    log_info(&format!(
        "command=ingest source={source_type} target={target}"
    ));
    let target = parse_generic_git_target(target)?;
    reporter
        .report(serde_json::json!({"phase": "cloning", "repo": target.clone_url}))
        .await;
    let tmp = clone_repo(&target).await?;
    let branch = current_branch(tmp.path())
        .await
        .unwrap_or_else(|| "HEAD".to_string());
    let files = collect_repo_files(tmp.path(), include_source).await?;
    let total = files.len();
    let mut docs = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        if let Some(doc) =
            file_doc(tmp.path(), &target, &branch, file, source_type, provider).await?
        {
            docs.push(doc);
        }
        if (index + 1) % 25 == 0 || index + 1 == total {
            reporter
                .report(serde_json::json!({"files_done": index + 1, "files_total": total}))
                .await;
        }
    }
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{e}"))?;
    reporter
        .report(serde_json::json!({
            "phase": "completed",
            "files_done": total,
            "files_total": total,
            "chunks_embedded": summary.chunks_embedded,
        }))
        .await;
    log_done(&format!(
        "command=ingest source={source_type} target={} chunk_count={}",
        target.clone_url, summary.chunks_embedded
    ));
    Ok(summary.chunks_embedded)
}

async fn file_doc(
    root: &Path,
    target: &GenericGitTarget,
    branch: &str,
    file: PathBuf,
    source_type: &str,
    provider: &str,
) -> Result<Option<PreparedDoc>> {
    let rel = file
        .strip_prefix(root)?
        .to_string_lossy()
        .replace('\\', "/");

    // S-M2: per-file size cap — skip oversized files rather than OOM-ing
    match tokio::fs::metadata(&file).await {
        Ok(meta) if meta.len() > MAX_INGEST_FILE_BYTES => {
            log_warn(&format!(
                "command=ingest_git skip_large_file path={rel} size_bytes={}",
                meta.len()
            ));
            return Ok(None);
        }
        Err(e) => {
            log_warn(&format!(
                "command=ingest_git stat_failed path={rel} err={e}"
            ));
            return Ok(None);
        }
        _ => {}
    }

    // Separate I/O error (hard failure) from non-UTF-8 (benign skip)
    let bytes = match tokio::fs::read(&file).await {
        Ok(b) => b,
        Err(e) => {
            log_warn(&format!(
                "command=ingest_git read_failed path={rel} err={e}"
            ));
            return Ok(None);
        }
    };
    let content = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            log_warn(&format!("command=ingest_git skip_non_utf8 path={rel}"));
            return Ok(None);
        }
    };

    // Q-H1: tree-sitter AST-aware chunking with extension routing + text fallback
    let ext = path_extension(&rel).to_ascii_lowercase();
    let chunks = tokio::task::spawn_blocking({
        let content = content.clone();
        let ext = ext.clone();
        move || chunk_code(&content, &ext).unwrap_or_else(|| chunk_text(&content))
    })
    .await
    .unwrap_or_else(|_| chunk_text(&content));
    if chunks.is_empty() {
        return Ok(None);
    }
    let extra = build_git_payload(&GitPayload {
        provider: provider.to_string(),
        host: target.host.clone(),
        owner: None,
        repo: target.name.clone(),
        content_kind: ContentKind::File,
        branch: Some(branch.to_string()),
        file_path: Some(rel.clone()),
        file_language: Some(language_name(path_extension(&rel)).to_string()),
        file_type: Some(classify_file_type(&rel).to_string()),
        file_is_test: Some(is_test_path(&rel)),
        meta: Some(serde_json::json!({ "clone_url": target.clone_url })),
        ..GitPayload::default()
    });
    Ok(Some(PreparedDoc::ingest(
        format!("{}#{}:{}", target.web_url, branch, rel),
        target.host.clone(),
        chunks,
        source_type,
        Some(rel.clone()),
        Some(extra),
    )))
}

#[cfg(test)]
#[path = "generic_git_tests.rs"]
mod tests;
