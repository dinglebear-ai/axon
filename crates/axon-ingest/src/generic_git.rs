use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow, bail};
use axon_source_ledger::{ManifestItem, SourceIdentity, SourceKind, SourceLedgerStore};
use reqwest::Url;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::git_payload::{ContentKind, GitPayload, build_git_payload};
use crate::progress::PhaseReporter;
use crate::subprocess::{MAX_INGEST_FILE_BYTES, SUBPROCESS_TIMEOUT, run_command_with_timeout};
use axon_core::config::Config;
use axon_core::http::validate_url;
use axon_core::logging::{log_done, log_info, log_warn};
use axon_vector::ops::embed_prepared_docs;
use axon_vector::ops::file_ingest::{SelectionPolicy, collect_files};
use axon_vector::ops::input::classify::{
    classify_file_type, is_test_path, language_name, path_extension,
};
use axon_vector::ops::qdrant::qdrant_delete_repo_file_fragments;
use axon_vector::ops::{PreparedDoc, SourceDocument, SourceOrigin, prepare_source_document};

const GIT_SOURCE_INDEX_VERSION: i64 = 1;

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

pub(crate) fn git_ref_is_immutable_commit_sha(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn git_ref_schedules_refresh(reference: &str) -> bool {
    !git_ref_is_immutable_commit_sha(reference)
}

fn git_source_id(target: &GenericGitTarget, reference: &str) -> String {
    format!("git:{}#{reference}", target.clone_url)
}

fn git_source_identity(cfg: &Config, target: &GenericGitTarget, reference: &str) -> SourceIdentity {
    SourceIdentity::new(
        git_source_id(target, reference),
        SourceKind::Git,
        cfg.collection.clone(),
        GIT_SOURCE_INDEX_VERSION,
    )
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
    let manifest = git_manifest_from_docs(&docs);
    let summary = embed_prepared_docs(cfg, docs, None)
        .await
        .map_err(|e| anyhow!("{e}"))?
        .require_success("generic git embed")
        .map_err(|e| anyhow!("{e}"))?;
    if include_source && skipped_files == 0 && summary.docs_failed == 0 {
        if let Err(err) = commit_git_manifest_to_ledger(cfg, &target, &branch, &manifest).await {
            log_warn(&format!(
                "command=ingest_git source_ledger_commit_failed target={} err={err}",
                target.clone_url
            ));
        }
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
                target.clone_url
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
        target.clone_url, summary.chunks_embedded
    ));
    Ok(summary.chunks_embedded)
}

fn git_manifest_from_docs(docs: &[PreparedDoc]) -> Vec<ManifestItem> {
    docs.iter().map(git_manifest_item_from_doc).collect()
}

fn git_manifest_item_from_doc(doc: &PreparedDoc) -> ManifestItem {
    let item_key = doc
        .extra()
        .and_then(|extra| extra.get("code_file_path"))
        .and_then(|value| value.as_str())
        .unwrap_or_else(|| doc.url())
        .to_string();
    let mut hasher = Sha256::new();
    let mut size_bytes = 0i64;
    for chunk in doc.chunks() {
        size_bytes = size_bytes.saturating_add(chunk.len() as i64);
        hasher.update(chunk.as_bytes());
        hasher.update([0]);
    }
    ManifestItem::new(item_key, hex::encode(hasher.finalize()), size_bytes)
}

async fn commit_git_manifest_to_ledger(
    cfg: &Config,
    target: &GenericGitTarget,
    reference: &str,
    manifest: &[ManifestItem],
) -> Result<()> {
    let pool = open_source_ledger_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    let store = SourceLedgerStore::new(pool);
    let source = git_source_identity(cfg, target, reference);
    commit_git_manifest_with_store(&store, &source, target, reference, manifest)
        .await
        .map(|_| ())
}

