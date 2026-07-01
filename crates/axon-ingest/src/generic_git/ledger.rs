use std::collections::BTreeMap;

use anyhow::{Result, anyhow, bail};
use axon_core::config::Config;
use axon_core::logging::log_warn;
use axon_source_ledger::{
    CleanupDebtItem, ManifestItem, SourceIdentity, SourceLedgerStore, StaleManifestItem,
};
use axon_vector::ops::qdrant::{
    CleanupSelectorV1, qdrant_delete_source_cleanup_selectors, qdrant_publish_source_generation,
    qdrant_republish_committed_source_generation,
};
use axon_vector::ops::{LedgerPayload, PreparedDoc};
use sqlx::SqlitePool;

use super::{
    GenericGitTarget, SOURCE_CLEANUP_BATCH_SIZE, SOURCE_LEDGER_BACKOFF_MS,
    SOURCE_LEDGER_LEASE_TTL_MS, git_manifest_item_from_doc, git_source_identity,
};

#[derive(Debug)]
pub(crate) struct PreparedGitLedgerRefresh {
    pub(crate) store: SourceLedgerStore,
    pub(crate) source: SourceIdentity,
    pub(crate) manifest: Vec<ManifestItem>,
    pub(crate) stale: Vec<StaleManifestItem>,
    pub(crate) generation: i64,
    pub(crate) lease_owner: String,
}

pub(super) struct SourceLeaseHeartbeat {
    stop: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

pub(super) fn start_source_lease_heartbeat(
    prepared: &PreparedGitLedgerRefresh,
) -> SourceLeaseHeartbeat {
    let store = prepared.store.clone();
    let source_id = prepared.source.source_id.clone();
    let owner = prepared.lease_owner.clone();
    let (stop, mut stop_rx) = tokio::sync::oneshot::channel();
    let handle = tokio::spawn(async move {
        let tick_ms = (SOURCE_LEDGER_LEASE_TTL_MS / 3).max(1_000) as u64;
        let mut interval = tokio::time::interval(std::time::Duration::from_millis(tick_ms));
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    if let Err(err) = store
                        .extend_lease_for_owner(&source_id, &owner, SOURCE_LEDGER_LEASE_TTL_MS)
                        .await
                    {
                        log_warn(&format!(
                            "command=ingest_git source_ledger_heartbeat_failed source_id={source_id} err={err}"
                        ));
                        break;
                    }
                }
                _ = &mut stop_rx => break,
            }
        }
    });
    SourceLeaseHeartbeat { stop, handle }
}

pub(super) async fn stop_source_lease_heartbeat(heartbeat: Option<SourceLeaseHeartbeat>) {
    if let Some(heartbeat) = heartbeat {
        let _ = heartbeat.stop.send(());
        let _ = heartbeat.handle.await;
    }
}

