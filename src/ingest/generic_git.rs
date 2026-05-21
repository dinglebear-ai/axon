use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use reqwest::Url;

use crate::core::config::Config;
use crate::core::http::validate_url;
use crate::core::logging::{log_done, log_info};
use crate::ingest::git_payload::{GitPayload, build_git_payload};
use crate::ingest::github::{is_indexable_doc_path, is_indexable_source_path};
use crate::ingest::progress::PhaseReporter;
use crate::ingest::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};
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

async fn collect_files(root: &Path, include_source: bool) -> Result<Vec<PathBuf>> {
    let mut dirs = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = dirs.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let file_type = entry.file_type().await?;
            if file_type.is_dir() {
                if entry.file_name() != ".git" {
                    dirs.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let rel = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if is_indexable_doc_path(&rel) || (include_source && is_indexable_source_path(&rel)) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
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
    let files = collect_files(tmp.path(), include_source).await?;
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
    let Ok(content) = tokio::fs::read_to_string(&file).await else {
        return Ok(None);
    };
    let chunks = chunk_text(&content);
    if chunks.is_empty() {
        return Ok(None);
    }
    let extra = build_git_payload(&GitPayload {
        provider: provider.to_string(),
        host: target.host.clone(),
        owner: None,
        repo: target.name.clone(),
        content_kind: "file",
        branch: Some(branch.to_string()),
        file_path: Some(rel.clone()),
        meta: Some(serde_json::json!({ "clone_url": target.clone_url })),
        ..GitPayload::default()
    });
    Ok(Some(PreparedDoc {
        url: format!("{}#{}:{}", target.web_url, branch, rel),
        domain: target.host.clone(),
        chunks,
        source_type: source_type.to_string(),
        content_type: "text",
        title: Some(rel.clone()),
        extra: Some(extra),
        extractor_name: None,
        structured: None,
    }))
}

#[cfg(test)]
#[path = "generic_git_tests.rs"]
mod tests;
