//! Depot JSON-API ArtifactCandidate sink.
//!
//! Depot owns candidate intake and publication authority. This sink only
//! projects Axon's already-redacted neutral v1 evidence into Depot's canonical
//! JSON operation endpoint. Depot accepts one candidate per operation call, so
//! the sink advertises max_batch_size=1 and the unified source pipeline keeps
//! delivery sequential and bounded.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axon_api::source::{
    ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION, ARTIFACT_CANDIDATE_MAX_BYTES, ApiError,
    ArtifactCandidate, ArtifactCandidateBatch, ArtifactCandidateSinkCapability,
    ArtifactCandidateSinkResult, ArtifactCandidateSinkStatus, ErrorStage, Severity, SourceWarning,
};
use axon_error::{RetryPolicy, RetryScope};
use futures_util::StreamExt;
use serde::Deserialize;
use url::Url;

use super::{ArtifactCandidateSink, Result};

const OPERATION_PATH: &[&str] = &["api", "operations", "depot.artifacts.intake_candidate"];
const USER_AGENT: &str =
    "axon-artifact-candidate-sink/1.0 (+https://github.com/dinglebear-ai/axon)";
const REQUEST_TIMEOUT_SECS: u64 = 20;
const MAX_RESPONSE_BYTES: usize = ARTIFACT_CANDIDATE_MAX_BYTES + 65_536;
const MAX_ERROR_BODY_BYTES: usize = 16_384;
const MAX_RETRY_AFTER_SECS: u64 = 300;
pub(super) const DEPOT_MAX_IN_FLIGHT: usize = 1;

#[derive(Clone)]
pub struct DepotArtifactCandidateSink {
    client: reqwest::Client,
    endpoint: Url,
    bearer_token: Arc<str>,
    pub(super) in_flight: Arc<tokio::sync::Semaphore>,
}

impl DepotArtifactCandidateSink {
    /// Build a Depot sink from a trusted Depot service base URL and a
    /// write-scoped bearer token. Auth stays transport-only and never enters
    /// the ArtifactCandidate payload.
    pub fn new(base_url: &str, bearer_token: impl Into<String>) -> Result<Self> {
        let endpoint = operation_endpoint(base_url)?;
        let bearer_token = bearer_token.into();
        if bearer_token.trim().is_empty() {
            return Err(ApiError::new(
                "adapter.artifact_candidate.depot.token_missing",
                ErrorStage::Enriching,
                "Depot ArtifactCandidate sink requires a non-empty write-scoped bearer token",
            ));
        }
        if bearer_token.trim() != bearer_token {
            return Err(ApiError::new(
                "adapter.artifact_candidate.depot.token_invalid",
                ErrorStage::Enriching,
                "Depot ArtifactCandidate sink bearer token must not contain surrounding whitespace",
            ));
        }
        let bearer_token: Arc<str> = Arc::from(bearer_token);
        // Depot is operator-configured infrastructure, not a discovered crawl URL.
        // Connect directly so private/Tailscale addresses remain valid, disable
        // redirects so the bearer cannot move to another origin, and ignore
        // ambient HTTP proxies so credentials only reach the configured Depot.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .pool_max_idle_per_host(8)
            .pool_idle_timeout(Some(Duration::from_secs(60)))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| {
                ApiError::new(
                    "adapter.artifact_candidate.depot.client_init_failed",
                    ErrorStage::Enriching,
                    error.to_string(),
                )
            })?;
        Ok(Self {
            client,
            endpoint,
            bearer_token,
            in_flight: Arc::new(tokio::sync::Semaphore::new(DEPOT_MAX_IN_FLIGHT)),
        })
    }

    async fn submit_candidate(
        &self,
        candidate: &ArtifactCandidate,
    ) -> Result<ArtifactCandidateSinkResult> {
        let _permit = self.in_flight.acquire().await.map_err(|_| {
            retryable_error(
                "adapter.artifact_candidate.depot.delivery_gate_closed",
                "Depot candidate sink delivery gate is closed".to_string(),
            )
        })?;
        let response = self
            .client
            .post(self.endpoint.clone())
            .bearer_auth(self.bearer_token.as_ref())
            .json(&serde_json::json!({"candidate": candidate}))
            .send()
            .await
            .map_err(|error| {
                retryable_error(
                    "adapter.artifact_candidate.depot.request_failed",
                    format!("Depot candidate intake request failed: {error}"),
                )
            })?;

        classify_response(candidate, response).await
    }
}

