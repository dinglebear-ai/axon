//! Adapter-owned provider boundaries and fakes.

use async_trait::async_trait;
use axon_api::source::*;

pub type Result<T> = std::result::Result<T, ApiError>;

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

#[derive(Debug, Clone, Default)]
pub struct FakeAdapterProviders;

impl FakeAdapterProviders {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl SearchProvider for FakeAdapterProviders {
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
impl FetchProvider for FakeAdapterProviders {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource> {
        Ok(FetchedResource {
            uri: request.uri.clone(),
            final_uri: request.uri,
            status: 200,
            content: ContentRef::InlineText {
                text: "fake fetch".to_string(),
            },
            headers: RedactedHeaders {
                headers: Vec::new(),
            },
            fetched_at: timestamp(),
            etag: Some("fake-etag".to_string()),
            redirect_chain: Vec::new(),
            bytes: Some(10),
            metadata: request.metadata,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::Fetch))
    }
}

#[async_trait]
impl RenderProvider for FakeAdapterProviders {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource> {
        Ok(RenderedResource {
            uri: request.uri.clone(),
            final_uri: request.uri,
            markdown: "fake render".to_string(),
            html: Some("<p>fake render</p>".to_string()),
            text: Some("fake render".to_string()),
            render_mode: request.mode,
            captured_at: timestamp(),
            artifacts: Vec::new(),
            console: Vec::new(),
            network: Vec::new(),
            metadata: request.metadata,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::Render))
    }
}

#[async_trait]
impl NetworkCaptureProvider for FakeAdapterProviders {
    async fn capture(&self, request: NetworkCaptureRequest) -> Result<NetworkCaptureResult> {
        Ok(NetworkCaptureResult {
            uri: request.uri,
            captured_at: timestamp(),
            entries: Vec::new(),
            artifacts: Vec::new(),
            metadata: request.metadata,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(provider_capability(ProviderKind::NetworkCapture))
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

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
