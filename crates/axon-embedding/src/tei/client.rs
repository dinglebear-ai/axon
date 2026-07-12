//! Reqwest-backed TEI `/embed` HTTP client.
//!
//! Behaviour is ported from the legacy `axon-vector` TEI client
//! (`crates/axon-vector/src/ops/tei/tei_client.rs`): request/response wire shape,
//! 413 recursive batch-split, and 429/5xx exponential-backoff retries.
//!
//! Credentials never leak into [`ApiError`] messages — only the opaque marker
//! `"configured"` is attached to error context, mirroring the qdrant store's
//! redaction pattern.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axon_api::source::ApiError;
use axon_error::ErrorStage;
use axon_error::cooling::ProviderCooling;
use chrono::Utc;
use reqwest::{Client, StatusCode};

/// Opaque endpoint context marker attached to errors.
///
/// The raw URL and any embedded credentials are intentionally never surfaced.
pub const ENDPOINT_MARKER: &str = "configured";

/// Cap on exponential backoff before jitter, matching the legacy client.
const MAX_BACKOFF_MS: u64 = 60_000;

/// Cooling window attached to a retry-exhausted error, matching the default
/// `cooldown_secs` used by [`crate::reservation::ProviderReservations`].
const TEI_COOLDOWN_SECS: i64 = 30;

/// Absolute ceiling on the client-side batch size, matching the legacy client's
/// `tei_max_client_batch_size.clamp(1, 256)`.
const MAX_CLIENT_BATCH_SIZE: usize = 256;

/// Environment knob mirroring the legacy client's `TEI_MAX_CLIENT_BATCH_SIZE`.
const TEI_MAX_CLIENT_BATCH_SIZE_ENV: &str = "TEI_MAX_CLIENT_BATCH_SIZE";

/// Tunables for a single `embed_all` invocation.
#[derive(Debug, Clone)]
pub struct TeiClientParams {
    pub endpoint: String,
    pub provider_id: String,
    /// Initial per-request chunk size (`config.max_batch_inputs`).
    pub max_batch_inputs: usize,
    /// Total attempts = retries + 1; matches legacy `tei_max_retries + 1`.
    pub max_attempts: usize,
    pub request_timeout: Duration,
    /// Base backoff (ms) before exponential growth + jitter, passed to
    /// [`retry_delay`]. Config: `[providers.embedding].retry-backoff-ms`.
    pub retry_backoff_base_ms: u64,
}

/// Wire shape for a TEI `/embed` request body: `{"inputs": [...], "truncate": true}`.
#[derive(serde::Serialize)]
struct EmbedRequest<'a> {
    inputs: &'a [String],
    truncate: bool,
}

/// A single TEI request outcome after retries: either the decoded vectors or a
/// "chunk too large, split and retry" signal (HTTP 413).
enum ChunkOutcome {
    Vectors(Vec<Vec<f32>>),
    Split,
}

/// Result of an `embed_all` call: the ordered vectors plus how many HTTP
/// requests were actually issued (initial batches + retries + 413 splits).
#[derive(Debug)]
pub struct TeiEmbedOutcome {
    pub vectors: Vec<Vec<f32>>,
    pub requests: u64,
}

/// A subset of the TEI `/info` response. TEI serves `model_id` here, but NOT the
/// output dimensionality — dimensions are measured with a probe embed instead.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TeiInfo {
    #[serde(default)]
    pub model_id: Option<String>,
}

/// Reqwest-backed TEI embed transport carrying a redaction-safe embed URL.
#[derive(Debug)]
pub struct TeiClient {
    client: Client,
    embed_url: String,
    info_url: String,
    provider_id: String,
    max_batch_inputs: usize,
    max_attempts: usize,
    request_timeout: Duration,
    retry_backoff_base_ms: u64,
    requests: AtomicU64,
}

impl TeiClient {
    /// Build a transport for the configured TEI endpoint.
    ///
    /// The `/embed` path is appended to the configured base. The reqwest client
    /// carries no per-request timeout; each request applies `request_timeout`.
    pub fn new(params: TeiClientParams) -> Result<Self, ApiError> {
        let client = Client::builder().build().map_err(|err| {
            transport_error(
                "embedding.tei.client_init",
                "failed to build TEI HTTP client",
                error_category(&err),
            )
            .with_provider_id(&params.provider_id)
        })?;
        let base = params.endpoint.trim().trim_end_matches('/');
        let embed_url = format!("{base}/embed");
        let info_url = format!("{base}/info");
        let max_batch_inputs = resolve_batch_size(params.max_batch_inputs);
        Ok(Self {
            client,
            embed_url,
            info_url,
            provider_id: params.provider_id,
            max_batch_inputs,
            max_attempts: params.max_attempts.max(1),
            request_timeout: params.request_timeout,
            retry_backoff_base_ms: params.retry_backoff_base_ms,
            requests: AtomicU64::new(0),
        })
    }

