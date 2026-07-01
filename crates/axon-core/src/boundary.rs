use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use axon_api::source::*;
use tokio::sync::Mutex;

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
pub trait SearchProvider: Send + Sync {
    async fn search(&self, request: SearchRequest) -> Result<SearchResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[async_trait]
pub trait FetchProvider: Send + Sync {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[async_trait]
pub trait RenderProvider: Send + Sync {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[async_trait]
pub trait NetworkCaptureProvider: Send + Sync {
    async fn capture(&self, request: NetworkCaptureRequest) -> Result<NetworkCaptureResult>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
}

#[async_trait]
pub trait CredentialProvider: Send + Sync {
    async fn resolve(&self, request: CredentialRequest) -> Result<CredentialMaterial>;
    async fn capabilities(&self) -> Result<ProviderCapability>;
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

#[derive(Debug, Clone, Default)]
pub struct FakeCoreBoundaries {
    artifacts: Arc<Mutex<BTreeMap<ArtifactId, ArtifactReadResult>>>,
    cache: Arc<Mutex<BTreeMap<DocumentCacheKey, CachedDocument>>>,
}

impl FakeCoreBoundaries {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl ArtifactStore for FakeCoreBoundaries {
    async fn put(&self, artifact: ArtifactWriteRequest) -> Result<ArtifactHandle> {
        let artifact_id = ArtifactId::new(format!("artifact_{}", artifact.content_type));
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
        Ok(capability("fake-artifact", "axon-core").into())
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
        Ok(capability("fake-config", "axon-core").into())
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
        Ok(capability("fake-document-cache", "axon-core").into())
    }
}

#[async_trait]
impl SearchProvider for FakeCoreBoundaries {
    async fn search(&self, request: SearchRequest) -> Result<SearchResult> {
        Ok(SearchResult {
            query: request.query.clone(),
            results: vec![SearchResultItem {
                title: request.query,
                url: "https://example.test/".to_string(),
                snippet: "fake search result".to_string(),
            }],
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::Search))
    }
}

#[async_trait]
impl FetchProvider for FakeCoreBoundaries {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource> {
        Ok(FetchedResource {
            uri: request.uri,
            status: 200,
            content: ContentRef::InlineText {
                text: "fake fetch".to_string(),
            },
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::Fetch))
    }
}

#[async_trait]
impl RenderProvider for FakeCoreBoundaries {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource> {
        Ok(RenderedResource {
            uri: request.uri,
            markdown: "fake render".to_string(),
            artifacts: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::Render))
    }
}

#[async_trait]
impl NetworkCaptureProvider for FakeCoreBoundaries {
    async fn capture(&self, request: NetworkCaptureRequest) -> Result<NetworkCaptureResult> {
        Ok(NetworkCaptureResult {
            uri: request.uri,
            entries: Vec::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::NetworkCapture))
    }
}

#[async_trait]
impl CredentialProvider for FakeCoreBoundaries {
    async fn resolve(&self, request: CredentialRequest) -> Result<CredentialMaterial> {
        Ok(CredentialMaterial {
            secret_ref: request.secret_ref,
            redacted_value: "redacted".to_string(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::Credential))
    }
}

#[async_trait]
impl RateLimiter for FakeCoreBoundaries {
    async fn acquire(&self, request: RateLimitRequest) -> Result<RateLimitPermit> {
        Ok(RateLimitPermit {
            provider_id: request.provider_id,
            units: request.units,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::RateLimiter))
    }
}

#[async_trait]
impl HealthProbe for FakeCoreBoundaries {
    async fn probe(&self, _request: HealthProbeRequest) -> Result<HealthReport> {
        Ok(HealthReport {
            status: HealthStatus::Healthy,
            generated_at: Timestamp("2026-07-01T00:00:00Z".to_string()),
            providers: Vec::new(),
            warnings: Vec::new(),
            metadata: MetadataMap::new(),
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::HealthProbe))
    }
}

fn capability(name: &str, owner_crate: &str) -> CapabilityBase {
    CapabilityBase {
        name: name.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        owner_crate: owner_crate.to_string(),
        health: HealthStatus::Healthy,
        features: vec!["fake".to_string()],
        limits: MetadataMap::new(),
    }
}

fn provider_capability(provider_kind: ProviderKind) -> ProviderCapability {
    ProviderCapability {
        provider_id: ProviderId::new(format!("fake_{provider_kind:?}").to_lowercase()),
        provider_kind,
        implementation: "fake".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        health: HealthStatus::Healthy,
        limits: ProviderLimits::default(),
        features: vec!["fake".to_string()],
        cooldown_until: None,
        last_error: None,
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

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