async fn open_source_ledger_pool(path: &str) -> Result<SqlitePool> {
    let pool = axon_core::sqlite::open_pool(path)
        .await
        .map_err(|err| anyhow!("open source ledger sqlite: {err}"))?;
    sqlx::migrate!("../axon-jobs/src/migrations")
        .run(&pool)
        .await
        .map_err(|err| anyhow!("run source ledger migrations: {err}"))?;
    Ok(pool)
}

pub(crate) async fn commit_git_manifest_with_store(
    store: &SourceLedgerStore,
    source: &SourceIdentity,
    target: &GenericGitTarget,
    reference: &str,
    manifest: &[ManifestItem],
) -> Result<i64> {
    store.ensure_source(source).await?;
    let diff = store.diff_manifest(&source.source_id, manifest).await?;
    let generation = store.begin_generation(source).await?;
    for item in manifest {
        store
            .record_manifest_item(&source.source_id, generation, item.clone())
            .await?;
    }
    for item_key in diff.removed {
        let selector = serde_json::json!({
            "kind": "git_file",
            "source_id": source.source_id.as_str(),
            "source_kind": source.source_kind.as_str(),
            "generation": generation,
            "clone_url": target.clone_url.as_str(),
            "host": target.host.as_str(),
            "repo": target.name.as_str(),
            "reference": reference,
            "refreshable": git_ref_schedules_refresh(reference),
            "item_key": item_key.as_str(),
        });
        store
            .record_cleanup_debt(
                &source.source_id,
                generation,
                &item_key,
                &selector.to_string(),
            )
            .await?;
    }
    store
        .commit_generation(&source.source_id, generation)
        .await?;
    Ok(generation)
}

pub(crate) async fn file_docs(
    root: &Path,
    target: &GenericGitTarget,
    branch: &str,
    file: PathBuf,
    source_type: &str,
    provider: &str,
) -> Result<Vec<PreparedDoc>> {
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
            return Ok(Vec::new());
        }
        Err(e) => {
            log_warn(&format!(
                "command=ingest_git stat_failed path={rel} err={e}"
            ));
            return Ok(Vec::new());
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
            return Ok(Vec::new());
        }
    };
    let content = match String::from_utf8(bytes) {
        Ok(t) => t,
        Err(_) => {
            log_warn(&format!("command=ingest_git skip_non_utf8 path={rel}"));
            return Ok(Vec::new());
        }
    };

    if content.trim().is_empty() {
        return Ok(Vec::new());
    }
    let ext = path_extension(&rel).to_ascii_lowercase();
    let lang = language_name(&ext).to_string();
    let ftype = classify_file_type(&rel).to_string();
    let is_test = is_test_path(&rel);
    let extra = build_git_payload(&GitPayload {
        provider: provider.to_string(),
        host: target.host.clone(),
        owner: None,
        repo: target.name.clone(),
        content_kind: ContentKind::File,
        branch: Some(branch.to_string()),
        file_path: Some(rel.clone()),
        file_language: Some(lang),
        file_type: Some(ftype),
        file_is_test: Some(is_test),
        line_start: None,
        line_end: None,
        chunking_method: None,
        symbol_name: None,
        symbol_kind: None,
        symbol_extraction_status: None,
        meta: Some(serde_json::json!({ "clone_url": target.clone_url })),
        ..GitPayload::default()
    });
    let url = format!("{}#{}:{}", target.web_url, branch, rel);
    let source_doc = SourceDocument::try_new_file(
        SourceOrigin::GitFile,
        url,
        rel.clone(),
        ext,
        content,
        source_type,
        Some(rel.clone()),
        Some(extra),
    )
    .map_err(|err| anyhow!("invalid source document for {rel}: {err}"))?;
    let doc = prepare_source_document(source_doc)
        .await
        .map_err(|err| anyhow!("prepare source document failed for {rel}: {err}"))?;
    Ok(vec![doc])
}

#[cfg(test)]
#[path = "generic_git_tests.rs"]
mod tests;
