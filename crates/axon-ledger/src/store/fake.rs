use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

mod cleanup;
mod lease;

use super::util::*;
use super::{LedgerStore, Result};
use crate::validation::{
    ensure_generation_publishable, ensure_generation_writable, generation_already_published_error,
    manifest_missing_error, validate_manifest,
};
use cleanup::record_removed_item_cleanup_debt;

#[derive(Debug, Clone, Default)]
pub struct FakeLedgerStore {
    state: Arc<Mutex<FakeLedgerState>>,
    mode: FakeLedgerMode,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum FakeLedgerMode {
    #[default]
    Success,
    PublishFailure,
    HeartbeatLost,
}

#[derive(Debug, Default)]
struct FakeLedgerState {
    sources: BTreeMap<SourceId, SourceSummary>,
    generations: BTreeMap<(SourceId, SourceGenerationId), SourceGeneration>,
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

    pub fn with_publish_generation_failure(mut self) -> Self {
        self.mode = FakeLedgerMode::PublishFailure;
        self
    }

    pub fn with_heartbeat_lost(mut self) -> Self {
        self.mode = FakeLedgerMode::HeartbeatLost;
        self
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

    pub async fn cleanup_debt(&self, debt_id: &CleanupDebtId) -> Option<CleanupDebt> {
        self.state.lock().await.cleanup_debt.get(debt_id).cloned()
    }

    pub async fn cleanup_debt_count(&self) -> usize {
        self.state.lock().await.cleanup_debt.len()
    }

    pub async fn generation_count(&self) -> usize {
        self.state.lock().await.generations.len()
    }

    pub async fn generation(
        &self,
        source_id: &SourceId,
        generation: &SourceGenerationId,
    ) -> Option<SourceGeneration> {
        self.state
            .lock()
            .await
            .generations
            .get(&(source_id.clone(), generation.clone()))
            .cloned()
    }

    pub async fn manifest_count(&self) -> usize {
        self.state.lock().await.manifests.len()
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
        validate_manifest(&manifest)?;
        let mut state = self.state.lock().await;
        if !state.sources.contains_key(&manifest.source_id) {
            return Err(source_missing_error(&manifest.source_id));
        }
        let key = (manifest.source_id.clone(), manifest.generation.clone());
        let previous_generation = state.committed.get(&manifest.source_id).cloned();
        state
            .generations
            .entry(key.clone())
            .or_insert_with(|| SourceGeneration {
                source_id: manifest.source_id.clone(),
                generation: manifest.generation.clone(),
                status: LifecycleStatus::Running,
                publish_state: PublishState::Writing,
                created_at: manifest.created_at.clone(),
                published_at: None,
                item_counts: ItemCounts {
                    added: 0,
                    modified: 0,
                    removed: 0,
                    unchanged: manifest.items.len() as u64,
                    failed: 0,
                },
                document_counts: DocumentCounts {
                    discovered: manifest.items.len() as u64,
                    prepared: 0,
                    embedded: 0,
                    published: 0,
                    failed: 0,
                },
                cleanup_debt: Vec::new(),
                previous_generation,
            });
        state.manifests.insert(key, manifest);
        Ok(())
    }

    async fn get_manifest(
        &self,
        source_id: SourceId,
        generation: SourceGenerationId,
    ) -> Result<Option<SourceManifest>> {
        Ok(self
            .state
            .lock()
            .await
            .manifests
            .get(&(source_id, generation))
            .cloned())
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
        if !state.sources.contains_key(&source_id) {
            return Err(source_missing_error(&source_id));
        }
        let mut sequence = state
            .generation_counters
            .get(&source_id)
            .copied()
            .unwrap_or(0)
            + 1;
        while state.generations.contains_key(&(
            source_id.clone(),
            SourceGenerationId::new(format!("gen_{sequence}")),
        )) {
            sequence += 1;
        }
        state
            .generation_counters
            .insert(source_id.clone(), sequence);
        let generation = SourceGenerationId::new(format!("gen_{sequence}"));
        let generation = SourceGeneration {
            source_id: source_id.clone(),
            generation: generation.clone(),
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
        };
        state.generations.insert(
            (source_id, generation.generation.clone()),
            generation.clone(),
        );
        Ok(generation)
    }

    async fn committed_generation(
        &self,
        source_id: SourceId,
    ) -> Result<Option<SourceGenerationId>> {
        Ok(self.state.lock().await.committed.get(&source_id).cloned())
    }

    async fn complete_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        ensure_generation_publishable(&generation)?;
        let mut state = self.state.lock().await;
        if !state.sources.contains_key(&generation.source_id) {
            return Err(source_missing_error(&generation.source_id));
        }
        let key = (generation.source_id.clone(), generation.generation.clone());
        let Some(stored) = state.generations.get(&key).cloned() else {
            return Err(generation_missing_error(
                &generation.source_id,
                &generation.generation,
            ));
        };
        ensure_generation_writable(&stored)?;
        if !state
            .manifests
            .contains_key(&(generation.source_id.clone(), generation.generation.clone()))
        {
            return Err(manifest_missing_error(&generation));
        }
        if stored.previous_generation != generation.previous_generation {
            return Err(ApiError::new(
                "source.ledger.generation_baseline_changed",
                ErrorStage::Publishing,
                format!(
                    "generation {} was based on {:?}, but stored generation is based on {:?}",
                    generation.generation.0,
                    generation.previous_generation,
                    stored.previous_generation
                ),
            )
            .with_source_id(generation.source_id.0));
        }

        let mut completed = generation;
        completed.publish_state = PublishState::Writing;
        completed.published_at = None;
        completed.cleanup_debt = Vec::new();
        completed.created_at = stored.created_at;
        state.generations.insert(key, completed.clone());
        Ok(completed)
    }