pub(super) async fn prepare_git_ledger_refresh(
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
        .acquire_lease(&source, &lease_owner, SOURCE_LEDGER_LEASE_TTL_MS)
        .await?
    {
        bail!(
            "source ledger refresh already running for {}",
            source.source_id
        );
    }
    if let Err(err) = qdrant_republish_committed_source_generation(cfg, &store, &source).await {
        return match store.release_lease(&source.source_id, &lease_owner).await {
            Ok(()) => Err(err),
            Err(release_err) => Err(anyhow!(
                "{err}; additionally failed to release source ledger lease: {release_err}"
            )),
        };
    }
    match prepare_git_manifest_with_store(&store, &source, &lease_owner, manifest).await {
        Ok(prepared) => Ok(PreparedGitLedgerRefresh {
            store,
            source,
            manifest: manifest.to_vec(),
            stale: prepared.stale,
            generation: prepared.generation,
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

pub(crate) async fn open_source_ledger_pool(path: &str) -> Result<SqlitePool> {
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
    pub stale: Vec<StaleManifestItem>,
    pub generation: i64,
}

pub(crate) async fn prepare_git_manifest_with_store(
    store: &SourceLedgerStore,
    source: &SourceIdentity,
    owner: &str,
    manifest: &[ManifestItem],
) -> Result<PreparedGitManifest> {
    let diff = store.diff_manifest(&source.source_id, manifest).await?;
    let generation = store.begin_generation_for_owner(source, owner).await?;
    Ok(PreparedGitManifest {
        stale: diff.removed,
        generation,
    })
}

pub(super) fn stamp_git_docs_with_ledger(
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

pub(super) async fn finalize_git_ledger_refresh(
    cfg: &Config,
    prepared: &PreparedGitLedgerRefresh,
    expected_visible_points: usize,
) -> Result<()> {
    let result = async {
        commit_git_ledger_refresh(prepared).await?;
        qdrant_publish_source_generation(
            cfg,
            &prepared.source.source_id,
            prepared.generation,
            prepared.source.index_version,
            expected_visible_points,
        )
        .await?;
        drain_git_source_cleanup_debt(cfg, prepared).await
    }
    .await;
    let release_result = prepared
        .store
        .release_lease(&prepared.source.source_id, &prepared.lease_owner)
        .await;
    let backoff_result = if let Err(err) = &result {
        Some(
            set_source_backoff(
                &prepared.store,
                &prepared.source.source_id,
                "qdrant",
                &err.to_string(),
            )
            .await,
        )
    } else {
        None
    };
    match (result, release_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(err), Ok(())) => match backoff_result {
            Some(Err(backoff_err)) => Err(anyhow!(
                "{err}; additionally failed to set source ledger backoff: {backoff_err}"
            )),
            _ => Err(err),
        },
        (Ok(()), Err(release_err)) => Err(release_err),
        (Err(err), Err(release_err)) => match backoff_result {
            Some(Err(backoff_err)) => Err(anyhow!(
                "{err}; additionally failed to set source ledger backoff: {backoff_err}; additionally failed to release source ledger lease: {release_err}"
            )),
            _ => Err(anyhow!(
                "{err}; additionally failed to release source ledger lease: {release_err}"
            )),
        },
    }
}

pub(crate) async fn commit_git_ledger_refresh(prepared: &PreparedGitLedgerRefresh) -> Result<()> {
    let mut cleanup_debt = Vec::new();
    for stale in &prepared.stale {
        let selector = CleanupSelectorV1::new(
            prepared.source.collection.clone(),
            prepared.source.source_id.clone(),
            prepared.source.index_version,
            stale.indexed_generation,
            stale.item_key.clone(),
        )?;
        cleanup_debt.push(CleanupDebtItem::new(
            stale.indexed_generation,
            stale.item_key.clone(),
            serde_json::to_string(&selector)?,
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
        .await
}

async fn drain_git_source_cleanup_debt(
    cfg: &Config,
    prepared: &PreparedGitLedgerRefresh,
) -> Result<()> {
    let debt = prepared
        .store
        .cleanup_debt_items(&prepared.source.source_id)
        .await?;
    let mut grouped: BTreeMap<(String, i64, i64), Vec<(CleanupDebtItem, CleanupSelectorV1)>> =
        BTreeMap::new();
    for item in debt {
        let selector: CleanupSelectorV1 = serde_json::from_str(&item.selector_json)?;
        grouped
            .entry((
                selector.source_id().to_string(),
                selector.source_generation(),
                selector.source_index_version(),
            ))
            .or_default()
            .push((item, selector));
    }
    for entries in grouped.into_values() {
        for chunk in entries.chunks(SOURCE_CLEANUP_BATCH_SIZE) {
            let selectors: Vec<CleanupSelectorV1> =
                chunk.iter().map(|(_, selector)| selector.clone()).collect();
            if let Err(err) = qdrant_delete_source_cleanup_selectors(cfg, &selectors).await {
                let message = err.to_string();
                for (item, _) in chunk {
                    prepared
                        .store
                        .mark_cleanup_debt_failed(
                            &prepared.source.source_id,
                            item.generation,
                            &item.item_key,
                            &message,
                        )
                        .await?;
                }
                return Err(err);
            }
            for (item, _) in chunk {
                prepared
                    .store
                    .clear_cleanup_debt_item(
                        &prepared.source.source_id,
                        item.generation,
                        &item.item_key,
                    )
                    .await?;
            }
        }
    }
    Ok(())
}

async fn set_source_backoff(
    store: &SourceLedgerStore,
    source_id: &str,
    dependency: &str,
    message: &str,
) -> Result<()> {
    let until_ms = chrono::Utc::now()
        .timestamp_millis()
        .saturating_add(SOURCE_LEDGER_BACKOFF_MS);
    store
        .set_backoff(source_id, until_ms, dependency, message)
        .await
}

pub(super) async fn release_git_ledger_after_error<T>(
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
