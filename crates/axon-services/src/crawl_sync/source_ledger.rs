use axon_core::config::Config;
use axon_crawl::manifest::{ManifestEntry, read_manifest_data};
use axon_source_ledger::{
    CleanupDebtItem, ManifestItem, SourceIdentity, SourceKind, SourceLedgerStore,
};
use axon_vector::ops::qdrant::{
    CleanupSelectorV1, qdrant_delete_source_cleanup_selectors, qdrant_publish_source_generation,
};
use axon_vector::ops::{LedgerPayload, embed_prepared_docs, prepare_path_native_docs};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;

const SOURCE_LEDGER_LEASE_TTL_MS: i64 = 30 * 60 * 1000;
const SOURCE_LEDGER_BACKOFF_MS: i64 = 60 * 1000;
const SOURCE_CLEANUP_BATCH_SIZE: usize = 128;

struct SourceLeaseHeartbeat {
    stop: tokio::sync::oneshot::Sender<()>,
    handle: tokio::task::JoinHandle<()>,
}

fn start_source_lease_heartbeat(
    store: SourceLedgerStore,
    source_id: String,
    owner: String,
) -> SourceLeaseHeartbeat {
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
                        tracing::warn!(
                            source_id,
                            error = %err,
                            "crawl source ledger lease heartbeat failed"
                        );
                        break;
                    }
                }
                _ = &mut stop_rx => break,
            }
        }
    });
    SourceLeaseHeartbeat { stop, handle }
}

async fn stop_source_lease_heartbeat(heartbeat: Option<SourceLeaseHeartbeat>) {
    if let Some(heartbeat) = heartbeat {
        let _ = heartbeat.stop.send(());
        let _ = heartbeat.handle.await;
    }
}

pub(super) async fn embed_and_commit_sync_crawl_manifest_to_ledger(
    cfg: &Config,
    start_url: &str,
    manifest_path: &std::path::Path,
    input: &str,
) -> Result<(), Box<dyn Error>> {
    let pool = axon_jobs::store::open_sqlite_pool_or_recover(&cfg.sqlite_path.to_string_lossy())
        .await
        .map_err(|err| -> Box<dyn Error> { Box::new(err) })?;
    let store = SourceLedgerStore::new(pool);
    let source = crawl_source_identity(start_url, &cfg.collection);
    let lease_owner = format!("crawl-sync-{}", uuid::Uuid::new_v4());
    if !store
        .acquire_lease(&source, &lease_owner, SOURCE_LEDGER_LEASE_TTL_MS)
        .await?
    {
        return Err(format!(
            "source ledger refresh already running for {}",
            source.source_id
        )
        .into());
    }
    let mut active_generation = None;
    let result: Result<(), Box<dyn Error>> = async {
        republish_committed_crawl_generation(cfg, &store, &source)
            .await
            .map_err(|err| -> Box<dyn Error> { err.into() })?;
        let manifest_items = crawl_manifest_to_ledger_items(manifest_path).await?;
        let changed_manifest_keys = crawl_changed_manifest_keys(manifest_path).await?;
        let docs = prepare_path_native_docs(cfg, input, Some("crawl")).await?;
        let doc_keys: BTreeSet<String> = docs.iter().map(|doc| doc.url().to_string()).collect();
        if doc_keys != changed_manifest_keys {
            return Err(format!(
                "crawl embed prepared {} changed docs but manifest has {} changed items; refusing to commit source ledger generation",
                doc_keys.len(),
                changed_manifest_keys.len()
            )
            .into());
        }
        let diff = store.diff_manifest(&source.source_id, &manifest_items).await?;
        let generation = store
            .begin_generation_for_owner(&source, &lease_owner)
            .await?;
        active_generation = Some(generation);
        let stamped_docs = docs
            .into_iter()
            .map(|doc| {
                let payload = LedgerPayload::try_new(
                    source.source_id.clone(),
                    source.source_kind.as_str(),
                    generation,
                    doc.url().to_string(),
                    source.index_version,
                )
                .map_err(|err| format!("invalid crawl ledger payload: {err}"))?;
                Ok(doc.with_ledger_payload(payload))
            })
            .collect::<Result<Vec<_>, String>>()
            .map_err(|err| -> Box<dyn Error> { err.into() })?;
        let refresh = CrawlGenerationRefresh {
            generation,
            stamped_docs,
            manifest_items,
            changed_manifest_keys,
            removed_items: diff.removed,
        };
        embed_publish_and_cleanup_crawl_generation(cfg, &store, &source, &lease_owner, refresh)
            .await
    }
    .await;
    let abort_result = if result.is_err() {
        if let Some(generation) = active_generation {
            store
                .abort_generation_for_owner(&source.source_id, generation, &lease_owner)
                .await
                .map_err(|err| -> Box<dyn Error> { err.into() })
        } else {
            Ok(())
        }
    } else {
        Ok(())
    };
    if let (Err(err), Err(abort_err)) = (&result, abort_result) {
        return Err(format!(
            "{err}; additionally failed to abort source ledger generation: {abort_err}"
        )
        .into());
    }
    let release_result = store
        .release_lease(&source.source_id, &lease_owner)
        .await
        .map_err(|err| -> Box<dyn Error> { err.into() });
    match (result, release_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(err), Ok(())) => Err(err),
        (Ok(()), Err(err)) => Err(err),
        (Err(err), Err(release_err)) => Err(format!(
            "{err}; additionally failed to release source ledger lease: {release_err}"
        )
        .into()),
    }
}

struct CrawlGenerationRefresh {
    generation: i64,
    stamped_docs: Vec<axon_vector::ops::PreparedDoc>,
    manifest_items: Vec<ManifestItem>,
    changed_manifest_keys: BTreeSet<String>,
    removed_items: Vec<axon_source_ledger::StaleManifestItem>,
}