async fn classify_response(
    candidate: &ArtifactCandidate,
    response: reqwest::Response,
) -> Result<ArtifactCandidateSinkResult> {
    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = retry_after_seconds(&response);
        let mut error = retryable_error(
            "adapter.artifact_candidate.depot.rate_limited",
            "Depot candidate intake returned HTTP 429; retry the idempotent candidate later"
                .to_string(),
        );
        if let Some(seconds) = retry_after {
            error = error
                .with_retry_after_ms(seconds.saturating_mul(1_000))
                .with_context("retry_after_seconds", seconds.to_string());
        }
        return Err(error);
    }
    if status.is_server_error() {
        return Err(retryable_error(
            "adapter.artifact_candidate.depot.unavailable",
            format!("Depot candidate intake returned HTTP {}", status.as_u16()),
        ));
    }
    if status.is_client_error() {
        let detail = bounded_error_detail(response).await;
        let (code, message) = match status {
            reqwest::StatusCode::UNAUTHORIZED => (
                "source.artifact_candidate.depot.unauthorized",
                "Depot rejected the candidate sink bearer token".to_string(),
            ),
            reqwest::StatusCode::FORBIDDEN => (
                "source.artifact_candidate.depot.insufficient_scope",
                "Depot candidate intake requires write scope".to_string(),
            ),
            _ => (
                "source.artifact_candidate.depot.rejected",
                detail.unwrap_or_else(|| {
                    format!("Depot candidate intake returned HTTP {}", status.as_u16())
                }),
            ),
        };
        return Ok(rejected_result(code, message));
    }
    if !status.is_success() {
        return Ok(rejected_result(
            "source.artifact_candidate.depot.protocol_status",
            format!(
                "Depot candidate intake returned unexpected HTTP {}",
                status.as_u16()
            ),
        ));
    }

    let bytes = read_bounded_body(response, MAX_RESPONSE_BYTES).await?;
    let body: DepotOperationResponse = serde_json::from_slice(&bytes).map_err(|error| {
        retryable_error(
            "adapter.artifact_candidate.depot.response_invalid",
            format!("Depot candidate intake returned invalid JSON: {error}"),
        )
    })?;
    if body.result.candidate != *candidate {
        return Ok(rejected_result(
            "source.artifact_candidate.depot.echo_mismatch",
            "Depot candidate intake response did not echo the submitted canonical v1 candidate"
                .to_string(),
        ));
    }
    Ok(ArtifactCandidateSinkResult {
        status: ArtifactCandidateSinkStatus::Accepted,
        attempted: 1,
        accepted: 1,
        rejected: 0,
        warnings: Vec::new(),
    })
}

#[async_trait]
impl ArtifactCandidateSink for DepotArtifactCandidateSink {
    async fn submit(&self, batch: ArtifactCandidateBatch) -> Result<ArtifactCandidateSinkResult> {
        let attempted = batch.candidates.len() as u64;
        if batch.contract_version != ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION {
            return Ok(rejected_batch(
                attempted,
                "source.artifact_candidate.depot.batch_contract_rejected",
                format!(
                    "Depot sink only accepts Axon batch contract version {}",
                    ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION
                ),
            ));
        }
        if batch.candidates.len() != 1 {
            return Ok(rejected_batch(
                attempted,
                "source.artifact_candidate.depot.batch_size_rejected",
                "Depot intake accepts exactly one ArtifactCandidate per canonical operation call"
                    .to_string(),
            ));
        }
        let candidate = &batch.candidates[0];
        if let Err(error) = candidate.validate_shared_contract() {
            return Ok(rejected_result(
                "source.artifact_candidate.depot.shared_contract_rejected",
                format!(
                    "candidate {} failed the shared v1 contract: {error}",
                    candidate.id.0
                ),
            ));
        }
        self.submit_candidate(candidate).await
    }

