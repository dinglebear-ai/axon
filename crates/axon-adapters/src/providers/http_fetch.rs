//! `HttpFetchProvider` — a real, reqwest-backed [`FetchProvider`].
//!
//! Design choice (Wave 1a of issue #298): this fetches the *raw* response body
//! (text or, for non-UTF-8 payloads, base64 bytes) rather than routing through
//! the in-crate `web_engine`'s Spider/markdown pipeline. [`FetchedResource`]
//! carries a raw [`ContentRef`], not markdown — the DTO's shape is "relay
//! whatever the origin sent", the same job a `curl`/reqwest transport does.
//! Rendering HTML into markdown is
//! [`crate::providers::chrome_render::ChromeRenderProvider`]'s job. Building
//! on plain reqwest (already an axon-core transitive dependency, not a new
//! one) is the smallest surface that produces a correct [`FetchedResource`] —
//! pulling in `web_engine`'s Spider `Website` machinery here would mean
//! constructing a full `Config` and a single-page crawl just to issue one
//! GET.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::*;
use axon_core::http::{axon_ua, build_ssrf_guarded_client_builder, validate_url};
use axon_error::ErrorStage;
use axon_observe::reservation::{ProviderReservationConfig, ProviderReservationManager};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use chrono::Utc;
use reqwest::Method;

use crate::boundary::{FetchProvider, Result};

const PROVIDER_ID: &str = "http_fetch";

/// Header names whose value is replaced with a fixed marker in the
/// [`RedactedHeaders`] attached to a [`FetchedResource`]. Case-insensitive.
const SENSITIVE_RESPONSE_HEADERS: &[&str] = &[
    "authorization",
    "set-cookie",
    "cookie",
    "proxy-authorization",
    "www-authenticate",
];

/// Total redirect hops the provider will follow before giving up — matches
/// `web_engine::scrape`'s `build_scrape_fallback_client` limit.
const MAX_REDIRECTS: usize = 10;

/// Self-tracked health/cooldown capacity — sized generously, purely to fold
/// live outcomes into `capabilities()`, not to gate concurrency.
const HEALTH_TRACKER_CAPACITY: u32 = 1_000_000;

/// A single retryable failure (e.g. one timeout) reports `Degraded`; a second
/// consecutive one escalates to `Cooling`. A rate-limited (429) response is
/// treated as an immediately-severe signal and recorded as two strikes (see
/// [`HttpFetchProvider::record_rate_limited`]) so it reaches `Cooling` with a
/// `cooldown_until` on the very first occurrence, rather than requiring two
/// consecutive 429s.
const HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES: u32 = 2;
const HEALTH_TRACKER_COOLDOWN_SECS: u64 = 30;

#[derive(Debug, Clone)]
pub struct HttpFetchConfig {
    pub timeout: Duration,
    pub max_bytes: Option<u64>,
    pub user_agent: Option<String>,
}

impl Default for HttpFetchConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_bytes: None,
            user_agent: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HttpFetchProvider {
    config: HttpFetchConfig,
    health: ProviderReservationManager,
}

impl HttpFetchProvider {
    pub fn new(config: HttpFetchConfig) -> Self {
        let health = ProviderReservationManager::new(ProviderReservationConfig {
            provider_id: ProviderId::new(PROVIDER_ID),
            provider_kind: ProviderKind::Fetch,
            capacity: HEALTH_TRACKER_CAPACITY,
            interactive_reserve: 0,
            cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
            cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
        });
        Self { config, health }
    }

    pub fn config(&self) -> &HttpFetchConfig {
        &self.config
    }

    fn error(&self, code: &str, message: impl Into<String>) -> ApiError {
        ApiError::new(code, ErrorStage::Fetching, message.into()).with_provider_id(PROVIDER_ID)
    }