    /// Fetch the TEI `/info` document (single attempt, no retries). Errors carry
    /// only the opaque endpoint marker — never the raw URL.
    pub async fn fetch_info(&self) -> Result<TeiInfo, ApiError> {
        let resp = self
            .client
            .get(&self.info_url)
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|err| self.transport(error_category(&err)))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(self.status_error(status));
        }
        resp.json::<TeiInfo>()
            .await
            .map_err(|err| self.transport(error_category(&err)))
    }

    /// Embed a single probe input and return its vector length. Used to derive
    /// the provider's true output dimensionality, which `/info` does not expose.
    pub async fn probe_dimensions(&self, probe: &str) -> Result<u32, ApiError> {
        let outcome = self
            .embed_all(std::slice::from_ref(&probe.to_string()))
            .await?;
        let dims = outcome
            .vectors
            .first()
            .map(|vector| vector.len() as u32)
            .filter(|dims| *dims > 0)
            .ok_or_else(|| {
                self.error(
                    "embedding.tei.probe_empty",
                    "TEI probe embed returned no vector",
                )
            })?;
        Ok(dims)
    }

    /// Embed every input, preserving order. Returns one vector per input at the
    /// same index. Splits initial batches on HTTP 413 and retries 429/5xx.
    pub async fn embed_all(&self, inputs: &[String]) -> Result<TeiEmbedOutcome, ApiError> {
        if inputs.is_empty() {
            return Ok(TeiEmbedOutcome {
                vectors: Vec::new(),
                requests: 0,
            });
        }

        // Pre-size output so out-of-order splits can index directly by position.
        let mut slots: Vec<Vec<f32>> = vec![Vec::new(); inputs.len()];

        // Work queue of (absolute-offset, sub-slice) pairs still to embed.
        let mut pending: Vec<(usize, &[String])> = Vec::new();
        let mut start = 0usize;
        for chunk in inputs.chunks(self.max_batch_inputs) {
            pending.push((start, chunk));
            start += chunk.len();
        }

        while let Some((offset, chunk)) = pending.pop() {
            match self.send_chunk_with_retries(chunk).await? {
                ChunkOutcome::Vectors(batch) => {
                    if batch.len() != chunk.len() {
                        return Err(self.error(
                            "embedding.tei.count_mismatch",
                            &format!(
                                "TEI returned {} vectors for a {}-input batch",
                                batch.len(),
                                chunk.len()
                            ),
                        ));
                    }
                    for (i, vec) in batch.into_iter().enumerate() {
                        slots[offset + i] = vec;
                    }
                }
                ChunkOutcome::Split => {
                    let mid = chunk.len() / 2;
                    let (left, right) = chunk.split_at(mid);
                    pending.push((offset, left));
                    pending.push((offset + mid, right));
                }
            }
        }

        Ok(TeiEmbedOutcome {
            vectors: slots,
            requests: self.requests.load(Ordering::Relaxed),
        })
    }

    /// Send one chunk, retrying transport errors and 429/5xx, and signalling a
    /// split on 413 for multi-input chunks.
    ///
    /// When every attempt is exhausted on a retryable condition (transport
    /// error or 429/5xx status), the returned [`ApiError`] carries
    /// [`ProviderCooling`] metadata (`with_provider_cooling`) so the scheduler
    /// backs off this provider instead of hammering it again immediately —
    /// see "Cooling" in `docs/pipeline-unification/runtime/provider-contract.md`.
    async fn send_chunk_with_retries(&self, chunk: &[String]) -> Result<ChunkOutcome, ApiError> {
        let body = EmbedRequest {
            inputs: chunk,
            truncate: true,
        };
        let started = Instant::now();
        let mut last: Option<ApiError> = None;
        // Transport errors are always retried until attempts are exhausted, so
        // reaching the final fallthrough below always means the last failure
        // was retryable; this only tracks the 429/5xx status branch, which can
        // also exit on a non-retryable status (e.g. 400) with no cooling.
        let mut last_retryable = true;

        for attempt in 1..=self.max_attempts {
            self.requests.fetch_add(1, Ordering::Relaxed);
            let send = self
                .client
                .post(&self.embed_url)
                .timeout(self.request_timeout)
                .json(&body)
                .send()
                .await;

            let resp = match send {
                Ok(resp) => resp,
                Err(err) => {
                    last = Some(self.transport(error_category(&err)));
                    last_retryable = true;
                    if attempt < self.max_attempts {
                        tokio::time::sleep(retry_delay(
                            attempt,
                            started,
                            self.retry_backoff_base_ms,
                        ))
                        .await;
                    }
                    continue;
                }
            };

            let status = resp.status();
            if status.is_success() {
                return match resp.json::<Vec<Vec<f32>>>().await {
                    Ok(vectors) => Ok(ChunkOutcome::Vectors(vectors)),
                    Err(err) => Err(self.transport(error_category(&err))),
                };
            }

            // 413 = payload too large; split multi-input chunks and retry halves.
            if is_batch_too_large(status) && chunk.len() > 1 {
                return Ok(ChunkOutcome::Split);
            }

            let retryable = is_retryable_status(status);
            last = Some(self.status_error(status));
            last_retryable = retryable;
            if retryable && attempt < self.max_attempts {
                tokio::time::sleep(retry_delay(attempt, started, self.retry_backoff_base_ms)).await;
                continue;
            }
            let err = last.unwrap();
            return Err(if retryable {
                self.with_exhausted_cooling(err)
            } else {
                err
            });
        }

        let err = last.unwrap_or_else(|| {
            self.error(
                "embedding.tei.exhausted",
                "TEI embed exhausted all attempts",
            )
        });
        Err(if last_retryable {
            self.with_exhausted_cooling(err)
        } else {
            err
        })
    }

    /// Attach a bounded [`ProviderCooling`] window to a retry-exhausted error
    /// so callers holding a scheduler reservation back off before their next
    /// attempt. The window is fixed (not exponential) — it only needs to
    /// outlast one scheduling tick, not model the retry backoff itself.
    fn with_exhausted_cooling(&self, err: ApiError) -> ApiError {
        err.with_provider_cooling(
            ProviderCooling::new(Utc::now() + chrono::Duration::seconds(TEI_COOLDOWN_SECS))
                .with_provider(self.provider_id.as_str())
                .with_reason("tei_retry_exhausted"),
        )
    }

    fn error(&self, code: &str, message: &str) -> ApiError {
        ApiError::new(code, ErrorStage::Embedding, message.to_string())
            .with_context("endpoint", ENDPOINT_MARKER)
            .with_provider_id(&self.provider_id)
    }

    fn transport(&self, category: &str) -> ApiError {
        // reqwest's Display can carry the request URL, so it is never embedded.
        ApiError::new(
            "embedding.tei.transport",
            ErrorStage::Embedding,
            format!("TEI transport error ({category})"),
        )
        .with_context("endpoint", ENDPOINT_MARKER)
        .with_provider_id(&self.provider_id)
    }

    fn status_error(&self, status: StatusCode) -> ApiError {
        ApiError::new(
            "embedding.tei.status",
            ErrorStage::Embedding,
            format!("TEI returned status {}", status.as_u16()),
        )
        .with_context("endpoint", ENDPOINT_MARKER)
        .with_context("status", status.as_u16().to_string())
        .with_provider_id(&self.provider_id)
    }
}

