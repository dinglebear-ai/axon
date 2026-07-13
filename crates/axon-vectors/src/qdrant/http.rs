//! Reqwest-backed Qdrant REST transport.
//!
//! Credentials never leak into [`ApiError`] details: [`QdrantEndpoint`] splits
//! the user-supplied URL into a bare `scheme://host[:port]` base and an
//! extracted API key (from userinfo or the `api_key` query parameter). Only the
//! opaque marker `"configured"` is ever attached to error context.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use axon_api::source::ApiError;
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Opaque endpoint context marker attached to errors.
///
/// The raw URL and any embedded credentials are intentionally never surfaced.
pub const ENDPOINT_MARKER: &str = "configured";

const MAX_ATTEMPTS: usize = 4;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Process-wide reqwest client shared by every [`QdrantHttp`] instance.
///
/// Each `QdrantHttp::new` used to allocate a fresh connection pool even though
/// upsert/search/delete create short-lived transport wrappers per operation.
/// Cloning the shared client keeps those operation wrappers cheap while reusing
/// keep-alive connections.
static SHARED_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    #[cfg(test)]
    CLIENT_BUILDS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("failed to build shared qdrant reqwest client")
});

#[cfg(test)]
static CLIENT_BUILDS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn shared_client_build_count() -> usize {
    CLIENT_BUILDS.load(std::sync::atomic::Ordering::SeqCst)
}

/// A Qdrant REST endpoint with credentials split away from the base URL.
#[derive(Debug, Clone)]
pub struct QdrantEndpoint {
    /// `scheme://host[:port]` with no path, query, or userinfo.
    base: String,
    /// Optional API key extracted from userinfo password or `api_key` query.
    api_key: Option<String>,
}

impl QdrantEndpoint {
    /// Parse the configured URL into a redaction-safe base + optional API key.
    ///
    /// Best-effort: if the URL cannot be parsed the trimmed input is used as the
    /// base and no key is extracted. Path and query segments are discarded so a
    /// value like `http://host:6333/collections?api_key=…` cannot leak.
    pub fn parse(url: &str) -> Self {
        let trimmed = url.trim();
        match url::Url::parse(trimmed) {
            Ok(parsed) => {
                let mut api_key = None;
                if !parsed.password().unwrap_or_default().is_empty() {
                    api_key = parsed.password().map(ToString::to_string);
                } else if !parsed.username().is_empty() {
                    // A bare `token@host` form carries the key as the username.
                    api_key = Some(parsed.username().to_string());
                }
                if api_key.is_none() {
                    api_key = parsed
                        .query_pairs()
                        .find(|(key, _)| key == "api_key")
                        .map(|(_, value)| value.into_owned());
                }
                let scheme = parsed.scheme();
                let base = match (parsed.host_str(), parsed.port()) {
                    (Some(host), Some(port)) => format!("{scheme}://{host}:{port}"),
                    (Some(host), None) => format!("{scheme}://{host}"),
                    _ => trimmed.to_string(),
                };
                Self { base, api_key }
            }
            Err(_) => Self {
                base: trimmed.trim_end_matches('/').to_string(),
                api_key: None,
            },
        }
    }

    /// Build a full request URL for a collection sub-path (e.g. `points/query`).
    pub fn collection_path(&self, collection: &str, suffix: &str) -> String {
        let suffix = suffix.trim_start_matches('/');
        if suffix.is_empty() {
            format!("{}/collections/{}", self.base, collection)
        } else {
            format!("{}/collections/{}/{}", self.base, collection, suffix)
        }
    }

    /// The bare `scheme://host[:port]` root, used for liveness probes.
    pub fn root(&self) -> &str {
        &self.base
    }

    fn api_key(&self) -> Option<&str> {
        self.api_key.as_deref()
    }
}

/// Reqwest client wrapper carrying a parsed, redaction-safe endpoint.
#[derive(Debug, Clone)]
pub struct QdrantHttp {
    client: Client,
    endpoint: QdrantEndpoint,
    provider_id: String,
}

impl QdrantHttp {
    /// Construct a transport for the configured Qdrant URL, attributing every
    /// surfaced error to `provider_id`.
    pub fn new(url: &str, provider_id: &str) -> Result<Self, ApiError> {
        Ok(Self {
            client: SHARED_CLIENT.clone(),
            endpoint: QdrantEndpoint::parse(url),
            provider_id: provider_id.to_string(),
        })
    }

    /// Endpoint accessor for URL construction.
    pub fn endpoint(&self) -> &QdrantEndpoint {
        &self.endpoint
    }

