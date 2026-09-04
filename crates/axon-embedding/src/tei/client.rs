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
use axon_error::cooling::ProviderCooling;
use chrono::Utc;
use futures_util::{StreamExt, stream::FuturesUnordered};
use reqwest::{Client, StatusCode};
use tokio::sync::Semaphore;

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

/// Absolute safety ceiling matching the instrumented server's default row
/// limit. The effective value remains controlled by
/// `TEI_MAX_CLIENT_BATCH_SIZE`; raising the ceiling avoids silently defeating
/// a generation pool that has already been bounded by chunks and bytes.
const MAX_CLIENT_BATCH_SIZE: usize = 4096;

/// Environment override for the initial client-side batch size.
const TEI_MAX_CLIENT_BATCH_SIZE_ENV: &str = "TEI_MAX_CLIENT_BATCH_SIZE";

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
    inputs: &'a [String],
    truncate: bool,
}

/// A single TEI request outcome after retries: either the decoded vectors or a
/// "chunk too large, split and retry" signal (HTTP 413).
enum ChunkOutcome {
    Vectors(Vec<Vec<f32>>),
    Split,
}

type IndexedBatch = (Vec<usize>, Vec<String>);

#[derive(Debug, Clone, Copy)]
struct BatchLimits {
    max_inputs: usize,
    max_input_tokens: usize,
    max_batch_tokens: usize,
    max_batch_bytes: usize,
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
    bearer_token: Option<String>,
    max_batch_inputs: usize,
    max_input_tokens: usize,
    max_batch_tokens: usize,
    max_concurrent_requests: usize,
    request_slots: Arc<Semaphore>,
    input_slots: Arc<Semaphore>,
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
        let embed_url = format!("{base}/embed");
        let info_url = format!("{base}/info");
        let max_in_flight_inputs = params.max_in_flight_inputs.max(1);
        let max_batch_inputs =
            resolve_batch_size(params.max_batch_inputs).min(max_in_flight_inputs);
        Ok(Self {
            client: SHARED_CLIENT.clone(),
            embed_url,
            info_url,
            provider_id: params.provider_id,
            bearer_token: std::env::var("AXON_TEI_BEARER_TOKEN")
                .ok()
                .filter(|value| !value.is_empty()),
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

        // Pack similarly-sized inputs together before forming HTTP requests.
        // Transformer cost follows the longest padded sequence in a request,
        // so arrival-order slicing can waste most of a Metal dispatch on
        // padding. Keep original indices beside the reordered text so response
        // vectors still satisfy this method's input-order contract.
        let pending = pack_batches(
            inputs,
            BatchLimits {
                max_inputs: self.max_batch_inputs,
                max_input_tokens: self.max_input_tokens,
                max_batch_tokens: self.max_batch_tokens,
                max_batch_bytes: MAX_BATCH_BYTES,
            },
        )
        .map_err(|message| self.error("embedding.tei.input_too_large", message))?;
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
    async fn send_chunk_with_retries(
        &self,
        chunk: &[String],
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
            let request_permit = Arc::clone(&self.request_slots)
                .acquire_owned()
                .await
                .map_err(|_| {
                    self.error(
                        "embedding.tei.admission_closed",
                        "TEI request admission gate is closed",
                    )
                })?;
            let input_count = u32::try_from(chunk.len()).map_err(|_| {
                self.error(
                    "embedding.tei.input_budget_invalid",
                    "TEI client batch exceeds the weighted admission range",
                )
            })?;
            let input_permit = Arc::clone(&self.input_slots)
                .acquire_many_owned(input_count)
                .await
                .map_err(|_| {
                    self.error(
                        "embedding.tei.admission_closed",
                        "TEI input admission gate is closed",
                    )
                })?;

            invocation_requests.fetch_add(1, Ordering::Relaxed);
            self.cumulative_requests.fetch_add(1, Ordering::Relaxed);
            let mut request = self.client.post(&self.embed_url);
            if let Some(token) = &self.bearer_token {
                request = request.bearer_auth(token);
            }
            let send = request
                .timeout(self.request_timeout)
                .json(&body)
                .send()
                .await;

            let resp = match send {
                Ok(resp) => resp,
                Err(err) => {
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
                drop(input_permit);
                drop(request_permit);
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

/// Estimate tokens without coupling the transport to a model-specific tokenizer.
///
/// Common English and source code average more than two ASCII bytes per token,
/// so charging one token per two bytes is deliberately conservative without the
/// severe under-batching of charging every character as a token. Non-ASCII text
/// is charged by UTF-8 byte count: this safely covers CJK, emoji, and tokenizer
/// byte-fallback pieces. TEI's tokenizer remains authoritative; HTTP 413 still
/// drives recursive splitting when an estimate is too optimistic.
fn estimated_tokens(text: &str) -> usize {
    let (ascii, non_ascii_bytes) = text.chars().fold((0_usize, 0_usize), |counts, ch| {
        if ch.is_ascii() {
            (counts.0 + 1, counts.1)
        } else {
            (counts.0, counts.1 + ch.len_utf8())
        }
    });
    ascii.div_ceil(2).saturating_add(non_ascii_bytes).max(1)
}

/// Form canonical request batches before any admission permits are acquired.
/// Inputs estimated above the per-input model limit are isolated as singletons,
/// allowing TEI to make the final tokenization decision without contaminating a
/// following normal batch.
fn pack_batches(inputs: &[String], limits: BatchLimits) -> Result<Vec<IndexedBatch>, &'static str> {
    let mut ordered = inputs.iter().enumerate().collect::<Vec<_>>();
    ordered.sort_by_key(|(index, text)| (estimated_tokens(text), text.chars().count(), *index));

    let mut batches = Vec::new();
    let mut indices = Vec::new();
    let mut texts = Vec::new();
    let mut batch_tokens = 0_usize;
    let mut batch_bytes = 0_usize;

    for (index, text) in ordered {
        let tokens = estimated_tokens(text);
        let bytes = text.len().saturating_add(4);
        if bytes > limits.max_batch_bytes {
            return Err("one embedding input exceeds the configured payload limit");
        }

        if tokens > limits.max_input_tokens {
            push_batch(&mut batches, &mut indices, &mut texts);
            batches.push((vec![index], vec![text.clone()]));
            batch_tokens = 0;
            batch_bytes = 0;
            continue;
        }

        if !texts.is_empty()
            && (texts.len() >= limits.max_inputs
                || batch_tokens.saturating_add(tokens) > limits.max_batch_tokens
                || batch_bytes.saturating_add(bytes) > limits.max_batch_bytes)
        {
            push_batch(&mut batches, &mut indices, &mut texts);
            batch_tokens = 0;
            batch_bytes = 0;
        }
        indices.push(index);
        texts.push(text.clone());
        batch_tokens = batch_tokens.saturating_add(tokens);
        batch_bytes = batch_bytes.saturating_add(bytes);
    }
    push_batch(&mut batches, &mut indices, &mut texts);
    Ok(batches)
}

fn push_batch(batches: &mut Vec<IndexedBatch>, indices: &mut Vec<usize>, texts: &mut Vec<String>) {
    if !texts.is_empty() {
        batches.push((std::mem::take(indices), std::mem::take(texts)));
    }
}

/// Resolve the initial client-side batch size, honouring the
/// `TEI_MAX_CLIENT_BATCH_SIZE` environment override, then clamp to `[1, 4096]`.
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
