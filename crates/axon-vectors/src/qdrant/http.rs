//! Reqwest-backed Qdrant REST transport.
//!
//! Credentials never leak into [`ApiError`] details: [`QdrantEndpoint`] splits
//! the user-supplied URL into a bare `scheme://host[:port]` base and an
//! extracted API key (from userinfo or the `api_key` query parameter). Only the
//! opaque marker `"configured"` is ever attached to error context.

use std::sync::LazyLock;
use std::time::{Duration, Instant};

use axon_api::source::ApiError;
use reqwest::header::{HeaderValue, RETRY_AFTER};
use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

mod endpoint;
pub(crate) use endpoint::QdrantEndpoint;

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

/// Reqwest client wrapper carrying a parsed, redaction-safe endpoint.
#[derive(Debug, Clone)]
pub struct QdrantHttp {
    client: Client,
    endpoint: QdrantEndpoint,
    api_key_header: Option<HeaderValue>,
    provider_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PutCreateOutcome {
    Created,
    AlreadyExists,
}

impl QdrantHttp {
    /// Construct a transport for the configured Qdrant URL, attributing every
    /// surfaced error to `provider_id`.
    pub fn new(url: &str, provider_id: &str) -> Result<Self, ApiError> {
        let endpoint = QdrantEndpoint::parse(url);
        if !endpoint.valid {
            return Err(ApiError::new(
                "vector.qdrant.invalid_endpoint",
                axon_error::ErrorStage::Authorizing,
                "Qdrant endpoint must be an absolute HTTP or HTTPS URL",
            )
            .with_context("endpoint", ENDPOINT_MARKER)
            .with_provider_id(provider_id));
        }
        if !endpoint.credentials_use_safe_transport() {
            return Err(ApiError::new(
                "vector.qdrant.insecure_credentials",
                axon_error::ErrorStage::Authorizing,
                "Qdrant credentials require HTTPS for non-loopback endpoints",
            )
            .with_context("endpoint", ENDPOINT_MARKER)
            .with_provider_id(provider_id));
        }
        let api_key_header = endpoint
            .api_key()
            .map(HeaderValue::from_str)
            .transpose()
            .map_err(|_| {
                ApiError::new(
                    "vector.qdrant.invalid_credentials",
                    axon_error::ErrorStage::Authorizing,
                    "Qdrant API key is not a valid HTTP header value",
                )
                .with_context("endpoint", ENDPOINT_MARKER)
                .with_provider_id(provider_id)
            })?
            .map(|mut value| {
                value.set_sensitive(true);
                value
            });
        Ok(Self {
            client: SHARED_CLIENT.clone(),
            endpoint,
            api_key_header,
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

    /// PUT a JSON body. Conflict is an error for data mutations; callers that
    /// create idempotent resources must opt into conflict acceptance.
    pub async fn put_json<B: Serialize + ?Sized>(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        body: &B,
        context: &str,
    ) -> Result<(), ApiError> {
        let request = self.request(Method::PUT).put(url).json(body);
        self.send_put(request, stage, context).await
    }

    pub async fn patch_json<B: Serialize + ?Sized>(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        body: &B,
        context: &str,
    ) -> Result<(), ApiError> {
        let resp = self
            .request(Method::PATCH)
            .patch(url)
            .json(body)
            .send()
            .await
            .map_err(|err| self.transport(stage, context, &err))?;
        if resp.status().is_success() {
            return Ok(());
        }
        Err(self.status_error(stage, context, resp.status()))
    }

    pub async fn delete(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        context: &str,
    ) -> Result<bool, ApiError> {
        let response = self
            .request(Method::DELETE)
            .delete(url)
            .send()
            .await
            .map_err(|error| self.transport(stage, context, &error))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Ok(false);
        }
        if response.status().is_success() {
            return Ok(true);
        }
        Err(self.status_error(stage, context, response.status()))
    }

    /// PUT an idempotently-created resource, accepting a conflict that means
    /// another caller already created the same collection or payload index.
    pub async fn put_json_idempotent_create<B: Serialize + ?Sized>(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        body: &B,
        context: &str,
    ) -> Result<PutCreateOutcome, ApiError> {
        let request = self.request(Method::PUT).put(url).json(body);
        match self.send_put_status(request, stage, context).await? {
            status if status.is_success() => Ok(PutCreateOutcome::Created),
            StatusCode::CONFLICT => Ok(PutCreateOutcome::AlreadyExists),
            status => Err(self.status_error(stage, context, status)),
        }
    }

    /// PUT an already encoded JSON body. This is used by the
    /// vector hot path after enforcing the exact encoded-byte ceiling, avoiding
    /// a second serialization pass inside reqwest.
    pub async fn put_json_bytes(
        &self,
        stage: axon_error::ErrorStage,
        url: &str,
        body: Vec<u8>,
        context: &str,
    ) -> Result<(), ApiError> {
        let request = self
            .request(Method::PUT)
            .put(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
        self.send_put(request, stage, context).await
    }

    async fn send_put(
        &self,
        request: reqwest::RequestBuilder,
        stage: axon_error::ErrorStage,
        context: &str,
    ) -> Result<(), ApiError> {
        let status = self.send_put_status(request, stage, context).await?;
        if status.is_success() {
            return Ok(());
        }
        Err(self.status_error(stage, context, status))
    }

    async fn send_put_status(
        &self,
        request: reqwest::RequestBuilder,
        stage: axon_error::ErrorStage,
        context: &str,
    ) -> Result<StatusCode, ApiError> {
        let started = Instant::now();
        for attempt in 1..=MAX_ATTEMPTS {
            let mut server_delay = None;
            let replay = request.try_clone().ok_or_else(|| {
                ApiError::new(
                    "vector.qdrant.non_replayable_request",
                    stage,
                    "Qdrant JSON request cannot be replayed",
                )
            })?;
            match replay.send().await {
                Ok(response) => {
                    let status = response.status();
                    server_delay = response
                        .headers()
                        .get(RETRY_AFTER)
                        .and_then(parse_retry_after);
                    if attempt == MAX_ATTEMPTS
                        || !(status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                    {
                        return Ok(status);
                    }
                }
                Err(error) if attempt == MAX_ATTEMPTS => {
                    return Err(self.transport(stage, context, &error));
                }
                Err(_) => {}
            }
            tokio::time::sleep(server_delay.unwrap_or_else(|| retry_delay(attempt, started))).await;
        }
        unreachable!("the final attempt always returns")
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
                        let server_delay =
                            resp.headers().get(RETRY_AFTER).and_then(parse_retry_after);
                        last = Some(self.status_error(stage, context, status));
                        tokio::time::sleep(
                            server_delay.unwrap_or_else(|| retry_delay(attempt, started)),
                        )
                        .await;
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
            api_key_header: self.api_key_header.as_ref(),
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

fn parse_retry_after(value: &HeaderValue) -> Option<Duration> {
    value
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Small builder that injects the `api-key` header when configured.
struct AuthedBuilder<'a> {
    client: &'a Client,
    api_key_header: Option<&'a HeaderValue>,
}

impl<'a> AuthedBuilder<'a> {
    fn get(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.get(url))
    }

    fn delete(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.delete(url))
    }

    fn put(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.put(url))
    }

    fn post(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.post(url))
    }

    fn patch(self, url: &str) -> reqwest::RequestBuilder {
        self.apply(self.client.patch(url))
    }

    fn apply(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.api_key_header {
            Some(value) => builder.header("api-key", value.clone()),
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
