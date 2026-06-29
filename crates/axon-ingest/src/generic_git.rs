use std::collections::HashSet;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use axon_source_ledger::{
    CleanupDebtItem, ManifestItem, SourceIdentity, SourceKind, SourceLedgerStore,
};
use reqwest::Url;
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;

use crate::progress::PhaseReporter;
use crate::subprocess::{SUBPROCESS_TIMEOUT, run_command_with_timeout};
use axon_core::config::Config;
use axon_core::content::redact_url;
use axon_core::http::validate_url;
use axon_core::logging::{log_done, log_info, log_warn};
use axon_vector::ops::embed_prepared_docs;
use axon_vector::ops::file_ingest::{SelectionPolicy, collect_files};
use axon_vector::ops::qdrant::{
    CleanupSelectorV1, qdrant_delete_repo_file_fragments, qdrant_delete_source_cleanup_selector,
};
use axon_vector::ops::{LedgerPayload, PreparedDoc};

const GIT_SOURCE_INDEX_VERSION: i64 = 1;

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

pub(crate) fn git_ref_is_immutable_commit_sha(reference: &str) -> bool {
    reference.len() == 40 && reference.bytes().all(|b| b.is_ascii_hexdigit())
}

pub(crate) fn git_ref_schedules_refresh(reference: &str) -> bool {
    !git_ref_is_immutable_commit_sha(reference)
}

fn git_source_id(collection: &str, target: &GenericGitTarget, reference: &str) -> String {
    format!("git:{collection}:{}#{reference}", target.web_url)
}

