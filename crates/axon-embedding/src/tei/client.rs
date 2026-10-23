//! Reqwest-backed TEI `/embed` HTTP client.
//!
//! Requests use TEI's `/embed` wire shape, recursively split batches after HTTP
//! 413, and retry HTTP 429/5xx responses with exponential backoff.
//!
//! Credentials never leak into [`ApiError`] messages — only the opaque marker
//! `"configured"` is attached to error context, mirroring the qdrant store's
//! redaction pattern.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use axon_api::source::ApiError;
use axon_error::ErrorStage;
use futures_util::{StreamExt, stream::FuturesUnordered};
use reqwest::header::RETRY_AFTER;
use reqwest::{Client, StatusCode};
use tokio::sync::Semaphore;

mod admission;
mod errors;
mod policy;
mod response;
mod tokenization;
mod types;
use policy::{
    credential_transport_is_safe, error_category, pack_batches_counted, parse_retry_after,
    resolve_batch_size,
};
#[cfg(test)]
use policy::{estimated_tokens, pack_batches};
pub use policy::{is_batch_too_large, is_retryable_status, retry_delay};
pub(crate) use tokenization::TokenizationState;
pub use types::{TeiEmbedOutcome, TeiInfo};
/// Opaque endpoint context marker attached to errors.
///
/// The raw URL and any embedded credentials are intentionally never surfaced.
pub const ENDPOINT_MARKER: &str = "configured";

/// Cap on exponential backoff before jitter.
const MAX_BACKOFF_MS: u64 = 60_000;
pub(crate) const MAX_BATCH_BYTES: usize = 8 * 1024 * 1024;

/// Cooling window attached to a retry-exhausted error, matching the default
/// `cooldown_secs` used by [`crate::reservation::ProviderReservations`].
const TEI_COOLDOWN_SECS: i64 = 30;

/// Absolute safety ceiling for the single typed batch-size authority.
const MAX_CLIENT_BATCH_SIZE: usize = 4096;

/// Process-wide reqwest client shared by every [`TeiClient`].
///
/// `TeiEmbeddingProvider::build_client()` constructs each transport with
/// [`TeiClient::new_with_gates`] so provider instances share admission state.
/// Building a fresh `reqwest::Client` per operation would also throw away its
/// connection pool and DNS resolver, so the HTTP client stays process-wide.
static SHARED_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    #[cfg(test)]
    CLIENT_BUILDS.fetch_add(1, Ordering::SeqCst);
    Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .expect("failed to build shared TEI reqwest client")
});

#[cfg(test)]
static CLIENT_BUILDS: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
pub(crate) fn shared_client_build_count() -> u64 {
    CLIENT_BUILDS.load(Ordering::SeqCst)
}

/// Tunables for a single `embed_all` invocation.
#[derive(Debug, Clone)]
pub struct TeiClientParams {
    pub endpoint: String,
    pub provider_id: String,
    /// Initial per-request chunk size (`config.max_batch_inputs`).
    pub max_batch_inputs: usize,
    pub max_input_tokens: usize,
    pub max_batch_tokens: usize,
    pub max_concurrent_requests: usize,
    pub max_in_flight_inputs: usize,
    /// Total attempts = configured retries + 1.
    pub max_attempts: usize,
    pub request_timeout: Duration,
    /// Base backoff (ms) before exponential growth + jitter, passed to
    /// [`retry_delay`]. Config: `[providers.embedding].retry-backoff-ms`.
    pub retry_backoff_base_ms: u64,
}

/// Wire shape for a lossless TEI `/embed` request body.
#[derive(serde::Serialize)]
struct EmbedRequest<'a> {
    inputs: &'a [&'a str],
    truncate: bool,
}

/// A single TEI request outcome after retries: either the decoded vectors or a
/// "chunk too large, split and retry" signal (HTTP 413).
enum ChunkOutcome {
    Vectors(Vec<Vec<f32>>),
    Split,
}

type IndexedBatch<'a> = (Vec<usize>, Vec<&'a str>);

#[derive(Debug, Clone, Copy)]
struct BatchLimits {
    max_inputs: usize,
    max_input_tokens: usize,
    max_batch_tokens: usize,
    max_batch_bytes: usize,
}
/// Reqwest-backed TEI embed transport carrying a redaction-safe embed URL.
#[derive(Debug)]
pub struct TeiClient {
    client: Client,
    embed_url: String,
    info_url: String,
    tokenize_url: String,
    tokenization: Arc<TokenizationState>,
    provider_id: String,
    bearer_token: Option<String>,
    max_batch_inputs: usize,
    max_input_tokens: usize,
    max_batch_tokens: usize,
    max_concurrent_requests: usize,
    request_slots: Arc<Semaphore>,
    input_slots: Arc<Semaphore>,
    profile_input_slots: Arc<Semaphore>,
    max_attempts: usize,
    request_timeout: Duration,
    retry_backoff_base_ms: u64,
    cumulative_requests: AtomicU64,
}

