//! Shared cleanup-debt row builders.
//!
//! The fake and SQLite ledgers query different backing stores, but the cleanup
//! debt rows they produce must stay byte-for-byte aligned. Keep pure row
//! construction here and leave only lookup/insertion policy in each backend.

pub const MODULE_NAME: &str = "cleanup_debt";

use std::collections::BTreeSet;

use axon_api::source::*;

pub(crate) fn vector_delete_debt(
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

pub(crate) fn artifact_delete_debt_for_metadata(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    metadata: &MetadataMap,
    owner_key: &str,
    seen: &mut BTreeSet<ArtifactId>,
) -> serde_json::Result<Vec<CleanupDebt>> {
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

pub(crate) fn cache_prune_debt_for_metadata(
    source_id: &SourceId,
    previous_generation: &SourceGenerationId,
    metadata: &MetadataMap,
    source_item_key: &SourceItemKey,
) -> serde_json::Result<Option<CleanupDebt>> {
    let Some(value) = metadata.get(CACHE_KEY_METADATA_KEY) else {
        return Ok(None);
    };
    let key: DocumentCacheKey = serde_json::from_value(value.clone())?;
    let key_json = serde_json::to_string(&key)?;
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

pub(crate) fn graph_prune_debt(
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
                    "graph:{}:{}:{}",
                    source_id.0, previous_generation.0, source_item_key.0
                )
                .as_bytes(),
            )
        )),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: source_id.clone(),
        generation: Some(previous_generation.clone()),
        kind: CleanupDebtKind::GraphPrune,
        selector: CleanupSelector::GraphNodes {
            stable_keys: vec![source_item_key.0.clone()],
        },
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}

pub(crate) fn ledger_prune_debt(
    source_id: &SourceId,
    generation: &SourceGenerationId,
) -> CleanupDebt {
    CleanupDebt {
        debt_id: CleanupDebtId::new(format!(
            "debt_{}",
            uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_URL,
                format!("ledger:{}:{}", source_id.0, generation.0).as_bytes(),
            )
        )),
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        source_id: source_id.clone(),
        generation: Some(generation.clone()),
        kind: CleanupDebtKind::LedgerPrune,
        selector: CleanupSelector::LedgerGenerations {
            source_id: source_id.clone(),
            up_to_generation: generation.clone(),
        },
        status: LifecycleStatus::Pending,
        created_at: timestamp(),
        attempts: 0,
        last_error: None,
        next_retry_at: None,
        completed_at: None,
    }
}

fn artifact_refs_from_metadata(metadata: &MetadataMap) -> serde_json::Result<Vec<ArtifactRef>> {
    let Some(value) = metadata.get(ARTIFACT_METADATA_KEY) else {
        return Ok(Vec::new());
    };
    serde_json::from_value(value.clone())
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

fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

const ARTIFACT_METADATA_KEY: &str = "_axon_artifacts";
const CACHE_KEY_METADATA_KEY: &str = "_axon_document_cache_key";
