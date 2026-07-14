use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

mod file_artifact_store;
pub use file_artifact_store::FileArtifactStore;

pub type Result<T> = std::result::Result<T, ApiError>;

#[async_trait]
pub trait ArtifactStore: Send + Sync {
    async fn put(&self, artifact: ArtifactWriteRequest) -> Result<ArtifactHandle>;
    async fn get(&self, handle: ArtifactHandle) -> Result<ArtifactReadResult>;
    async fn delete(&self, handle: ArtifactHandle) -> Result<()>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<ArtifactStoreCapability>;
}

#[async_trait]
pub trait ConfigStore: Send + Sync {
    async fn load(&self) -> Result<EffectiveConfig>;
    async fn validate(&self) -> Result<ConfigValidationReport>;
    async fn snapshot(&self) -> Result<ConfigSnapshotId>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<ConfigStoreCapability>;
}

#[async_trait]
pub trait DocumentCache: Send + Sync {
    async fn get(&self, key: DocumentCacheKey) -> Result<Option<CachedDocument>>;
    async fn put(&self, key: DocumentCacheKey, value: CachedDocument) -> Result<()>;
    async fn invalidate(&self, selector: DocumentCacheInvalidation) -> Result<()>;
    async fn reset(&self) -> Result<()>;
    async fn capabilities(&self) -> Result<DocumentCacheCapability>;
}

#[async_trait]
pub trait RateLimiter: Send + Sync {
    async fn acquire(&self, request: RateLimitRequest) -> Result<RateLimitPermit>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[async_trait]
pub trait HealthProbe: Send + Sync {
    async fn probe(&self, request: HealthProbeRequest) -> Result<HealthReport>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[derive(Debug, Clone)]
pub struct FakeCoreBoundaries {
    artifacts: Arc<Mutex<BTreeMap<ArtifactId, ArtifactReadResult>>>,
    artifact_counter: Arc<Mutex<u64>>,
    cache: Arc<Mutex<BTreeMap<DocumentCacheKey, CachedDocument>>>,
    health: HealthStatus,
    mode: FakeCoreMode,
    calls: Arc<StdMutex<Vec<&'static str>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeCoreMode {
    Success,
    Timeout,
    RateLimited,
    Fatal,
}

impl FakeCoreBoundaries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_health(mut self, health: HealthStatus) -> Self {
        self.health = health;
        self
    }

    pub fn with_mode(mut self, mode: FakeCoreMode) -> Self {
        self.mode = mode;
        self
    }

    pub async fn calls(&self) -> Vec<&'static str> {
        self.calls
            .lock()
            .expect("core fake call log mutex poisoned")
            .clone()
    }

    fn record(&self, call: &'static str) {
        self.calls
            .lock()
            .expect("core fake call log mutex poisoned")
            .push(call);
    }

    fn mode_error(&self, stage: ErrorStage, provider_id: Option<String>) -> Option<ApiError> {
        fake_provider_mode_error(
            self.mode_state(),
            provider_id.as_deref().unwrap_or("fake-core"),
            stage,
            "core provider",
        )
    }

    fn mode_state(&self) -> FakeProviderModeState {
        match self.mode {
            FakeCoreMode::Success => FakeProviderModeState::Success,
            FakeCoreMode::Timeout => FakeProviderModeState::Timeout,
            FakeCoreMode::RateLimited => FakeProviderModeState::RateLimited,
            FakeCoreMode::Fatal => FakeProviderModeState::Fatal,
        }
    }

    fn capability_state(&self, provider_id: &str) -> FakeProviderCapabilityState {
        let mut state = fake_provider_capability_state(
            self.mode_state(),
            provider_id,
            ErrorStage::Observing,
            "core provider",
        );
        if self.health != HealthStatus::Healthy {
            state.health = self.health;
        }
        state
    }

    fn provider_capability(&self, provider_kind: ProviderKind) -> ProviderCapability {
        let provider_id = provider_id(provider_kind);
        let state = self.capability_state(&provider_id);
        ProviderCapability {
            provider_id: ProviderId::new(provider_id.clone()),
            provider_kind,
            implementation: "fake".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health: state.health,
            limits: ProviderLimits::default(),
            features: vec!["fake".to_string()],
            cooldown_until: state.cooldown_until,
            last_error: state.last_error,
            reservation_policy: ReservationPolicy {
                supports_reservations: false,
                queue_policy: QueuePolicy::Fifo,
                interactive_reserve: 0,
                cooldown_after_failures: 0,
                cooldown_secs: 0,
                retry_backoff_ms: None,
            },
            reservation_state: ReservationStateSnapshot {
                queued: 0,
                active: 0,
                available_units: 1,
                oldest_queued_ms: None,
                priority_breakdown: Default::default(),
                states: Vec::new(),
            },
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: true,
            embedding: None,
            llm: None,
            vector_store: None,
            fetch: None,
            render: None,
            credential: None,
        }
    }
}

