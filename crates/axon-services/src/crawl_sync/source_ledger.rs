use axon_core::config::Config;
use axon_crawl::manifest::{ManifestEntry, read_manifest_data};
use axon_source_ledger::{
    CleanupDebtItem, ManifestItem, SourceIdentity, SourceKind, SourceLedgerStore,
};
use axon_vector::ops::qdrant::{CleanupSelectorV1, qdrant_delete_source_cleanup_selector};
use axon_vector::ops::{LedgerPayload, embed_prepared_docs, prepare_path_native_docs};
use std::collections::BTreeSet;
use std::error::Error;

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
        .acquire_lease(&source, &lease_owner, 5 * 60 * 1000)
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
        let generation = store.begin_generation(&source).await?;
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
        embed_prepared_docs(cfg, stamped_docs, None)
            .await
            .map_err(|err| -> Box<dyn Error> { err })?
            .require_success("crawl output embed")
            .map_err(|err| -> Box<dyn Error> { err.into() })?;
        let cleanup_debt = crawl_cleanup_debt(&diff.removed, &source, start_url);
        store
            .commit_generation_payload_for_owner(
                &source.source_id,
                generation,
                &lease_owner,
                &manifest_items,
                &cleanup_debt,
            )
            .await?;
        drain_crawl_source_cleanup_debt(cfg, &store, &source).await?;
        Ok(())
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

fn crawl_cleanup_debt(
    removed: &[axon_source_ledger::StaleManifestItem],
    source: &SourceIdentity,
    start_url: &str,
) -> Vec<CleanupDebtItem> {
    removed
        .iter()
        .map(|stale| {
            let selector = serde_json::json!({
                "kind": "source_cleanup_v1",
                "selector_kind": "crawl_url",
                "collection": source.collection.as_str(),
                "source_id": source.source_id.as_str(),
                "source_kind": source.source_kind.as_str(),
                "source_generation": stale.indexed_generation,
                "start_url": start_url,
                "source_item_key": stale.item_key.as_str(),
                "source_index_version": source.index_version,
            });
            CleanupDebtItem::new(
                stale.indexed_generation,
                stale.item_key.clone(),
                selector.to_string(),
            )
        })
        .collect()
}

async fn drain_crawl_source_cleanup_debt(
    cfg: &Config,
    store: &SourceLedgerStore,
    source: &SourceIdentity,
) -> Result<(), Box<dyn Error>> {
    let debt = store.cleanup_debt_items(&source.source_id).await?;
    for item in debt {
        let selector: CleanupSelectorV1 = serde_json::from_str(&item.selector_json)?;
        qdrant_delete_source_cleanup_selector(cfg, &selector).await?;
        store
            .clear_cleanup_debt_item(&source.source_id, item.generation, &item.item_key)
            .await?;
    }
    Ok(())
}

pub(crate) fn crawl_source_identity(start_url: &str, collection: &str) -> SourceIdentity {
    SourceIdentity::new(
        format!("crawl:{collection}:{start_url}"),
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
