use std::collections::HashSet;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use reqwest::Url;

use crate::progress::PhaseReporter;
use crate::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};
use axon_core::config::Config;
use axon_core::content::redact_url;
use axon_core::http::validate_url;
use axon_core::logging::{log_done, log_info, log_warn};
use axon_vector::ops::embed_prepared_docs;
use axon_vector::ops::file_ingest::{SelectionPolicy, collect_files};
use axon_vector::ops::qdrant::qdrant_delete_repo_file_fragments;

mod file_docs;
pub(crate) use file_docs::file_docs;

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

#[cfg(test)]
pub(crate) fn git_ref_is_immutable_commit_sha(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
pub(crate) fn git_ref_schedules_refresh(reference: &str) -> bool {
    !git_ref_is_immutable_commit_sha(reference)
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
    let mut web = url.clone();
    let _ = web.set_username("");
    let _ = web.set_password(None);
    let web_url = web.as_str().trim_end_matches(".git").to_string();
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
    let stderr = redact_url(String::from_utf8_lossy(&output.stderr).trim());
    bail!("{}", git_clone_failed_message(target, &stderr));
}

fn git_clone_failed_message(target: &GenericGitTarget, stderr: &str) -> String {
    format!(
        "git clone failed for {}: {}",
        redact_url(&target.clone_url),
        stderr
    )
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
    let target = parse_generic_git_target(target)?;
    log_info(&format!(
        "command=ingest source={source_type} target={}",
        target.web_url
    ));
    reporter
        .report(serde_json::json!({"phase": "cloning", "repo": target.web_url}))
        .await;
    let tmp = clone_repo(&target).await?;
    let branch = current_branch(tmp.path())
        .await
        .unwrap_or_else(|| "HEAD".to_string());
    let files = collect_files(tmp.path(), SelectionPolicy::Allowlist { include_source })
        .await
        .map_err(|e| anyhow!("{e}"))?;
    let total = files.len();
    let mut docs = Vec::new();
    let mut skipped_files = 0usize;
    for (index, file) in files.into_iter().enumerate() {
        let mut file_docs =
            file_docs(tmp.path(), &target, &branch, file, source_type, provider).await?;
        if file_docs.is_empty() {
            skipped_files += 1;
        }
        docs.append(&mut file_docs);
        if (index + 1) % 25 == 0 || index + 1 == total {
            reporter
                .report(serde_json::json!({"files_done": index + 1, "files_total": total}))
                .await;
        }
    }
    let current_urls: HashSet<String> = docs.iter().map(|doc| doc.url().to_string()).collect();
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{e}"))?
        .require_success("generic git embed")
        .map_err(|e| anyhow!("{e}"))?;
    if include_source {
        // Delete file chunks that vanished upstream. Upserts over live URLs
        // (deterministic point IDs) already refreshed changed files during embed;
        // this removes fragments for files no longer present in the current
        // checkout. Source-generation cleanup now belongs to axon-ledger via the
        // `axon source` pipeline; this legacy `ingest` path does no generation
        // tracking.
        if let Err(err) = qdrant_delete_repo_file_fragments(
            cfg,
            provider,
            &target.host,
            None,
            &target.name,
            &current_urls,
        )
        .await
        {
            log_warn(&format!(
                "command=ingest_git legacy_fragment_cleanup_failed target={} err={err}",
                target.web_url
            ));
        }
    } else {
        log_warn(&format!(
            "command=ingest_git legacy_fragment_cleanup_skipped include_source={include_source} skipped_files={skipped_files} docs_failed={}",
            summary.docs_failed
        ));
    }
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
        target.web_url, summary.chunks_embedded
    ));
    Ok(summary.chunks_embedded)
}

#[cfg(test)]
#[path = "generic_git_tests.rs"]
mod tests;