impl TeiClient {
    /// Build a transport for the configured TEI endpoint.
    ///
    /// The `/embed` path is appended to the configured base. The reqwest client
    /// carries no per-request timeout; each request applies `request_timeout`.
    #[cfg(test)]
    pub fn new(params: TeiClientParams) -> Result<Self, ApiError> {
        let request_slots = Arc::new(Semaphore::new(params.max_concurrent_requests.max(1)));
        let input_slots = Arc::new(Semaphore::new(params.max_in_flight_inputs.max(1)));
        Self::new_with_gates(params, request_slots, input_slots)
    }

    pub(crate) fn new_with_gates(
        params: TeiClientParams,
        request_slots: Arc<Semaphore>,
        input_slots: Arc<Semaphore>,
    ) -> Result<Self, ApiError> {
        let base = params.endpoint.trim().trim_end_matches('/');
        let parsed = url::Url::parse(base).map_err(|_| {
            ApiError::new(
                "embedding.tei.invalid_endpoint",
                ErrorStage::Authorizing,
                "TEI endpoint must be an absolute HTTP or HTTPS URL",
            )
            .with_context("endpoint", ENDPOINT_MARKER)
            .with_provider_id(&params.provider_id)
        })?;
        if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
            return Err(ApiError::new(
                "embedding.tei.invalid_endpoint",
                ErrorStage::Authorizing,
                "TEI endpoint must be an absolute HTTP or HTTPS URL",
            )
            .with_context("endpoint", ENDPOINT_MARKER)
            .with_provider_id(&params.provider_id));
        }
        let bearer_token = std::env::var("AXON_TEI_BEARER_TOKEN")
            .ok()
            .filter(|value| !value.is_empty());
        if !credential_transport_is_safe(&parsed, bearer_token.is_some()) {
            return Err(ApiError::new(
                "embedding.tei.insecure_credentials",
                ErrorStage::Authorizing,
                "TEI credentials require HTTPS for non-loopback endpoints",
            )
            .with_context("endpoint", ENDPOINT_MARKER)
            .with_provider_id(&params.provider_id));
        }
        let embed_url = format!("{base}/embed");
        let info_url = format!("{base}/info");
        let max_in_flight_inputs = params.max_in_flight_inputs.max(1);
        let max_batch_inputs =
            resolve_batch_size(params.max_batch_inputs).min(max_in_flight_inputs);
        Ok(Self {
            profile_input_slots: Arc::new(Semaphore::new(max_in_flight_inputs)),
            client: SHARED_CLIENT.clone(),
            embed_url,
            info_url,
            tokenize_url: format!("{base}/tokenize"),
            tokenization: Arc::new(TokenizationState::default()),
            provider_id: params.provider_id,
            bearer_token,
            max_batch_inputs,
            max_input_tokens: params.max_input_tokens.max(1),
            max_batch_tokens: params.max_batch_tokens.max(1),
            // Request concurrency and weighted input admission are independent
            // limits. Each packed request acquires its actual input weight in
            // `send_chunk_with_retries`; reducing request concurrency by the
            // configured *maximum* batch size strands capacity whenever real
            // packed requests are smaller.
            max_concurrent_requests: params.max_concurrent_requests.max(1),
            request_slots,
            input_slots,
            max_attempts: params.max_attempts.max(1),
            request_timeout: params.request_timeout,
            retry_backoff_base_ms: params.retry_backoff_base_ms,
            cumulative_requests: AtomicU64::new(0),
        })
    }

    pub(crate) fn with_tokenization(mut self, state: Arc<TokenizationState>) -> Self {
        self.tokenization = state;
        self
    }

    /// Fetch the TEI `/info` document (single attempt, no retries). Errors carry
    /// only the opaque endpoint marker — never the raw URL.
    pub async fn fetch_info(&self) -> Result<TeiInfo, ApiError> {
        let mut request = self.client.get(&self.info_url);
        if let Some(token) = &self.bearer_token {
            request = request.bearer_auth(token);
        }
        let resp = request
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(|err| self.transport(error_category(&err)))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(self.status_error(status));
        }
        response::read_json::<TeiInfo>(resp, response::INFO_BODY_LIMIT)
            .await
            .map_err(|err| self.response_error(err))
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
                input_tokens: Some(0),
                tokenization_fallback: None,
            });
        }

        // Pre-size output so out-of-order splits can index directly by position.
        let mut slots: Vec<Vec<f32>> = vec![Vec::new(); inputs.len()];

        // Pack similarly-sized inputs together before forming HTTP requests.
        // Transformer cost follows the longest padded sequence in a request,
        // so arrival-order slicing can waste most of a Metal dispatch on
        // padding. Keep original indices beside the reordered text so response
        // vectors still satisfy this method's input-order contract.
        let (counts, tokenization_fallback) = self.token_counts(inputs).await;
        let input_tokens = tokenization_fallback.is_none().then(|| {
            counts
                .iter()
                .fold(0_u64, |sum, count| sum.saturating_add(*count as u64))
        });
        let pending = pack_batches_counted(
            inputs,
            &counts,
            BatchLimits {
                max_inputs: self.max_batch_inputs,
                max_input_tokens: self.max_input_tokens,
                max_batch_tokens: self.max_batch_tokens,
                max_batch_bytes: MAX_BATCH_BYTES,
            },
        )
        .map_err(|message| self.error("embedding.tei.input_too_large", message))?;
        tokenization::record_packing(
            &pending,
            &counts,
            self.max_batch_tokens,
            tokenization_fallback.is_none(),
        );
        let mut pending = pending;

        let invocation_requests = Arc::new(AtomicU64::new(0));
        let mut in_flight = FuturesUnordered::new();
        while !pending.is_empty() || !in_flight.is_empty() {
            while in_flight.len() < self.max_concurrent_requests && !pending.is_empty() {
                let (indices, chunk) = pending.pop().expect("pending checked non-empty");
                let invocation_requests = Arc::clone(&invocation_requests);
                in_flight.push(async move {
                    let outcome = self
                        .send_chunk_with_retries(&chunk, invocation_requests.as_ref())
                        .await;
                    (indices, chunk, outcome)
                });
            }
            if let Some((indices, chunk, outcome)) = in_flight.next().await {
                match outcome? {
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
                        for (index, vector) in indices.iter().copied().zip(batch) {
                            slots[index] = vector;
                        }
                    }
                    ChunkOutcome::Split => {
                        let mid = chunk.len() / 2;
                        pending.push((indices[..mid].to_vec(), chunk[..mid].to_vec()));
                        pending.push((indices[mid..].to_vec(), chunk[mid..].to_vec()));
                    }
                }
            }
        }

        Ok(TeiEmbedOutcome {
            vectors: slots,
            requests: invocation_requests.load(Ordering::Relaxed),
            input_tokens,
            tokenization_fallback,
        })
    }

    /// Send one chunk, retrying transport errors and 429/5xx, and signalling a
    /// split on 413 for multi-input chunks.
    ///
    /// When every attempt is exhausted on a retryable condition (transport
    /// error or 429/5xx status), the returned [`ApiError`] carries
    /// [`axon_error::cooling::ProviderCooling`] metadata so the scheduler
    /// backs off this provider instead of hammering it again immediately —
    /// see "Cooling" in `docs/pipeline-unification/runtime/provider-contract.md`.
    async fn send_chunk_with_retries(
        &self,
        chunk: &[&str],
        invocation_requests: &AtomicU64,
    ) -> Result<ChunkOutcome, ApiError> {
        let body = EmbedRequest {
            inputs: chunk,
            truncate: false,
        };
        let started = Instant::now();
        let mut last: Option<ApiError> = None;
        // Transport errors are always retried until attempts are exhausted, so
        // reaching the final fallthrough below always means the last failure
        // was retryable; this only tracks the 429/5xx status branch, which can
        // also exit on a non-retryable status (e.g. 400) with no cooling.
        let mut last_retryable = true;

        for attempt in 1..=self.max_attempts {
            let (profile_input_permit, request_permit, input_permit) =
                self.acquire_admission(chunk.len()).await?;

            invocation_requests.fetch_add(1, Ordering::Relaxed);
            self.cumulative_requests.fetch_add(1, Ordering::Relaxed);
            let mut request = self.client.post(&self.embed_url);
            if let Some(token) = &self.bearer_token {
                request = request.bearer_auth(token);
            }
            let send =
                response::send_with_body(request.timeout(self.request_timeout).json(&body)).await;

            let resp = match send {
                Ok(response::EmbedResponse::Vectors(vectors)) => {
                    return Ok(ChunkOutcome::Vectors(vectors));
                }
                Ok(response::EmbedResponse::Status(resp)) => resp,
                Err(response::ResponseError::TooLarge) if chunk.len() > 1 => {
                    return Ok(ChunkOutcome::Split);
                }
                Err(
                    err @ (response::ResponseError::TooLarge | response::ResponseError::Decode),
                ) => {
                    return Err(self.response_error(err));
                }
                Err(response::ResponseError::Transport(err)) => {
                    drop(profile_input_permit);
                    drop(input_permit);
                    drop(request_permit);
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

            // 413 = payload too large; split multi-input chunks and retry halves.
            if is_batch_too_large(status) && chunk.len() > 1 {
                return Ok(ChunkOutcome::Split);
            }

            let retryable = is_retryable_status(status);
            let retry_after = resp.headers().get(RETRY_AFTER).and_then(parse_retry_after);
            if retryable
                && let Some(delay) = retry_after
                && (delay
                    > self
                        .request_timeout
                        .min(Duration::from_millis(MAX_BACKOFF_MS))
                    || attempt == self.max_attempts)
            {
                return Err(self.deferred_retry_after(status, delay));
            }
            last = Some(self.status_error(status));
            last_retryable = retryable;
            if retryable && attempt < self.max_attempts {
                drop(profile_input_permit);
                drop(input_permit);
                drop(request_permit);
                tokio::time::sleep(
                    retry_after.unwrap_or_else(|| {
                        retry_delay(attempt, started, self.retry_backoff_base_ms)
                    }),
                )
                .await;
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
}

#[cfg(test)]
#[path = "client_tests.rs"]
mod tests;