fn git_source_identity(cfg: &Config, target: &GenericGitTarget, reference: &str) -> SourceIdentity {
    SourceIdentity::new(
        git_source_id(&cfg.collection, target, reference),
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
    let manifest = git_manifest_from_docs(&docs);
    let mut lease = None;
    if include_source && skipped_files == 0 {
        lease = Some(prepare_git_ledger_refresh(cfg, &target, &branch, &manifest).await?);
        if let Some(prepared) = &lease {
            docs = stamp_git_docs_with_ledger(docs, prepared)?;
        }
    }
    let summary_result: Result<_> = async {
        embed_prepared_docs(cfg, docs, None)
            .await
            .map_err(|e| anyhow!("{e}"))?
            .require_success("generic git embed")
            .map_err(|e| anyhow!("{e}"))
    }
    .await;
    let summary = match summary_result {
        Ok(summary) => summary,
        Err(err) => {
            if let Some(prepared) = &lease {
                return release_git_ledger_after_error(prepared, err).await;
            }
            return Err(err);
        }
    };
    if let Some(prepared) = lease {
        finalize_git_ledger_refresh(cfg, &prepared).await?;
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

#[derive(Debug)]
struct PreparedGitLedgerRefresh {
    store: SourceLedgerStore,
    source: SourceIdentity,
    manifest: Vec<ManifestItem>,
    stale: Vec<axon_source_ledger::StaleManifestItem>,
    generation: i64,
    target: GenericGitTarget,
    reference: String,
    lease_owner: String,
}

async fn prepare_git_ledger_refresh(
    cfg: &Config,
    target: &GenericGitTarget,
    reference: &str,
    manifest: &[ManifestItem],
) -> Result<PreparedGitLedgerRefresh> {
    let pool = open_source_ledger_pool(&cfg.sqlite_path.to_string_lossy()).await?;
    let store = SourceLedgerStore::new(pool);
    let source = git_source_identity(cfg, target, reference);
    let lease_owner = format!("ingest-git-{}", uuid::Uuid::new_v4());
    if !store
        .acquire_lease(&source, &lease_owner, 5 * 60 * 1000)
        .await?
    {
        bail!(
            "source ledger refresh already running for {}",
            source.source_id
        );
    }
    match prepare_git_manifest_with_store(&store, &source, manifest).await {
        Ok(prepared) => Ok(PreparedGitLedgerRefresh {
            store,
            source,
            manifest: manifest.to_vec(),
            stale: prepared.stale,
            generation: prepared.generation,
            target: target.clone(),
            reference: reference.to_string(),
            lease_owner,
        }),
        Err(err) => match store.release_lease(&source.source_id, &lease_owner).await {
            Ok(()) => Err(err),
            Err(release_err) => Err(anyhow!(
                "{err}; additionally failed to release source ledger lease: {release_err}"
            )),
        },
    }
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

#[derive(Debug, Clone)]
pub(crate) struct PreparedGitManifest {
    pub stale: Vec<axon_source_ledger::StaleManifestItem>,
    pub generation: i64,
}

pub(crate) async fn prepare_git_manifest_with_store(
    store: &SourceLedgerStore,
    source: &SourceIdentity,
    manifest: &[ManifestItem],
) -> Result<PreparedGitManifest> {
    let diff = store.diff_manifest(&source.source_id, manifest).await?;
    let generation = store.begin_generation(source).await?;
    Ok(PreparedGitManifest {
        stale: diff.removed,
        generation,
    })
}

fn stamp_git_docs_with_ledger(
    docs: Vec<PreparedDoc>,
    prepared: &PreparedGitLedgerRefresh,
) -> Result<Vec<PreparedDoc>> {
    docs.into_iter()
        .map(|doc| {
            let item_key = git_manifest_item_from_doc(&doc).item_key;
            let payload = LedgerPayload::try_new(
                prepared.source.source_id.clone(),
                prepared.source.source_kind.as_str(),
                prepared.generation,
                item_key,
                prepared.source.index_version,
            )
            .map_err(|err| anyhow!("invalid git ledger payload: {err}"))?;
            Ok(doc.with_ledger_payload(payload))
        })
        .collect()
}

async fn finalize_git_ledger_refresh(
    cfg: &Config,
    prepared: &PreparedGitLedgerRefresh,
) -> Result<()> {
    commit_git_ledger_refresh(prepared).await?;
    drain_git_source_cleanup_debt(cfg, prepared).await
}

async fn commit_git_ledger_refresh(prepared: &PreparedGitLedgerRefresh) -> Result<()> {
    let result: Result<()> = async {
        let mut cleanup_debt = Vec::new();
        for stale in &prepared.stale {
            let selector = serde_json::json!({
                "kind": "source_cleanup_v1",
                "selector_kind": "git_file",
                "collection": prepared.source.collection.as_str(),
                "source_id": prepared.source.source_id.as_str(),
                "source_kind": prepared.source.source_kind.as_str(),
                "source_generation": stale.indexed_generation,
                "clone_url": redact_url(&prepared.target.clone_url),
                "host": prepared.target.host.as_str(),
                "repo": prepared.target.name.as_str(),
                "reference": prepared.reference.as_str(),
                "refreshable": git_ref_schedules_refresh(&prepared.reference),
                "source_item_key": stale.item_key.as_str(),
                "source_index_version": prepared.source.index_version,
            });
            cleanup_debt.push(CleanupDebtItem::new(
                stale.indexed_generation,
                stale.item_key.clone(),
                selector.to_string(),
            ));
        }
        prepared
            .store
            .commit_generation_payload_for_owner(
                &prepared.source.source_id,
                prepared.generation,
                &prepared.lease_owner,
                &prepared.manifest,
                &cleanup_debt,
            )
            .await?;
        Ok(())
    }
    .await;
    let release_result = prepared
        .store
        .release_lease(&prepared.source.source_id, &prepared.lease_owner)
        .await;
    match (result, release_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(err), Ok(())) => Err(err),
        (Ok(()), Err(release_err)) => Err(release_err),
        (Err(err), Err(release_err)) => Err(anyhow!(
            "{err}; additionally failed to release source ledger lease: {release_err}"
        )),
    }
}

async fn drain_git_source_cleanup_debt(
    cfg: &Config,
    prepared: &PreparedGitLedgerRefresh,
) -> Result<()> {
    let debt = prepared
        .store
        .cleanup_debt_items(&prepared.source.source_id)
        .await?;
    for item in debt {
        let selector: CleanupSelectorV1 = serde_json::from_str(&item.selector_json)?;
        qdrant_delete_source_cleanup_selector(cfg, &selector).await?;
        prepared
            .store
            .clear_cleanup_debt_item(&prepared.source.source_id, item.generation, &item.item_key)
            .await?;
    }
    Ok(())
}

async fn release_git_ledger_after_error<T>(
    prepared: &PreparedGitLedgerRefresh,
    err: anyhow::Error,
) -> Result<T> {
    let abort_result = prepared
        .store
        .abort_generation_for_owner(
            &prepared.source.source_id,
            prepared.generation,
            &prepared.lease_owner,
        )
        .await;
    match prepared
        .store
        .release_lease(&prepared.source.source_id, &prepared.lease_owner)
        .await
    {
        Ok(()) => match abort_result {
            Ok(()) => Err(err),
            Err(abort_err) => Err(anyhow!(
                "{err}; additionally failed to abort source ledger generation: {abort_err}"
            )),
        },
        Err(release_err) => match abort_result {
            Ok(()) => Err(anyhow!(
                "{err}; additionally failed to release source ledger lease: {release_err}"
            )),
            Err(abort_err) => Err(anyhow!(
                "{err}; additionally failed to abort source ledger generation: {abort_err}; additionally failed to release source ledger lease: {release_err}"
            )),
        },
    }
}

#[cfg(test)]
#[path = "generic_git_tests.rs"]
mod tests;