/// Resolve the initial client-side batch size, honouring the
/// `TEI_MAX_CLIENT_BATCH_SIZE` env knob (matching the legacy client), then
/// clamping to `[1, 256]`.
fn resolve_batch_size(config_batch: usize) -> usize {
    let base = std::env::var(TEI_MAX_CLIENT_BATCH_SIZE_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(config_batch);
    base.clamp(1, MAX_CLIENT_BATCH_SIZE)
}

/// 429 and any 5xx are retryable; everything else (including 413) is not.
pub fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// HTTP 413 signals the batch is too large for the server and must be split.
pub fn is_batch_too_large(status: StatusCode) -> bool {
    status == StatusCode::PAYLOAD_TOO_LARGE
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

fn transport_error(code: &str, message: &str, category: &str) -> ApiError {
    ApiError::new(
        code,
        ErrorStage::Embedding,
        format!("{message} ({category})"),
    )
    .with_context("endpoint", ENDPOINT_MARKER)
}

/// Exponential backoff (`base_ms`, `2*base_ms`, `4*base_ms`, …, capped at 60s)
/// with lightweight jitter derived from the elapsed clock — no `rand`
/// dependency, mirroring the qdrant store's `retry_delay`. `base_ms` is
/// caller-configured (`[providers.embedding].retry-backoff-ms`, default
/// 500ms) rather than a hardcoded literal.
pub fn retry_delay(attempt: usize, started: Instant, base_ms: u64) -> Duration {
    let exponent = (attempt as u32).saturating_sub(1);
    let scaled_ms = base_ms.saturating_mul(2u64.saturating_pow(exponent));
    let capped_ms = scaled_ms.min(MAX_BACKOFF_MS);
    let jitter_ms = (started.elapsed().subsec_nanos() as u64) % 500;
    Duration::from_millis(capped_ms + jitter_ms)
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