    /// GET a collection sub-resource, returning the parsed JSON on 2xx, `None`
    /// on 404, and an error otherwise. Never leaks the URL into error details.
    pub async fn get_json(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        context: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        let resp = self
            .request(Method::GET)
            .get(url)
            .send()
            .await
            .map_err(|err| self.transport(stage, context, &err))?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(self.status_error(stage, context, status));
        }
        let body = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|err| self.transport(stage, context, &err))?;
        Ok(Some(body))
    }

    /// DELETE a collection resource, tolerating 404 (already absent).
    pub async fn delete_json(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        context: &str,
    ) -> Result<(), ApiError> {
        let resp = self
            .request(Method::DELETE)
            .delete(url)
            .send()
            .await
            .map_err(|err| self.transport(stage, context, &err))?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND || status.is_success() {
            return Ok(());
        }
        Err(self.status_error(stage, context, status))
    }

    /// PUT a JSON body, tolerating 409 (collection already exists).
    pub async fn put_json<B: Serialize + ?Sized>(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        body: &B,
        context: &str,
    ) -> Result<(), ApiError> {
        let resp = self
            .request(Method::PUT)
            .put(url)
            .json(body)
            .send()
            .await
            .map_err(|err| self.transport(stage, context, &err))?;
        let status = resp.status();
        if status == StatusCode::CONFLICT || status.is_success() {
            return Ok(());
        }
        Err(self.status_error(stage, context, status))
    }

    /// POST a JSON body and parse the response, retrying on 429/5xx.
    pub async fn post_json<B, T>(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        body: &B,
        context: &str,
    ) -> Result<T, ApiError>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let started = Instant::now();
        let mut last: Option<ApiError> = None;
        for attempt in 1..=MAX_ATTEMPTS {
            match self.request(Method::POST).post(url).json(body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let retryable =
                        status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error();
                    if retryable && attempt < MAX_ATTEMPTS {
                        last = Some(self.status_error(stage, context, status));
                        tokio::time::sleep(retry_delay(attempt, started)).await;
                        continue;
                    }
                    if !status.is_success() {
                        return Err(self.status_error(stage, context, status));
                    }
                    return resp
                        .json::<T>()
                        .await
                        .map_err(|err| self.transport(stage, context, &err));
                }
                Err(err) => {
                    last = Some(self.transport(stage, context, &err));
                    if attempt < MAX_ATTEMPTS {
                        tokio::time::sleep(retry_delay(attempt, started)).await;
                    }
                }
            }
        }
        Err(last.unwrap_or_else(|| {
            transport_error(
                "vector.qdrant.transport",
                &format!("{context}: request failed"),
            )
            .with_context("endpoint", ENDPOINT_MARKER)
            .with_provider_id(&self.provider_id)
        }))
    }

    fn request(&self, _method: Method) -> AuthedBuilder<'_> {
        AuthedBuilder {
            client: &self.client,
            api_key: self.endpoint.api_key(),
        }
    }

    fn transport(
        &self,
        stage: axon_error::ErrorStage,
        context: &str,
        err: &reqwest::Error,
    ) -> ApiError {
        // Only the redaction-safe category is surfaced; reqwest's Display can
        // include the request URL, so it is never embedded in the message.
        ApiError::new(
            "vector.qdrant.transport",
            stage,
            format!(
                "{context}: qdrant transport error ({})",
                error_category(err)
            ),
        )
        .with_context("endpoint", ENDPOINT_MARKER)
        .with_provider_id(&self.provider_id)
    }

    fn status_error(
        &self,
        stage: axon_error::ErrorStage,
        context: &str,
        status: StatusCode,
    ) -> ApiError {
        ApiError::new(
            "vector.qdrant.status",
            stage,
            format!("{context}: qdrant returned status {}", status.as_u16()),
        )
        .with_context("endpoint", ENDPOINT_MARKER)
        .with_context("status", status.as_u16().to_string())
        .with_provider_id(&self.provider_id)
    }
}

/// Small builder that injects the `api-key` header when configured.
struct AuthedBuilder<'a> {
    client: &'a Client,
    api_key: Option<&'a str>,
}

impl<'a> AuthedBuilder<'a> {
    fn get(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.get(url))
    }

    fn put(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.put(url))
    }

    fn post(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.post(url))
    }

    fn delete(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.delete(url))
    }

    fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.api_key {
            Some(key) => builder.header("api-key", key),
            None => builder,
        }
    }
}

fn error_category(err: &reqwest::Error) -> &'static str {
    if err.is_timeout() {
        "timeout"
    } else if err.is_connect() {
        "connect"
    } else if err.is_decode() {
        "decode"
    } else {
        "request"
    }
}

fn transport_error(code: &str, message: &str) -> ApiError {
    ApiError::new(code, axon_error::ErrorStage::Observing, message.to_string())
        .with_context("endpoint", ENDPOINT_MARKER)
}

/// Exponential backoff with lightweight jitter derived from the elapsed clock.
fn retry_delay(attempt: usize, started: Instant) -> Duration {
    let base_ms = 250_u64.saturating_mul(1u64 << attempt.saturating_sub(1));
    let jitter_ms = (started.elapsed().subsec_nanos() as u64) % 100;
    Duration::from_millis(base_ms + jitter_ms)
}

#[cfg(test)]
#[path = "http_tests.rs"]
mod tests;
