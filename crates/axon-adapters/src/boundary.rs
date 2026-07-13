//! Adapter-owned provider boundaries and fakes.

use std::sync::{Arc, Mutex};

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

#[derive(Debug, Clone)]
pub struct FakeAdapterProviders {
    health: HealthStatus,
    mode: FakeAdapterMode,
    calls: Arc<Mutex<Vec<&'static str>>>,
    fetch_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FakeAdapterMode {
    Success,
    Timeout,
    RateLimited,
    Fatal,
}

impl FakeAdapterProviders {
    pub fn new() -> Self {
        Self {
            health: HealthStatus::Healthy,
            mode: FakeAdapterMode::Success,
            calls: Arc::new(Mutex::new(Vec::new())),
            fetch_text: "fake fetch".to_string(),
        }
    }

    pub fn with_health(mut self, health: HealthStatus) -> Self {
        self.health = health;
        self
    }

    pub fn with_mode(mut self, mode: FakeAdapterMode) -> Self {
        self.mode = mode;
        self
    }

    /// Override the body `fetch()` returns (default: `"fake fetch"`). Lets
    /// tests simulate two distinct fetched page bodies — e.g. to prove a
    /// content_hash derived from the fetch result actually varies with the
    /// content, rather than being a hardcoded constant.
    pub fn with_fetch_text(mut self, text: impl Into<String>) -> Self {
        self.fetch_text = text.into();
        self
    }

    pub async fn calls(&self) -> Vec<&'static str> {
        self.calls
            .lock()
            .expect("adapter fake call log mutex poisoned")
            .clone()
    }

    fn record(&self, call: &'static str) {
        self.calls
            .lock()
            .expect("adapter fake call log mutex poisoned")
            .push(call);
    }

    fn mode_error(
        &self,
        stage: axon_error::ErrorStage,
        provider_kind: ProviderKind,
    ) -> Option<ApiError> {
        fake_provider_mode_error(
            self.mode_state(),
            &provider_id(provider_kind),
            stage,
            "adapter provider",
        )
    }

    fn mode_state(&self) -> FakeProviderModeState {
        match self.mode {
            FakeAdapterMode::Success => FakeProviderModeState::Success,
            FakeAdapterMode::Timeout => FakeProviderModeState::Timeout,
            FakeAdapterMode::RateLimited => FakeProviderModeState::RateLimited,
            FakeAdapterMode::Fatal => FakeProviderModeState::Fatal,
        }
    }

    fn capability_state(&self, provider_kind: ProviderKind) -> FakeProviderCapabilityState {
        let mut state = fake_provider_capability_state(
            self.mode_state(),
            &provider_id(provider_kind),
            axon_error::ErrorStage::Observing,
            "adapter provider",
        );
        if self.health != HealthStatus::Healthy {
            state.health = self.health;
        }
        state
    }

    fn provider_capability(&self, provider_kind: ProviderKind) -> ProviderCapability {
        let state = self.capability_state(provider_kind);
        ProviderCapability {
            provider_id: ProviderId::new(provider_id(provider_kind)),
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

impl Default for FakeAdapterProviders {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchProvider for FakeAdapterProviders {
    async fn search(&self, request: SearchRequest) -> Result<SearchResult> {
        self.record("search");
        if let Some(err) =
            self.mode_error(axon_error::ErrorStage::Discovering, ProviderKind::Search)
        {
            return Err(err);
        }
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
        Ok(self.provider_capability(ProviderKind::Search))
    }
}

#[async_trait]
impl FetchProvider for FakeAdapterProviders {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource> {
        self.record("fetch");
        if let Some(err) = self.mode_error(axon_error::ErrorStage::Fetching, ProviderKind::Fetch) {
            return Err(err);
        }
        Ok(FetchedResource {
            uri: request.uri.clone(),
            final_uri: request.uri,
            status: 200,
            content: ContentRef::InlineText {
                text: self.fetch_text.clone(),
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
        Ok(self.provider_capability(ProviderKind::Fetch))
    }
}

#[async_trait]
impl RenderProvider for FakeAdapterProviders {
    async fn render(&self, request: RenderRequest) -> Result<RenderedResource> {
        self.record("render");
        if let Some(err) = self.mode_error(axon_error::ErrorStage::Rendering, ProviderKind::Render)
        {
            return Err(err);
        }
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
        Ok(self.provider_capability(ProviderKind::Render))
    }
}

#[async_trait]
impl NetworkCaptureProvider for FakeAdapterProviders {
    async fn capture(&self, request: NetworkCaptureRequest) -> Result<NetworkCaptureResult> {
        self.record("capture");
        if let Some(err) = self.mode_error(
            axon_error::ErrorStage::Discovering,
            ProviderKind::NetworkCapture,
        ) {
            return Err(err);
        }
        Ok(NetworkCaptureResult {
            uri: request.uri,
            captured_at: timestamp(),
            entries: Vec::new(),
            artifacts: Vec::new(),
            metadata: request.metadata,
        })
    }

    async fn capabilities(&self) -> Result<ProviderCapability> {
        Ok(self.provider_capability(ProviderKind::NetworkCapture))
    }
}

fn provider_id(provider_kind: ProviderKind) -> String {
    format!("fake_{provider_kind:?}").to_lowercase()
}

fn timestamp() -> Timestamp {
    Timestamp("2026-07-01T00:00:00Z".to_string())
}

#[cfg(test)]
#[path = "boundary_tests.rs"]
mod tests;
