use anyhow::{Result, bail};
use base64::Engine as _;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::core::config::Config;
use crate::core::logging::log_warn;
use crate::ingest::git_files::embed_doc_summary;
use crate::ingest::git_payload::{ContentKind, GitPayload, build_git_payload};
use crate::ingest::progress::PhaseReporter;
use crate::ingest::subprocess::MAX_INGEST_FILE_BYTES;
use crate::ingest::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};
use crate::vector::ops::file_ingest::{SelectionPolicy, collect_files};
use crate::vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use crate::vector::ops::qdrant::qdrant_delete_repo_file_fragments;
use crate::vector::ops::{SourceDocument, SourceOrigin, prepare_source_document};

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
    let PreparedFiles {
        docs,
        skipped_files,
    } = prepare_file_docs(tmp.path(), files, target, project, branch, reporter).await?;
    let current_urls: HashSet<String> = docs.iter().map(|doc| doc.url().to_string()).collect();
    let summary = embed_doc_summary(cfg, docs).await?;
    let chunks = summary.chunks_embedded;
    if include_source && skipped_files == 0 && summary.docs_failed == 0 {
        let owner = target
            .namespace_path
            .rfind('/')
            .map(|idx| &target.namespace_path[..idx]);
        if let Err(err) = qdrant_delete_repo_file_fragments(
            cfg,
            "gitlab",
            &target.host,
            owner,
            &target.project,
            &current_urls,
        )
        .await
        {
            log_warn(&format!(
                "command=ingest_gitlab legacy_fragment_cleanup_failed target={} err={err}",
                target.namespace_path
            ));
        }
    } else {
        log_warn(&format!(
            "command=ingest_gitlab legacy_fragment_cleanup_skipped include_source={include_source} skipped_files={skipped_files} docs_failed={}",
            summary.docs_failed
        ));
    }
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

struct PreparedFiles {
    docs: Vec<crate::vector::ops::PreparedDoc>,
    skipped_files: usize,
}

async fn prepare_file_docs(
    root: &Path,
    files: Vec<PathBuf>,
    target: &GitLabTarget,
    project: &GitLabProject,
    branch: &str,
    reporter: &PhaseReporter,
) -> Result<PreparedFiles> {
    let total = files.len();
    let mut docs = Vec::new();
    let mut skipped_files = 0usize;
    for (index, file) in files.into_iter().enumerate() {
        let rel = file
            .strip_prefix(root)?
            .to_string_lossy()
            .replace('\\', "/");
        // S-M2: stat before read — skip files over the ingest size cap
        match tokio::fs::metadata(&file).await {
            Ok(meta) if meta.len() > MAX_INGEST_FILE_BYTES => {
                log_warn(&format!(
                    "command=ingest_gitlab skip_large_file path={rel} size_bytes={}",
                    meta.len()
                ));
                skipped_files += 1;
                continue;
            }
            Err(e) => {
                log_warn(&format!(
                    "command=ingest_gitlab stat_failed path={rel} err={e}"
                ));
                skipped_files += 1;
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
                skipped_files += 1;
                continue;
            }
        };
        let content = match String::from_utf8(bytes) {
            Ok(t) => t,
            Err(_) => {
                log_warn(&format!("command=ingest_gitlab skip_non_utf8 path={rel}"));
                skipped_files += 1;
                continue;
            }
        };
        let ext = path_extension(&rel).to_ascii_lowercase();
        let extra = gitlab_file_doc_extra(target, project, branch, &rel, &ext);
        let source_doc = match SourceDocument::try_new_file(
            SourceOrigin::GitFile,
            format!("{}/-/blob/{}/{}", target.web_url, branch, rel),
            rel.clone(),
            ext,
            content,
            "gitlab",
            Some(rel.clone()),
            Some(extra),
        ) {
            Ok(doc) => doc,
            Err(err) => {
                log_warn(&format!(
                    "command=ingest_gitlab invalid_source_doc path={rel} err={err}"
                ));
                skipped_files += 1;
                continue;
            }
        };
        match prepare_source_document(source_doc).await {
            Ok(doc) => docs.push(doc),
            Err(err) => {
                log_warn(&format!(
                    "command=ingest_gitlab prepare_source_doc_failed path={rel} err={err}"
                ));
                skipped_files += 1;
                continue;
            }
        }
        if (index + 1) % 25 == 0 || index + 1 == total {
            reporter
                .report(serde_json::json!({"files_done": index + 1, "files_total": total}))
                .await;
        }
    }
    Ok(PreparedFiles {
        docs,
        skipped_files,
    })
}

pub(crate) fn gitlab_file_doc_extra(
    target: &GitLabTarget,
    project: &GitLabProject,
    branch: &str,
    rel: &str,
    ext: &str,
) -> serde_json::Value {
    let owner = target
        .namespace_path
        .rfind('/')
        .map(|idx| target.namespace_path[..idx].to_string());
    build_git_payload(&GitPayload {
        provider: "gitlab".to_string(),
        host: target.host.clone(),
        owner,
        repo: target.project.clone(),
        content_kind: ContentKind::File,
        branch: Some(branch.to_string()),
        file_path: Some(rel.to_string()),
        file_language: Some(language_name(ext).to_string()),
        file_type: Some(classify_file_type(rel).to_string()),
        file_is_test: Some(is_test_path(rel)),
        meta: Some(serde_json::json!({
            "namespace_path": target.namespace_path,
            "visibility": project.visibility,
            "last_activity_at": project.last_activity_at,
            "default_branch": project.default_branch,
        })),
        ..GitPayload::default()
    })
}

#[cfg(test)]
#[path = "files_tests.rs"]
mod tests;