    /// Build a fresh SSRF-guarded client for one request, wiring a redirect
    /// policy that re-validates every hop (closing the TOCTOU window a plain
    /// `reqwest::redirect::Policy::default()` would leave open) and records
    /// each followed hop into `redirect_chain`.
    fn build_client(
        &self,
        redirect_chain: Arc<Mutex<Vec<String>>>,
    ) -> std::result::Result<reqwest::Client, ApiError> {
        let ua = self
            .config
            .user_agent
            .clone()
            .unwrap_or_else(|| axon_ua().to_string());
        let mut builder = build_ssrf_guarded_client_builder(Some(self.config.timeout));
        builder = builder.user_agent(ua);
        builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
            let url_string = attempt.url().as_str().to_owned();
            if let Err(err) = validate_url(&url_string) {
                return attempt.error(err.to_string());
            }
            if attempt.previous().len() >= MAX_REDIRECTS {
                return attempt.error("too many redirects");
            }
            redirect_chain
                .lock()
                .expect("redirect chain mutex poisoned")
                .push(url_string);
            attempt.follow()
        }));
        builder
            .build()
            .map_err(|err| self.error("fetch.client_init", err.to_string()))
    }

    fn method(&self, raw: &str) -> std::result::Result<Method, ApiError> {
        if raw.trim().is_empty() {
            return Ok(Method::GET);
        }
        Method::from_bytes(raw.as_bytes())
            .map_err(|_| self.error("fetch.invalid_method", format!("invalid HTTP method {raw}")))
    }

    /// Encode a request-side [`ContentRef`] into raw bytes. Only inline
    /// payloads are supported in Wave 1a — `Artifact`/`External` bodies need a
    /// store/fetch round-trip this provider does not yet perform.
    fn encode_body(&self, body: &ContentRef) -> std::result::Result<Vec<u8>, ApiError> {
        match body {
            ContentRef::InlineText { text } => Ok(text.clone().into_bytes()),
            ContentRef::InlineBytes { bytes_base64, .. } => BASE64_STANDARD
                .decode(bytes_base64)
                .map_err(|err| self.error("fetch.invalid_body", err.to_string())),
            ContentRef::Artifact { .. } | ContentRef::External { .. } => Err(self.error(
                "fetch.body_source_unsupported",
                "artifact/external request bodies are not yet supported",
            )),
        }
    }

    fn redact_headers(&self, headers: &reqwest::header::HeaderMap) -> RedactedHeaders {
        let redacted = headers
            .iter()
            .map(|(name, value)| {
                let name = name.as_str().to_string();
                let sensitive = SENSITIVE_RESPONSE_HEADERS
                    .iter()
                    .any(|candidate| candidate.eq_ignore_ascii_case(&name));
                let value = if sensitive {
                    "[redacted]".to_string()
                } else {
                    value.to_str().unwrap_or("[non-utf8]").to_string()
                };
                RedactedHeader {
                    name,
                    value,
                    redacted: sensitive,
                }
            })
            .collect();
        RedactedHeaders { headers: redacted }
    }

    /// Decode a response body into a [`ContentRef`]: valid UTF-8 stays
    /// `InlineText`; anything else is base64-encoded `InlineBytes`.
    fn decode_body(&self, bytes: &[u8], content_type: Option<&str>) -> ContentRef {
        match std::str::from_utf8(bytes) {
            Ok(text) => ContentRef::InlineText {
                text: text.to_string(),
            },
            Err(_) => ContentRef::InlineBytes {
                bytes_base64: BASE64_STANDARD.encode(bytes),
                mime_type: content_type
                    .unwrap_or("application/octet-stream")
                    .to_string(),
            },
        }
    }

    async fn record_timeout(&self) {
        self.health.record_failure("provider.timeout", true).await;
    }

    /// 429 is a maximal-severity signal: recorded as two strikes so the
    /// cooldown-after-2 threshold trips on the very first rate-limited
    /// response instead of requiring a second consecutive one. See the
    /// `HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES` doc comment.
    async fn record_rate_limited(&self) {
        for _ in 0..HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES {
            self.health
                .record_failure("provider.rate_limited", true)
                .await;
        }
    }

    async fn record_fatal(&self) {
        self.health.record_failure("provider.fatal", false).await;
    }

    /// Finalizes a non-error (not 429/5xx) HTTP response into a
    /// [`FetchedResource`]: extracts headers/etag, reads and size-caps the
    /// body, then records success. Split out of `fetch()` to keep that
    /// function under the monolith function-length warning.
    async fn finish_success(
        &self,
        request: FetchRequest,
        response: reqwest::Response,
        status: reqwest::StatusCode,
        redirect_chain: Arc<Mutex<Vec<String>>>,
    ) -> Result<FetchedResource> {
        let final_uri = response.url().to_string();
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let headers = self.redact_headers(response.headers());

        let bytes = response
            .bytes()
            .await
            .map_err(|err| self.error("fetch.body_read", err.to_string()))?;
        let effective_max_bytes = request.max_bytes.or(self.config.max_bytes);
        if effective_max_bytes.is_some_and(|max_bytes| bytes.len() as u64 > max_bytes) {
            let max_bytes = effective_max_bytes.expect("checked by is_some_and above");
            return Err(self.error(
                "fetch.response_too_large",
                format!(
                    "response body {} bytes exceeds max_bytes {max_bytes}",
                    bytes.len()
                ),
            ));
        }

        self.health.record_success().await;
        Ok(FetchedResource {
            uri: request.uri,
            final_uri,
            status: status.as_u16(),
            content: self.decode_body(&bytes, content_type.as_deref()),
            headers,
            fetched_at: Timestamp::from(Utc::now()),
            etag,
            redirect_chain: redirect_chain
                .lock()
                .expect("redirect chain mutex poisoned")
                .clone(),
            bytes: Some(bytes.len() as u64),
            metadata: request.metadata,
        })
    }
}

