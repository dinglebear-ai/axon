use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use base64::Engine as _;

use crate::core::config::Config;
use crate::ingest::github::{is_indexable_doc_path, is_indexable_source_path};
use crate::ingest::progress::PhaseReporter;
use crate::ingest::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};
use crate::vector::ops::input::code::chunk_code;
use crate::vector::ops::{PreparedDoc, chunk_text};

use super::embed::{embed_docs, gitlab_payload};
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
    let files = collect_files(tmp.path(), include_source).await?;
    let total = files.len();
    let mut docs = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        let rel = file
            .strip_prefix(tmp.path())?
            .to_string_lossy()
            .replace('\\', "/");
        let Ok(content) = tokio::fs::read_to_string(&file).await else {
            continue;
        };
        let ext = Path::new(&rel)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let chunks = {
            let content_clone = content.clone();
            let ext_clone = ext.clone();
            match tokio::task::spawn_blocking(move || {
                chunk_code(&content_clone, &ext_clone).unwrap_or_else(|| chunk_text(&content_clone))
            })
            .await
            {
                Ok(chunks) => chunks,
                Err(_) => chunk_text(&content),
            }
        };
        if !chunks.is_empty() {
            docs.push(PreparedDoc {
                url: format!("{}/-/blob/{}/{}", target.web_url, branch, rel),
                domain: target.host.clone(),
                chunks,
                source_type: "gitlab".to_string(),
                content_type: "text",
                title: Some(rel.clone()),
                extra: Some(gitlab_payload(
                    target,
                    project,
                    "file",
                    serde_json::json!({"path": rel, "branch": branch}),
                )),
                extractor_name: None,
                structured: None,
            });
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