    async fn capabilities(&self) -> Result<ArtifactCandidateSinkCapability> {
        Ok(ArtifactCandidateSinkCapability {
            name: "depot-http".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            contract_versions: vec![ARTIFACT_CANDIDATE_BATCH_CONTRACT_VERSION.to_string()],
            max_batch_size: 1,
            supports_idempotency: true,
        })
    }
}

#[derive(Deserialize)]
struct DepotOperationResponse {
    result: DepotOperationResult,
}
#[derive(Deserialize)]
struct DepotOperationResult {
    candidate: ArtifactCandidate,
}

fn operation_endpoint(base_url: &str) -> Result<Url> {
    let mut url = Url::parse(base_url).map_err(|error| {
        ApiError::new(
            "adapter.artifact_candidate.depot.url_invalid",
            ErrorStage::Enriching,
            format!("invalid Depot base URL: {error}"),
        )
    })?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.cannot_be_a_base()
    {
        return Err(ApiError::new(
            "adapter.artifact_candidate.depot.url_invalid",
            ErrorStage::Enriching,
            "Depot base URL must be an HTTP(S) base URL without userinfo, query, or fragment",
        ));
    }
    {
        let mut segments = url.path_segments_mut().map_err(|_| {
            ApiError::new(
                "adapter.artifact_candidate.depot.url_invalid",
                ErrorStage::Enriching,
                "Depot base URL cannot accept operation path segments",
            )
        })?;
        segments.pop_if_empty();
        for segment in OPERATION_PATH {
            segments.push(segment);
        }
    }
    Ok(url)
}

fn retry_after_seconds(response: &reqwest::Response) -> Option<u64> {
    response
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(|seconds| seconds.min(MAX_RETRY_AFTER_SECS))
}

async fn read_bounded_body(response: reqwest::Response, max_bytes: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > max_bytes as u64)
    {
        return Err(retryable_error(
            "adapter.artifact_candidate.depot.response_too_large",
            format!("Depot candidate intake response exceeds {max_bytes} byte cap"),
        ));
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| {
            retryable_error(
                "adapter.artifact_candidate.depot.response_read_failed",
                format!("failed reading Depot candidate intake response: {error}"),
            )
        })?;
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(retryable_error(
                "adapter.artifact_candidate.depot.response_too_large",
                format!("Depot candidate intake response exceeds {max_bytes} byte cap"),
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn bounded_error_detail(response: reqwest::Response) -> Option<String> {
    let bytes = read_bounded_error_body(response).await?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let message = value.get("error")?.as_str()?.trim();
    if message.is_empty() {
        return None;
    }
    Some(message.chars().take(1_024).collect())
}

async fn read_bounded_error_body(response: reqwest::Response) -> Option<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > MAX_ERROR_BODY_BYTES as u64)
    {
        return None;
    }
    let mut bytes = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.ok()?;
        if bytes.len().saturating_add(chunk.len()) > MAX_ERROR_BODY_BYTES {
            return None;
        }
        bytes.extend_from_slice(&chunk);
    }
    Some(bytes)
}

fn rejected_result(code: &str, message: String) -> ArtifactCandidateSinkResult {
    rejected_batch(1, code, message)
}
fn rejected_batch(attempted: u64, code: &str, message: String) -> ArtifactCandidateSinkResult {
    ArtifactCandidateSinkResult {
        status: ArtifactCandidateSinkStatus::Rejected,
        attempted,
        accepted: 0,
        rejected: attempted,
        warnings: vec![sink_warning(code, message, false)],
    }
}
fn sink_warning(code: &str, message: String, retryable: bool) -> SourceWarning {
    SourceWarning {
        code: code.to_string(),
        severity: Severity::Warning,
        message,
        source_item_key: None,
        retryable,
    }
}
fn retryable_error(code: &str, message: String) -> ApiError {
    ApiError::new(code, ErrorStage::Enriching, message)
        .with_retry_policy(RetryPolicy::retryable(RetryScope::Provider))
        .with_provider_id("depot")
}