#[async_trait]
impl FetchProvider for HttpFetchProvider {
    async fn fetch(&self, request: FetchRequest) -> Result<FetchedResource> {
        validate_url(&request.uri)
            .map_err(|err| self.error("fetch.invalid_uri", err.to_string()))?;

        let redirect_chain = Arc::new(Mutex::new(Vec::new()));
        let client = self.build_client(Arc::clone(&redirect_chain))?;
        let method = self.method(&request.method)?;

        let mut builder = client.request(method, &request.uri);
        for header in &request.headers.headers {
            builder = builder.header(&header.name, &header.value);
        }
        builder = builder.timeout(
            request
                .timeout_ms
                .map(Duration::from_millis)
                .unwrap_or(self.config.timeout),
        );
        if let Some(body) = &request.body {
            builder = builder.body(self.encode_body(body)?);
        }

        let send_result = builder.send().await;
        let response = match send_result {
            Ok(response) => response,
            Err(err) if err.is_timeout() => {
                self.record_timeout().await;
                return Err(self.error("fetch.timeout", "request timed out"));
            }
            Err(err) => {
                self.record_fatal().await;
                return Err(self.error("fetch.transport", err.to_string()));
            }
        };

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            self.record_rate_limited().await;
            return Err(self.error("fetch.rate_limited", "provider returned HTTP 429"));
        }
        if status.is_server_error() {
            self.record_fatal().await;
            return Err(self.error(
                "fetch.server_error",
                format!("provider returned HTTP {}", status.as_u16()),
            ));
        }

        self.finish_success(request, response, status, redirect_chain)
            .await
    }

    /// Reports the provider's **live** health/cooldown, folded in from every
    /// [`fetch`](Self::fetch) call's outcome — mirrors
    /// `axon-embedding`'s `TeiEmbeddingProvider::capabilities`.
    async fn capabilities(&self) -> Result<ProviderCapability> {
        let health = self.health.health().await;
        let cooldown_until = self.health.cooldown_until().await;
        let last_error = self
            .health
            .cooling_snapshot()
            .await
            .map(|cooling| self.error("provider.cooling", cooling.reason));
        Ok(ProviderCapability {
            provider_id: ProviderId::new(PROVIDER_ID),
            provider_kind: ProviderKind::Fetch,
            implementation: "reqwest".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            health,
            limits: ProviderLimits {
                timeout_ms: Some(self.config.timeout.as_millis() as u64),
                max_input_bytes: self.config.max_bytes,
                ..ProviderLimits::default()
            },
            features: vec!["http".to_string(), "https".to_string()],
            cooldown_until,
            last_error,
            reservation_policy: ReservationPolicy {
                supports_reservations: true,
                queue_policy: QueuePolicy::Fifo,
                interactive_reserve: 0,
                cooldown_after_failures: HEALTH_TRACKER_COOLDOWN_AFTER_FAILURES,
                cooldown_secs: HEALTH_TRACKER_COOLDOWN_SECS,
                retry_backoff_ms: None,
            },
            reservation_state: super::single_slot_reservation_state(health),
            cost_class: ProviderCostClass::Internal,
            degraded_modes: Vec::new(),
            fake_overrides_supported: false,
            embedding: None,
            llm: None,
            vector_store: None,
            fetch: Some(FetchProviderCapability {
                schemes: vec!["http".to_string(), "https".to_string()],
                redirect_policy: RedirectPolicy::Any,
                header_policy: HeaderPolicy::RedactedPassthrough,
            }),
            render: None,
            credential: None,
        })
    }
}

#[cfg(test)]
#[path = "http_fetch_tests.rs"]
mod tests;