impl Default for FakeCoreBoundaries {
    fn default() -> Self {
        Self {
            artifacts: Arc::new(Mutex::new(BTreeMap::new())),
            artifact_counter: Arc::new(Mutex::new(0)),
            cache: Arc::new(Mutex::new(BTreeMap::new())),
            health: HealthStatus::Healthy,
            mode: FakeCoreMode::Success,
            calls: Arc::new(StdMutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl ArtifactStore for FakeCoreBoundaries {
    async fn put(&self, artifact: ArtifactWriteRequest) -> Result<ArtifactHandle> {
        let mut counter = self.artifact_counter.lock().await;
        *counter += 1;
        let artifact_id = ArtifactId::new(format!(
            "artifact_{}_{}",
            artifact.content_type.replace('/', "_"),
            *counter
        ));
        drop(counter);
        let handle = ArtifactHandle {
            artifact_id: artifact_id.clone(),
            artifact_kind: artifact.kind,
            uri: Some(format!("fake://artifact/{}", artifact_id.0)),
        };
        self.artifacts.lock().await.insert(
            artifact_id,
            ArtifactReadResult {
                handle: handle.clone(),
                content_type: artifact.content_type,
                content: Some(artifact.content),
                metadata: artifact.metadata,
            },
        );
        Ok(handle)
    }

    async fn get(&self, handle: ArtifactHandle) -> Result<ArtifactReadResult> {
        self.artifacts
            .lock()
            .await
            .get(&handle.artifact_id)
            .cloned()
            .ok_or_else(|| {
                ApiError::new(
                    "artifact.not_found",
                    ErrorStage::Retrieving,
                    "artifact not found",
                )
            })
    }

    async fn delete(&self, handle: ArtifactHandle) -> Result<()> {
        self.artifacts.lock().await.remove(&handle.artifact_id);
        Ok(())
    }

    async fn reset(&self) -> Result<()> {
        self.artifacts.lock().await.clear();
        Ok(())
    }

    async fn capabilities(&self) -> Result<ArtifactStoreCapability> {
        Ok(capability("fake-artifact", "axon-core", self.health).into())
    }
}

#[async_trait]
impl ConfigStore for FakeCoreBoundaries {
    async fn load(&self) -> Result<EffectiveConfig> {
        Ok(EffectiveConfig {
            snapshot_id: ConfigSnapshotId::new("cfg_fake"),
            values: MetadataMap::new(),
        })
    }

    async fn validate(&self) -> Result<ConfigValidationReport> {
        Ok(ConfigValidationReport {
            valid: true,
            warnings: Vec::new(),
        })
    }

    async fn snapshot(&self) -> Result<ConfigSnapshotId> {
        Ok(ConfigSnapshotId::new("cfg_fake"))
    }

    async fn reset(&self) -> Result<()> {
        Ok(())
    }

    async fn capabilities(&self) -> Result<ConfigStoreCapability> {
        Ok(capability("fake-config", "axon-core", self.health).into())
    }
}

#[async_trait]
impl DocumentCache for FakeCoreBoundaries {
    async fn get(&self, key: DocumentCacheKey) -> Result<Option<CachedDocument>> {
        Ok(self.cache.lock().await.get(&key).cloned())
    }

    async fn put(&self, key: DocumentCacheKey, value: CachedDocument) -> Result<()> {
        self.cache.lock().await.insert(key, value);
        Ok(())
    }

    async fn invalidate(&self, selector: DocumentCacheInvalidation) -> Result<()> {
        let mut cache = self.cache.lock().await;
        match selector {
            DocumentCacheInvalidation::Key { key } => {
                cache.remove(&key);
            }
            DocumentCacheInvalidation::All => cache.clear(),
            DocumentCacheInvalidation::Source { source_id } => {
                cache.retain(|key, _| key.source_id != source_id);
            }
            DocumentCacheInvalidation::Generation { generation } => {
                cache.retain(|key, _| key.generation.as_ref() != Some(&generation));
            }
        }
        Ok(())
    }

    async fn reset(&self) -> Result<()> {
        self.cache.lock().await.clear();
        Ok(())
    }

    async fn capabilities(&self) -> Result<DocumentCacheCapability> {
        Ok(capability("fake-document-cache", "axon-core", self.health).into())
    }
}

#[async_trait]
impl RateLimiter for FakeCoreBoundaries {
    async fn acquire(&self, request: RateLimitRequest) -> Result<RateLimitPermit> {
        self.record("rate_limit.acquire");
        if let Some(err) =
            self.mode_error(ErrorStage::Planning, Some(request.provider_id.0.clone()))
        {
            return Err(err);
        }
        Ok(RateLimitPermit {
            provider_id: request.provider_id,
            units: request.units,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(self.provider_capability(ProviderKind::RateLimiter))
    }
}

#[async_trait]
impl HealthProbe for FakeCoreBoundaries {
    async fn probe(&self, _request: HealthProbeRequest) -> Result<HealthReport> {
        self.record("health.probe");
        if let Some(err) =
            self.mode_error(ErrorStage::Observing, Some(_request.provider_id.0.clone()))
        {
            return Err(err);
        }
        Ok(HealthReport {
            status: self.health,
            generated_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
            providers: Vec::new(),
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(self.provider_capability(ProviderKind::HealthProbe))
    }
}

fn capability(name: &str, owner_crate: &str, health: HealthStatus) -> CapabilityBase {
    CapabilityBase {
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        owner_crate: owner_crate.to_string(),
        health,
        features: vec!["fake".to_string()],
        limits: MetadataMap::new(),
    }
}

fn provider_id(provider_kind: ProviderKind) -> String {
    format!("fake_{provider_kind:?}").to_lowercase()
}

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