async fn embed_publish_and_cleanup_crawl_generation(
    cfg: &Config,
    store: &SourceLedgerStore,
    source: &SourceIdentity,
    lease_owner: &str,
    refresh: CrawlGenerationRefresh,
) -> Result<(), Box<dyn Error>> {
    let heartbeat = start_source_lease_heartbeat(
        store.clone(),
        source.source_id.clone(),
        lease_owner.to_string(),
    );
    let result: Result<(), Box<dyn Error>> = async {
        let summary = embed_prepared_docs(cfg, refresh.stamped_docs, None)
            .await
            .map_err(|err| -> Box<dyn Error> { err })?
            .require_success("crawl output embed")
            .map_err(|err| -> Box<dyn Error> { err.into() })?;
        let changed_manifest_items: Vec<ManifestItem> = refresh
            .manifest_items
            .iter()
            .filter(|item| refresh.changed_manifest_keys.contains(&item.item_key))
            .cloned()
            .collect();
        let live_item_keys: BTreeSet<String> = refresh
            .manifest_items
            .iter()
            .map(|item| item.item_key.clone())
            .collect();
        let cleanup_debt = crawl_cleanup_debt(&refresh.removed_items, source)?;
        store
            .commit_generation_delta_for_owner(
                &source.source_id,
                refresh.generation,
                lease_owner,
                &changed_manifest_items,
                &live_item_keys,
                &cleanup_debt,
            )
            .await?;
        qdrant_publish_source_generation(
            cfg,
            &source.source_id,
            refresh.generation,
            source.index_version,
            summary.chunks_embedded,
        )
        .await
        .map_err(|err| -> Box<dyn Error> { err.into() })?;
        drain_crawl_source_cleanup_debt(cfg, store, source).await?;
        Ok(())
    }
    .await;
    stop_source_lease_heartbeat(Some(heartbeat)).await;
    if let Err(err) = &result {
        set_source_backoff(store, &source.source_id, "qdrant", &err.to_string()).await;
    }
    result
}

async fn republish_committed_crawl_generation(
    cfg: &Config,
    store: &SourceLedgerStore,
    source: &SourceIdentity,
) -> Result<(), anyhow::Error> {
    let status = store.source_status(&source.source_id).await?;
    if status.committed_generation > 0 {
        qdrant_publish_source_generation(
            cfg,
            &source.source_id,
            status.committed_generation,
            source.index_version,
            0,
        )
        .await?;
    }
    Ok(())
}

fn crawl_cleanup_debt(
    removed: &[axon_source_ledger::StaleManifestItem],
    source: &SourceIdentity,
) -> Result<Vec<CleanupDebtItem>, Box<dyn Error>> {
    removed
        .iter()
        .map(|stale| -> Result<CleanupDebtItem, Box<dyn Error>> {
            let selector = CleanupSelectorV1::new(
                source.collection.clone(),
                source.source_id.clone(),
                source.index_version,
                stale.indexed_generation,
                stale.item_key.clone(),
            )
            .map_err(|err| -> Box<dyn Error> { err.into() })?;
            Ok(CleanupDebtItem::new(
                stale.indexed_generation,
                stale.item_key.clone(),
                serde_json::to_string(&selector).map_err(|err| -> Box<dyn Error> { err.into() })?,
            ))
        })
        .collect()
}

async fn drain_crawl_source_cleanup_debt(
    cfg: &Config,
    store: &SourceLedgerStore,
    source: &SourceIdentity,
) -> Result<(), Box<dyn Error>> {
    let debt = store.cleanup_debt_items(&source.source_id).await?;
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
                    store
                        .mark_cleanup_debt_failed(
                            &source.source_id,
                            item.generation,
                            &item.item_key,
                            &message,
                        )
                        .await?;
                }
                return Err(err.into());
            }
            for (item, _) in chunk {
                store
                    .clear_cleanup_debt_item(&source.source_id, item.generation, &item.item_key)
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
) {
    let until_ms = chrono::Utc::now()
        .timestamp_millis()
        .saturating_add(SOURCE_LEDGER_BACKOFF_MS);
    if let Err(err) = store
        .set_backoff(source_id, until_ms, dependency, message)
        .await
    {
        tracing::warn!(
            source_id,
            error = %err,
            "failed to set crawl source ledger backoff"
        );
    }
}

pub(crate) fn crawl_source_identity(start_url: &str, collection: &str) -> SourceIdentity {
    let mut hasher = Sha256::new();
    hasher.update(start_url.as_bytes());
    let digest = hasher.finalize();
    SourceIdentity::new(
        format!("crawl:{collection}:{digest:x}"),
        SourceKind::Crawl,
        collection.to_string(),
        1,
    )
}

pub(crate) async fn crawl_manifest_to_ledger_items(
    manifest_path: &std::path::Path,
) -> Result<Vec<ManifestItem>, Box<dyn Error>> {
    let mut entries: Vec<ManifestEntry> = read_manifest_data(manifest_path)
        .await?
        .into_values()
        .collect();
    entries.sort_by(|a, b| a.url.cmp(&b.url));
    Ok(entries
        .into_iter()
        .map(|entry| {
            ManifestItem::new(
                entry.url,
                entry
                    .content_hash
                    .unwrap_or_else(|| format!("markdown_chars:{}", entry.markdown_chars)),
                entry.markdown_chars as i64,
            )
        })
        .collect())
}

pub(crate) async fn crawl_changed_manifest_keys(
    manifest_path: &std::path::Path,
) -> Result<BTreeSet<String>, Box<dyn Error>> {
    Ok(read_manifest_data(manifest_path)
        .await?
        .into_values()
        .filter(|entry| entry.changed)
        .map(|entry| entry.url)
        .collect())
}
