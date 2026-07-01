//! Ledger store boundary and in-memory fake.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait LedgerStore: Send + Sync {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()>;
    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>>;
    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()>;
    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff>;
    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration>;
    async fn publish_generation(&self, generation: SourceGeneration) -> Result<()>;
    async fn update_document_status(&self, status: DocumentStatus) -> Result<()>;
    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()>;
    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>>;
    async fn heartbeat_lease(
        &self,
        lease_id: LeaseId,
        heartbeat_at: Timestamp,
        ttl_seconds: u64,
    ) -> Result<Option<LeaseGuard>>;
    async fn release_lease(&self, lease_id: LeaseId) -> Result<()>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<LedgerStoreCapability>;
}

#[derive(Debug, Clone, Default)]
pub struct FakeLedgerStore {
    state: Arc<Mutex<FakeLedgerState>>,
}

#[derive(Debug, Default)]
struct FakeLedgerState {
    sources: BTreeMap<SourceId, SourceSummary>,
    manifests: BTreeMap<(SourceId, SourceGenerationId), SourceManifest>,
    committed: BTreeMap<SourceId, SourceGenerationId>,
    document_statuses: BTreeMap<DocumentId, DocumentStatus>,
    cleanup_debt: BTreeMap<CleanupDebtId, CleanupDebt>,
    leases: BTreeMap<LeaseId, LeaseGuard>,
    lease_ids_by_key: BTreeMap<String, LeaseId>,
    generation_counters: BTreeMap<SourceId, u64>,
}

impl FakeLedgerStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn committed_generation(&self, source_id: &SourceId) -> Option<SourceGenerationId> {
        self.state.lock().await.committed.get(source_id).cloned()
    }

    pub async fn document_status(&self, document_id: &DocumentId) -> Option<DocumentStatus> {
        self.state
            .lock()
            .await
            .document_statuses
            .get(document_id)
            .cloned()
    }

    pub async fn cleanup_debt_count(&self) -> usize {
        self.state.lock().await.cleanup_debt.len()
    }
}

#[async_trait]
impl LedgerStore for FakeLedgerStore {
    async fn upsert_source(&self, source: SourceSummary) -> Result<()> {
        self.state
            .lock()
            .await
            .sources
            .insert(source.source_id.clone(), source);
        Ok(())
    }

    async fn get_source(&self, source_id: SourceId) -> Result<Option<SourceSummary>> {
        Ok(self.state.lock().await.sources.get(&source_id).cloned())
    }

    async fn put_manifest(&self, manifest: SourceManifest) -> Result<()> {
        let key = (manifest.source_id.clone(), manifest.generation.clone());
        self.state.lock().await.manifests.insert(key, manifest);
        Ok(())
    }

    async fn diff_manifest(&self, manifest: SourceManifest) -> Result<SourceManifestDiff> {
        let state = self.state.lock().await;
        let previous_generation = state.committed.get(&manifest.source_id).cloned();
        let previous = previous_generation
            .as_ref()
            .and_then(|generation| {
                state
                    .manifests
                    .get(&(manifest.source_id.clone(), generation.clone()))
            })
            .cloned();
        drop(state);
        let previous_items = previous
            .map(|old| keyed_manifest_items(old.items))
            .unwrap_or_default();
        let SourceManifest {
            source_id,
            generation,
            items,
            ..
        } = manifest;
        let next_items = keyed_manifest_items(items);

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut unchanged = Vec::new();
        for (key, item) in &next_items {
            match previous_items.get(key) {
                None => added.push(item.clone()),
                Some(old) if manifest_item_changed(old, item) => modified.push(item.clone()),
                Some(_) => unchanged.push(item.clone()),
            }
        }

        let next_keys = next_items.keys().cloned().collect::<BTreeSet<_>>();
        let removed = previous_items
            .into_iter()
            .filter_map(|(key, item)| (!next_keys.contains(&key)).then_some(item))
            .collect::<Vec<_>>();

        Ok(SourceManifestDiff {
            header: stage_header(PipelinePhase::Diffing),
            source_id,
            previous_generation,
            next_generation: generation,
            counts: DiffCounts {
                added: added.len() as u64,
                modified: modified.len() as u64,
                removed: removed.len() as u64,
                unchanged: unchanged.len() as u64,
                skipped: 0,
                failed: 0,
            },
            added,
            modified,
            removed,
            unchanged,
            skipped: Vec::new(),
            failed: Vec::new(),
        })
    }

    async fn create_generation(&self, source_id: SourceId) -> Result<SourceGeneration> {
        let mut state = self.state.lock().await;
        let counter = state
            .generation_counters
            .entry(source_id.clone())
            .and_modify(|counter| *counter += 1)
            .or_insert(1);
        let generation = SourceGenerationId::new(format!("gen_{counter}"));
        Ok(SourceGeneration {
            source_id: source_id.clone(),
            generation,
            status: LifecycleStatus::Running,
            publish_state: PublishState::Writing,
            created_at: timestamp(),
            published_at: None,
            item_counts: ItemCounts {
                added: 0,
                modified: 0,
                removed: 0,
                unchanged: 0,
                failed: 0,
            },
            document_counts: DocumentCounts {
                discovered: 0,
                prepared: 0,
                embedded: 0,
                published: 0,
                failed: 0,
            },
            cleanup_debt: Vec::new(),
            previous_generation: state.committed.get(&source_id).cloned(),
        })
    }

