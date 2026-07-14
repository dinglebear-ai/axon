use std::collections::{BTreeMap, BTreeSet};

use axon_api::source::*;

use crate::sqlite::util::{json_error, manifest_item_changed, timestamp};
use crate::store::Result;

use super::manifest_items::{manifest_in_tx, manifest_items_in_tx};

const ARTIFACT_METADATA_KEY: &str = "_axon_artifacts";
const CACHE_KEY_METADATA_KEY: &str = "_axon_document_cache_key";

pub(super) async fn stale_item_cleanup_debt_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    generation: &SourceGeneration,
    previous_generation: Option<&SourceGenerationId>,
) -> Result<Vec<CleanupDebt>> {
    let Some(previous_generation) = previous_generation else {
        return Ok(Vec::new());
    };
    let previous_manifest = manifest_in_tx(tx, &generation.source_id, previous_generation).await?;
    let previous_items = previous_manifest
        .as_ref()
        .map(|manifest| manifest.items.clone())
        .unwrap_or_else(Vec::new);
    let next_items =
        manifest_items_in_tx(tx, &generation.source_id, &generation.generation).await?;
    let next_by_key = next_items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect::<BTreeMap<_, _>>();

    let mut cleanup_debt = Vec::new();
    let mut artifact_ids = BTreeSet::new();
    if let Some(previous_manifest) = previous_manifest.as_ref() {
        cleanup_debt.extend(artifact_cleanup_debt_for_metadata(
            &generation.source_id,
            previous_generation,
            &previous_manifest.metadata,
            "manifest",
            &mut artifact_ids,
        )?);
    }
    for item in previous_items {
        if let Some(next) = next_by_key.get(&item.source_item_key)
            && !manifest_item_changed(&item, next)
        {
            continue;
        }
        cleanup_debt.push(vector_cleanup_debt(
            &generation.source_id,
            previous_generation,
            &item.source_item_key,
        ));
        cleanup_debt.extend(artifact_cleanup_debt_for_metadata(
            &generation.source_id,
            previous_generation,
            &item.metadata,
            &item.source_item_key.0,
            &mut artifact_ids,
        )?);
        if let Some(debt) = cache_cleanup_debt_for_metadata(
            &generation.source_id,
            previous_generation,
            &item.metadata,
            &item.source_item_key,
        )? {
            cleanup_debt.push(debt);
        }
    }
    Ok(cleanup_debt)
}

fn vector_cleanup_debt(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    source_item_key: &SourceItemKey,
) -> CleanupDebt {
    CleanupDebt {
        debt_id: CleanupDebtId::new(format!(
            "debt_{}",
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!(
                    "{}:{}:{}",
                    source_id.0, previous_generation.0, source_item_key.0
                )
                .as_bytes(),
            )
        )),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: source_id.clone(),
        generation: Some(previous_generation.clone()),
        kind: CleanupDebtKind::VectorDelete,
        selector: CleanupSelector::SourceItem {
            source_id: source_id.clone(),
            source_item_key: source_item_key.clone(),
            generation: previous_generation.clone(),
        },
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}

fn artifact_cleanup_debt_for_metadata(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    metadata: &MetadataMap,
    owner_key: &str,
    seen: &mut BTreeSet<ArtifactId>,
) -> Result<Vec<CleanupDebt>> {
    artifact_refs_from_metadata(metadata)?
        .into_iter()
        .filter(|artifact| seen.insert(artifact.artifact_id.clone()))
        .map(|artifact| {
            Ok(CleanupDebt {
                debt_id: cleanup_debt_id(
                    "artifact",
                    source_id,
                    previous_generation,
                    &format!("{owner_key}:{}", artifact.artifact_id.0),
                ),
                job_id: JobId::new(uuid::Uuid::from_u128(0)),
                source_id: source_id.clone(),
                generation: Some(previous_generation.clone()),
                kind: CleanupDebtKind::ArtifactDelete,
                selector: CleanupSelector::Artifact {
                    artifact_id: artifact.artifact_id,
                },
                status: LifecycleStatus::Pending,
                created_at: timestamp(),
                attempts: 0,
                last_error: None,
                next_retry_at: None,
                completed_at: None,
            })
        })
        .collect()
}

fn cache_cleanup_debt_for_metadata(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    metadata: &MetadataMap,
    source_item_key: &SourceItemKey,
) -> Result<Option<CleanupDebt>> {
    let Some(value) = metadata.get(CACHE_KEY_METADATA_KEY) else {
        return Ok(None);
    };
    let key: DocumentCacheKey = serde_json::from_value(value.clone()).map_err(json_error)?;
    let key_json = serde_json::to_string(&key).map_err(json_error)?;
    Ok(Some(CleanupDebt {
        debt_id: cleanup_debt_id("cache", source_id, previous_generation, &source_item_key.0),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: source_id.clone(),
        generation: Some(previous_generation.clone()),
        kind: CleanupDebtKind::CachePrune,
        selector: CleanupSelector::CacheKeys {
            keys: vec![key_json],
        },
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }))
}

fn artifact_refs_from_metadata(metadata: &MetadataMap) -> Result<Vec<ArtifactRef>> {
    let Some(value) = metadata.get(ARTIFACT_METADATA_KEY) else {
        return Ok(Vec::new());
    };
    serde_json::from_value(value.clone()).map_err(json_error)
}

fn cleanup_debt_id(
    prefix: &str,
    source_id: &SourceId,
    generation: &SourceGenerationId,
    identity: &str,
) -> CleanupDebtId {
    CleanupDebtId::new(format!(
        "debt_{}",
        uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_URL,
            format!("{prefix}:{}:{}:{identity}", source_id.0, generation.0).as_bytes(),
        )
    ))
}