    async fn fail_generation(&self, generation: SourceGeneration) -> Result<SourceGeneration> {
        let mut state = self.state.lock().await;
        let key = (generation.source_id.clone(), generation.generation.clone());
        let Some(stored) = state.generations.get(&key).cloned() else {
            return Err(generation_missing_error(
                &generation.source_id,
                &generation.generation,
            ));
        };
        if stored.published_at.is_some() || stored.publish_state != PublishState::Writing {
            return Err(generation_already_published_error(&stored));
        }
        let mut failed = generation;
        failed.created_at = stored.created_at;
        failed.published_at = None;
        failed.publish_state = PublishState::Writing;
        failed.status = LifecycleStatus::Failed;
        state.generations.insert(key, failed.clone());
        Ok(failed)
    }

    async fn publish_generation(
        &self,
        request: PublishGenerationRequest,
    ) -> Result<SourceGeneration> {
        if self.mode == FakeLedgerMode::PublishFailure {
            return Err(ApiError::new(
                "source.ledger.publish_failed",
                ErrorStage::Publishing,
                "fake ledger failed to publish generation",
            )
            .with_source_id(request.source_id.0));
        }
        let mut state = self.state.lock().await;
        let Some(generation) = state
            .generations
            .get(&(request.source_id.clone(), request.generation.clone()))
            .cloned()
        else {
            return Err(generation_missing_error(
                &request.source_id,
                &request.generation,
            ));
        };
        ensure_generation_publishable(&generation)?;
        if !state
            .manifests
            .contains_key(&(generation.source_id.clone(), generation.generation.clone()))
        {
            return Err(manifest_missing_error(&generation));
        }
        let committed = state.committed.get(&generation.source_id).cloned();
        if committed != request.expected_previous_generation
            || generation.previous_generation != request.expected_previous_generation
        {
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
        record_removed_item_cleanup_debt(&mut state, &generation);
        let cleanup_debt = state
            .cleanup_debt
            .values()
            .filter(|debt| {
                debt.source_id == generation.source_id
                    && debt.generation == generation.previous_generation
                    && matches!(debt.kind, CleanupDebtKind::VectorDelete)
            })
            .map(|debt| debt.debt_id.clone())
            .collect::<Vec<_>>();
        let mut published = generation.clone();
        published.publish_state = if cleanup_debt.is_empty() {
            PublishState::Committed
        } else {
            PublishState::CleanupPending
        };
        published.published_at = Some(timestamp());
        published.cleanup_debt = cleanup_debt;
        state.generations.insert(
            (published.source_id.clone(), published.generation.clone()),
            published.clone(),
        );
        state
            .committed
            .insert(published.source_id.clone(), published.generation.clone());
        Ok(published)
    }

    async fn update_document_status(&self, status: DocumentStatus) -> Result<()> {
        let mut state = self.state.lock().await;
        if !state.sources.contains_key(&status.source_id) {
            return Err(source_missing_error(&status.source_id));
        }
        if state
            .document_statuses
            .get(&status.document_id)
            .is_some_and(|existing| existing.updated_at.0 > status.updated_at.0)
        {
            return Ok(());
        }
        state
            .document_statuses
            .insert(status.document_id.clone(), status);
        Ok(())
    }

    async fn record_cleanup_debt(&self, debt: CleanupDebt) -> Result<()> {
        validate_cleanup_debt(&debt)?;
        let mut state = self.state.lock().await;
        if !state.sources.contains_key(&debt.source_id) {
            return Err(source_missing_error(&debt.source_id));
        }
        let key = cleanup_debt_natural_key(&debt)?;
        let existing_id = state.cleanup_debt.iter().find_map(|(id, existing)| {
            cleanup_debt_natural_key(existing)
                .ok()
                .filter(|existing_key| existing_key == &key)
                .map(|_| id.clone())
        });
        if let Some(existing_id) = existing_id {
            if let Some(mut existing) = state.cleanup_debt.remove(&existing_id) {
                apply_cleanup_debt_update(&mut existing, debt);
                state
                    .cleanup_debt
                    .insert(existing.debt_id.clone(), existing);
            }
            return Ok(());
        }
        state.cleanup_debt.insert(debt.debt_id.clone(), debt);
        Ok(())
    }

    async fn list_pending_cleanup_debt(&self, source_id: SourceId) -> Result<Vec<CleanupDebt>> {
        cleanup::list_pending_cleanup_debt(&self.state, &source_id).await
    }

    async fn resolve_cleanup_debt(&self, debt_id: CleanupDebtId) -> Result<()> {
        cleanup::resolve_cleanup_debt(&self.state, &debt_id).await
    }

    async fn acquire_lease(&self, request: LeaseRequest) -> Result<Option<LeaseGuard>> {
        lease::acquire_lease(&self.state, request).await
    }

    async fn release_lease(&self, lease_id: LeaseId, owner_id: String) -> Result<()> {
        lease::release_lease(&self.state, lease_id, owner_id).await
    }

    async fn heartbeat_lease(
        &self,
        lease_id: LeaseId,
        owner_id: String,
        ttl_seconds: u64,
    ) -> Result<Option<LeaseGuard>> {
        if self.mode == FakeLedgerMode::HeartbeatLost {
            return Ok(None);
        }
        lease::heartbeat_lease(&self.state, lease_id, owner_id, ttl_seconds).await
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
