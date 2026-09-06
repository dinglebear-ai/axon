//! VerticalContext — narrowed service context passed to every extractor.
//!
//! Verticals receive this instead of a full `&ServiceContext` so they can't
//! accidentally call unrelated services. The HTTP client is the shared
//! provider is constructed by the owning adapter/service and injected here, so
//! extractors cannot bypass transport policy or create clients per call.

use axon_core::http::{axon_api_ua, axon_ua};
use reqwest::Client;

/// Narrowed view over `ServiceContext` for vertical extractors.
///
/// Contains exactly what an extractor needs: a public user agent and the
/// automatic-dispatch skip list. It deliberately cannot expose credentials,
/// provider endpoints, or unrelated runtime configuration. Extractors issue
/// requests only through the injected adapter-owned client.
#[derive(Clone)]
pub struct VerticalContext {
    user_agent: Option<String>,
    auto_dispatch_skip: Vec<String>,
    credentials: VerticalCredentials,
    http_client: Client,
}

#[derive(Clone, Default)]
pub struct VerticalCredentials {
    pub github_token: Option<String>,
    pub huggingface_token: Option<String>,
    pub reddit_client_id: Option<String>,
    pub reddit_client_secret: Option<String>,
}

impl VerticalContext {
    pub fn new(
        user_agent: Option<String>,
        auto_dispatch_skip: Vec<String>,
        http_client: Client,
    ) -> Self {
        Self {
            user_agent,
            auto_dispatch_skip,
            credentials: VerticalCredentials::default(),
            http_client,
        }
    }

    /// Adapter-owned HTTP provider used by vertical implementations.
    pub fn http_client(&self) -> &Client {
        &self.http_client
    }

    pub fn with_credentials(mut self, credentials: VerticalCredentials) -> Self {
        self.credentials = credentials;
        self
    }

    pub fn github_token(&self) -> Option<&str> {
        self.credentials.github_token.as_deref()
    }

    pub fn huggingface_token(&self) -> Option<&str> {
        self.credentials.huggingface_token.as_deref()
    }

    pub fn reddit_credentials(&self) -> Option<(&str, &str)> {
        Some((
            self.credentials.reddit_client_id.as_deref()?,
            self.credentials.reddit_client_secret.as_deref()?,
        ))
    }

    /// Browser User-Agent for HTML scraping — clean Firefox UA, no bot tokens.
    /// Use for verticals that scrape public HTML pages (Amazon, eBay, YouTube).
    pub fn ua(&self) -> &str {
        self.user_agent.as_deref().unwrap_or_else(|| axon_ua())
    }

    /// Bot-identifying User-Agent for structured API calls.
    /// Use for verticals that call package registry or structured JSON APIs
    /// (crates.io, npm, PyPI, GitHub, Docker Hub, HuggingFace, dev.to, Shopify).
    /// These services are bot-friendly and use the UA for rate-limit attribution.
    pub fn api_ua(&self) -> &str {
        self.user_agent.as_deref().unwrap_or_else(|| axon_api_ua())
    }

    pub fn auto_dispatch_skipped(&self, extractor_name: &str) -> bool {
        self.auto_dispatch_skip
            .iter()
            .any(|name| name == extractor_name)
    }
}

#[cfg(test)]
#[path = "context_tests.rs"]
mod tests;
