use crate::crates::core::config::Config;
use crate::crates::core::logging::log_warn;
use crate::crates::vector::ops::{PreparedDoc, chunk_text, embed_prepared_docs};
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

use super::GitHubCommonFields;
use super::meta::{GitHubPayloadParams, build_github_payload};

/// Walk a directory iteratively and collect all file paths.
/// Skips `.git` directories. Uses an explicit stack to avoid recursive heap allocation.
async fn collect_wiki_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.file_name().and_then(|n| n.to_str()) == Some(".git") {
                continue;
            }
            if entry.file_type().await?.is_dir() {
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }

    Ok(files)
}

/// Build PreparedDoc list from the files in a cloned wiki directory.
async fn build_wiki_docs(tmp_path: &str, common: &GitHubCommonFields) -> Result<Vec<PreparedDoc>> {
    let all_files = collect_wiki_files(Path::new(tmp_path)).await?;
    let mut docs: Vec<PreparedDoc> = Vec::new();

    for path in all_files {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();

        if !matches!(ext.as_str(), "md" | "rst" | "txt") {
            continue;
        }

        let content = match tokio::fs::read_to_string(&path).await {
            Ok(c) => c,
            Err(e) => {
                log_warn(&format!(
                    "command=ingest_github wiki_read_failed path={path:?} err={e}"
                ));
                continue;
            }
        };

        if content.trim().is_empty() {
            continue;
        }

        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Home");
        let wiki_url = format!(
            "https://github.com/{}/{}/wiki/{stem}",
            common.owner, common.name
        );
        let title = stem.replace(['-', '_'], " ");

        let extra = build_github_payload(&GitHubPayloadParams {
            repo: common.name.clone(),
            owner: common.owner.clone(),
            content_kind: "wiki".into(),
            default_branch: Some(common.default_branch.clone()),
            repo_description: common.repo_description.clone(),
            pushed_at: common.pushed_at.clone(),
            is_private: common.is_private,
            ..Default::default()
        });

        let chunks = chunk_text(&content);
        if !chunks.is_empty() {
            docs.push(PreparedDoc {
                url: wiki_url,
                domain: "github.com".to_string(),
                chunks,
                source_type: "github".to_string(),
                content_type: "text",
                title: Some(title),
                extra: Some(extra),
            });
        }
    }

    Ok(docs)
}

/// Ingest wiki pages from a GitHub repository by cloning the wiki git repo.
///
/// Uses `git clone --depth=1` to clone the wiki. If the wiki doesn't exist
/// (exit code 128 with "not found" in stderr), returns `Ok(0)` silently.
/// Other clone failures are logged and returned as errors.
///
/// Authentication uses `http.extraHeader` via git config env vars to avoid
/// embedding the token in the clone URL (which would leak in process args).
///
/// Requires `git` to be installed and on PATH.
pub async fn ingest_wiki(
    cfg: &Config,
    common: &GitHubCommonFields,
    token: Option<&str>,
) -> Result<usize> {
    // Create a temp directory; cleaned up automatically when `_tmp` is dropped
    let _tmp = tempfile::tempdir()?;
    let tmp_path = _tmp.path().to_string_lossy().to_string();

    // Plain HTTPS clone URL — token is passed via git config env vars, not the URL
    let clone_url = format!(
        "https://github.com/{}/{}.wiki.git",
        common.owner, common.name
    );

    // "--" separates flags from the URL argument to prevent argument injection
    let mut cmd = tokio::process::Command::new("git");
    cmd.args(["clone", "--depth=1", "--", &clone_url, &tmp_path]);

    // Use header-based auth to avoid embedding token in process args
    if let Some(t) = token {
        cmd.env("GIT_CONFIG_COUNT", "1");
        cmd.env("GIT_CONFIG_KEY_0", "http.extraHeader");
        cmd.env("GIT_CONFIG_VALUE_0", format!("Authorization: token {t}"));
    }

    let output = cmd
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("git not found or failed to start: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Exit code 128 with "not found" / "does not exist" = no wiki, expected
        // GitHub returns "invalid credentials" (not "not found") when a token is
        // provided but the wiki repo doesn't exist — security measure that avoids
        // revealing repo existence. Treat it the same as "not found".
        if stderr.contains("not found") || stderr.contains("does not exist") {
            return Ok(0);
        }
        // GitHub returns "invalid credentials" (not "not found") when a valid token
        // is provided but the wiki repo doesn't exist — anti-enumeration behaviour.
        // Log it so a genuine auth failure on an existing wiki is still visible.
        if token.is_some() && stderr.contains("invalid credentials") {
            log_warn(&format!(
                "command=ingest_github wiki_no_credentials repo={}/{} \
                 treating_as_no_wiki (GitHub anti-enumeration)",
                common.owner, common.name
            ));
            return Ok(0);
        }
        // Other failures are real errors worth surfacing
        log_warn(&format!(
            "wiki clone failed (exit {}): {}",
            output.status,
            stderr.trim()
        ));
        bail!("wiki clone failed: {}", stderr.trim());
    }

    let docs = build_wiki_docs(&tmp_path, common).await?;
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(summary.chunks_embedded)
}