    async fn publish_generation(&self, generation: SourceGeneration) -> Result<()> {
        if !matches!(
            generation.status,
            LifecycleStatus::Completed | LifecycleStatus::CompletedDegraded
        ) {
            return Err(ApiError::new(
                "source.ledger.generation_not_publishable",
                ErrorStage::Publishing,
                format!(
                    "generation {} has non-publishable status {:?}",
                    generation.generation.0, generation.status
                ),
            )
            .with_source_id(generation.source_id.0));
        }
        let mut state = self.state.lock().await;
        if !state
            .manifests
            .contains_key(&(generation.source_id.clone(), generation.generation.clone()))
        {
            return Err(ApiError::new(
                "source.ledger.manifest_missing",
                ErrorStage::Publishing,
                format!(
                    "generation {} cannot publish without a manifest",
                    generation.generation.0
                ),
            )
            .with_source_id(generation.source_id.0));
        }
        let committed = state.committed.get(&generation.source_id).cloned();
        if committed != generation.previous_generation {
            return Err(ApiError::new(
                "source.ledger.generation_baseline_changed",
                ErrorStage::Publishing,
                format!(
                    "generation {} was based on {:?}, but committed generation is {:?}",
                    generation.generation.0, generation.previous_generation, committed
                ),
            )
            .with_source_id(generation.source_id.0));
        }
        state
            .committed
            .insert(generation.source_id, generation.generation);
        Ok(())
    }

    async fn update_document_status(&self, status: DocumentStatus) -> Result<()> {
        self.state
            .lock()
            .await
            .document_statuses
            .insert(status.document_id.clone(), status);
        Ok(())
    }

    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()> {
        self.state
            .lock()
            .await
            .cleanup_debt
            .insert(debt.debt_id.clone(), debt);
        Ok(())
    }

    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>> {
        let mut state = self.state.lock().await;
        if let Some(existing_id) = state.lease_ids_by_key.get(&request.lease_key).cloned() {
            let existing = state.leases.get(&existing_id).cloned();
            match existing {
                Some(existing) if existing.expires_at.0 > request.acquired_at.0 => {
                    return Ok(None);
                }
                Some(_) | None => {
                    state.leases.remove(&existing_id);
                    state.lease_ids_by_key.remove(&request.lease_key);
                }
            }
        }

        let guard = LeaseGuard {
            lease_id: LeaseId::new(format!("lease_{}", uuid::Uuid::new_v4())),
            lease_key: request.lease_key,
            owner_id: request.owner_id,
            expires_at: add_seconds(&request.acquired_at, request.ttl_seconds),
            heartbeat_at: request.acquired_at.clone(),
            acquired_at: request.acquired_at,
            job_id: request.job_id,
            metadata: request.metadata,
        };
        state
            .lease_ids_by_key
            .insert(guard.lease_key.clone(), guard.lease_id.clone());
        state.leases.insert(guard.lease_id.clone(), guard.clone());
        Ok(Some(guard))
    }

    async fn release_lease(&self, lease_id: LeaseId) -> Result<()> {
        let mut state = self.state.lock().await;
        if let Some(guard) = state.leases.remove(&lease_id) {
            state.lease_ids_by_key.remove(&guard.lease_key);
        }
        Ok(())
    }

    async fn heartbeat_lease(
        &self,
        lease_id: LeaseId,
        heartbeat_at: Timestamp,
        ttl_seconds: u64,
    ) -> Result<Option<LeaseGuard>> {
        let mut state = self.state.lock().await;
        let Some(existing) = state.leases.get(&lease_id).cloned() else {
            return Ok(None);
        };
        let guard = LeaseGuard {
            heartbeat_at: heartbeat_at.clone(),
            expires_at: add_seconds(&heartbeat_at, ttl_seconds),
            ..existing
        };
        state.leases.insert(lease_id, guard.clone());
        Ok(Some(guard))
    }

    async fn reset(&self) -> Result<()> {
        *self.state.lock().await = FakeLedgerState::default();
        Ok(())
    }

    async fn capabilities(&self) -> Result<LedgerStoreCapability> {
        Ok(CapabilityBase {
            name: "fake-ledger".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            owner_crate: "axon-ledger".to_string(),
            health: HealthStatus::Healthy,
            features: vec![
                "manifest_diff".to_string(),
                "generation_publish".to_string(),
                "document_status".to_string(),
                "cleanup_debt".to_string(),
                "leases".to_string(),
            ],
            limits: MetadataMap::new(),
        }
        .into())
    }
}

fn keyed_manifest_items(items: Vec<ManifestItem>) -> BTreeMap<SourceItemKey, ManifestItem> {
    items
        .into_iter()
        .map(|item| (item.source_item_key.clone(), item))
        .collect()
}

fn manifest_item_changed(old: &ManifestItem, next: &ManifestItem) -> bool {
    old.content_hash != next.content_hash || old.version != next.version || old.mtime != next.mtime
}

fn stage_header(phase: PipelinePhase) -> StageResultHeader {
    StageResultHeader {
        job_id: JobId::new(uuid::Uuid::from_u128(0)),
        stage_id: StageId::new(uuid::Uuid::from_u128(0)),
        phase,
        status: LifecycleStatus::Completed,
        started_at: timestamp(),
        completed_at: Some(timestamp()),
        counts: StageCounts {
            items_total: None,
            items_done: 0,
            documents_total: None,
            documents_done: 0,
            chunks_total: None,
            chunks_done: 0,
            bytes_total: None,
            bytes_done: 0,
        },
        warnings: Vec::new(),
        error: None,
    }
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

fn add_seconds(timestamp: &Timestamp, seconds: u64) -> Timestamp {
    let parsed = chrono::DateTime::parse_from_rfc3339(&timestamp.0)
        .map(|value| value.with_timezone(&chrono::Utc));
    match parsed {
        Ok(value) => Timestamp((value + chrono::Duration::seconds(seconds as i64)).to_rfc3339()),
        Err(_) => timestamp.clone(),
    }
}
