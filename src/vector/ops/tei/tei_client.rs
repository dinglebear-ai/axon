use crate::core::config::Config;
use crate::core::http::internal_service_http_client;
use crate::core::logging::{log_debug, log_warn};
use crate::vector::ops::qdrant::env_usize_clamped;
use rand::RngExt as _;
use reqwest::StatusCode;
use std::collections::VecDeque;
use std::error::Error;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Semaphore;

const TEI_MAX_BACKOFF_MS: u64 = 60_000;

/// Instruction prefix for Qwen3-Embedding asymmetric query encoding.
///
/// Prepend this to every query text before calling `tei_embed`.
/// Do NOT apply to document chunks — document embedding must use raw text.
///
/// This prefix activates query-mode encoding in Qwen3-Embedding models.
/// TEI's `--default-prompt` config flag has been removed; the prefix is
/// now applied in Rust so documents and queries get different embeddings.
pub(crate) const QUERY_INSTRUCTION: &str =
    "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery: ";

/// Prepend the Qwen3-Embedding query instruction to `query`.
///
/// Must be called before `tei_embed()` for query vectors only.
/// Document chunks must be embedded as raw text — do NOT call this for documents.
pub(crate) fn prepend_query_instruction(query: &str) -> String {
    format!("{QUERY_INSTRUCTION}{query}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EmbedKind {
    Query,
    Document,
}

#[derive(Debug, Clone)]
pub(crate) struct EmbedInput {
    pub(crate) kind: EmbedKind,
    pub(crate) text: String,
}

impl EmbedInput {
    pub(crate) fn query(text: impl Into<String>) -> Self {
        Self {
            kind: EmbedKind::Query,
            text: text.into(),
        }
    }

    pub(crate) fn document(text: impl Into<String>) -> Self {
        Self {
            kind: EmbedKind::Document,
            text: text.into(),
        }
    }
}

fn materialize_embed_input(input: &EmbedInput) -> String {
    match input.kind {
        EmbedKind::Query => prepend_query_instruction(&input.text),
        EmbedKind::Document => input.text.clone(),
    }
}

pub(crate) async fn tei_embed_typed(
    cfg: &Config,
    inputs: &[EmbedInput],
) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    let materialized = inputs
        .iter()
        .map(materialize_embed_input)
        .collect::<Vec<_>>();
    tei_embed_raw(cfg, &materialized).await
}

pub(crate) async fn tei_embed_kind(
    cfg: &Config,
    kind: EmbedKind,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    let typed = inputs
        .iter()
        .cloned()
        .map(|text| EmbedInput { kind, text })
        .collect::<Vec<_>>();
    tei_embed_typed(cfg, &typed).await
}

pub(crate) fn is_openai_compatible_embedding_url(cfg: &Config) -> bool {
    cfg.tei_url.trim_end_matches('/').ends_with("/v1")
}

/// Global process-wide limit on concurrent in-flight TEI /embed requests.
/// Prevents thundering-herd TCP saturation when multiple embed workers run in parallel.
/// Each permit covers one batch sent to TEI; the permit is held until the response returns.
/// Tunable via AXON_TEI_MAX_CONCURRENT (default 8, range 1–64).
static TEI_CONCURRENCY: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(env_usize_clamped("AXON_TEI_MAX_CONCURRENT", 8, 1, 64)));

/// Weighted process-wide limit on total input chunks currently submitted to TEI.
///
/// TEI's overload boundary is closer to `batch_size * request_concurrency` than
/// raw request count. This limiter lets small batches use higher request
/// concurrency while preventing large batches from stampeding past the server's
/// `max_batch_requests` budget.
static TEI_IN_FLIGHT_INPUT_LIMIT: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("AXON_TEI_MAX_IN_FLIGHT_INPUTS", 320, 1, 4096));
static TEI_IN_FLIGHT_INPUTS: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(*TEI_IN_FLIGHT_INPUT_LIMIT));

/// OpenAI-compatible embedding servers such as vLLM do better with smaller
/// client request batches and higher request fanout than TEI's native `/embed`
/// endpoint on the same workload.
static OPENAI_EMBED_CONCURRENCY: LazyLock<Semaphore> = LazyLock::new(|| {
    Semaphore::new(env_usize_clamped(
        "AXON_OPENAI_EMBED_MAX_CONCURRENT",
        32,
        1,
        64,
    ))
});
static OPENAI_EMBED_IN_FLIGHT_INPUT_LIMIT: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("AXON_OPENAI_EMBED_MAX_IN_FLIGHT_INPUTS", 512, 1, 4096));
static OPENAI_EMBED_IN_FLIGHT_INPUTS: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(*OPENAI_EMBED_IN_FLIGHT_INPUT_LIMIT));

fn tei_in_flight_input_permits(chunk_len: usize, limit: usize) -> u32 {
    chunk_len.clamp(1, limit) as u32
}

fn retry_delay(attempt: usize) -> Duration {
    // saturating_sub: attempt is always >= 1 at all call sites (loop from 1),
    // but guard against hypothetical attempt=0 to prevent u32 underflow. (Q-L5)
    let exponent = (attempt as u32).saturating_sub(1);
    let base_ms = 1000_u64.saturating_mul(2u64.saturating_pow(exponent));
    let capped_ms = base_ms.min(TEI_MAX_BACKOFF_MS);
    let jitter = Duration::from_millis(rand::rng().random_range(0..500));
    Duration::from_millis(capped_ms) + jitter
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

enum ChunkOutcome {
    Vectors(Vec<Vec<f32>>),
    /// Chunk was too large (HTTP 413); caller should split and retry.
    Split,
}

/// Wire shape for a single TEI /embed request.
///
/// Built once per chunk before the retry loop so the same borrowed reference is
/// re-serialised on each attempt rather than re-constructing a `serde_json::Value`
/// on every pass through the loop. (P-M3)
#[derive(serde::Serialize)]
struct EmbedReq<'a> {
    inputs: &'a [String],
}

#[derive(Debug, Clone)]
enum EmbedBackend {
    Tei { url: String },
    OpenAiCompat { url: String, model: String },
}

impl EmbedBackend {
    fn from_config(cfg: &Config) -> Self {
        let base = cfg.tei_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            let model = std::env::var("AXON_OPENAI_EMBEDDING_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| {
                    std::env::var("VLLM_SERVED_MODEL_NAME")
                        .ok()
                        .filter(|value| !value.trim().is_empty())
                })
                .unwrap_or_else(|| "axon-qwen3-embedding".to_string());
            return Self::OpenAiCompat {
                url: format!("{base}/embeddings"),
                model,
            };
        }
        Self::Tei {
            url: format!("{base}/embed"),
        }
    }

    fn url(&self) -> &str {
        match self {
            Self::Tei { url } | Self::OpenAiCompat { url, .. } => url,
        }
    }

    fn is_openai_compat(&self) -> bool {
        matches!(self, Self::OpenAiCompat { .. })
    }

    fn request_body<'a>(&'a self, inputs: &'a [String]) -> EmbedRequestBody<'a> {
        match self {
            Self::Tei { .. } => EmbedRequestBody::Tei(EmbedReq { inputs }),
            Self::OpenAiCompat { model, .. } => EmbedRequestBody::OpenAiCompat(OpenAiEmbedReq {
                model,
                input: inputs,
            }),
        }
    }

    async fn decode_vectors(
        &self,
        resp: reqwest::Response,
    ) -> Result<Vec<Vec<f32>>, reqwest::Error> {
        match self {
            Self::Tei { .. } => resp.json::<Vec<Vec<f32>>>().await,
            Self::OpenAiCompat { .. } => {
                let payload = resp.json::<OpenAiEmbedResp>().await?;
                Ok(payload
                    .data
                    .into_iter()
                    .map(|item| item.embedding)
                    .collect())
            }
        }
    }
}

fn embed_limiters(backend: &EmbedBackend) -> (&'static Semaphore, &'static Semaphore, usize) {
    if backend.is_openai_compat() {
        (
            &OPENAI_EMBED_CONCURRENCY,
            &OPENAI_EMBED_IN_FLIGHT_INPUTS,
            *OPENAI_EMBED_IN_FLIGHT_INPUT_LIMIT,
        )
    } else {
        (
            &TEI_CONCURRENCY,
            &TEI_IN_FLIGHT_INPUTS,
            *TEI_IN_FLIGHT_INPUT_LIMIT,
        )
    }
}

fn embed_split_concurrency(backend: &EmbedBackend) -> usize {
    if backend.is_openai_compat() {
        env_usize_clamped("AXON_OPENAI_EMBED_MAX_CONCURRENT", 32, 1, 64)
    } else {
        env_usize_clamped("AXON_TEI_MAX_CONCURRENT", 8, 1, 64)
    }
}

fn embed_client_batch_size(cfg: &Config, backend: &EmbedBackend) -> usize {
    if backend.is_openai_compat() {
        env_usize_clamped("AXON_OPENAI_EMBED_MAX_CLIENT_BATCH_SIZE", 32, 1, 256)
    } else {
        cfg.tei_max_client_batch_size.clamp(1, 256)
    }
}

#[derive(serde::Serialize)]
#[serde(untagged)]
enum EmbedRequestBody<'a> {
    Tei(EmbedReq<'a>),
    OpenAiCompat(OpenAiEmbedReq<'a>),
}

#[derive(serde::Serialize)]
struct OpenAiEmbedReq<'a> {
    model: &'a str,
    input: &'a [String],
}

#[derive(serde::Deserialize)]
struct OpenAiEmbedResp {
    data: Vec<OpenAiEmbedding>,
}

#[derive(serde::Deserialize)]
struct OpenAiEmbedding {
    embedding: Vec<f32>,
}

/// Logs a retry warning and sleeps for the backoff delay.
/// Returns `true` if the caller should `continue` to the next attempt, `false` if exhausted.
async fn log_retry_and_sleep(
    attempt: usize,
    max_attempts: usize,
    kind: &str,
    embed_url: &str,
    err_msg: &str,
) -> bool {
    if attempt >= max_attempts {
        return false;
    }
    let delay = retry_delay(attempt);
    let safe_url = redact_url_for_log(embed_url);
    log_warn(&format!(
        "tei_embed retry {kind} attempt={attempt}/{max_attempts} delay_ms={} url={safe_url} err={err_msg}",
        delay.as_millis()
    ));
    tokio::time::sleep(delay).await;
    true
}

async fn send_chunk_with_retries(
    client: &reqwest::Client,
    backend: &EmbedBackend,
    chunk: &[String],
    max_attempts: usize,
    request_timeout_ms: u64,
) -> Result<ChunkOutcome, Box<dyn Error>> {
    // Build the request body once before the retry loop — avoids reconstructing a
    // serde_json::Value on every attempt. Borrows `chunk` for the function lifetime. (P-M3)
    let body = backend.request_body(chunk);
    let embed_url = backend.url();
    for attempt in 1..=max_attempts {
        // Acquire the concurrency permit just before the HTTP request and drop
        // it immediately after the response completes.  Previously the permit
        // was held across retry backoff sleeps, meaning a transient 429/503
        // would hold a semaphore slot for up to 16s+ of backoff, exhausting
        // the global concurrency limit and stalling unrelated embed requests.
        let (request_limiter, input_limiter, input_limit) = embed_limiters(backend);
        let input_permits = tei_in_flight_input_permits(chunk.len(), input_limit);
        let input_permit = input_limiter
            .acquire_many(input_permits)
            .await
            .map_err(|e| -> Box<dyn Error> { format!("TEI input semaphore closed: {e}").into() })?;
        let permit = request_limiter
            .acquire()
            .await
            .map_err(|e| -> Box<dyn Error> { format!("TEI semaphore closed: {e}").into() })?;
        let resp = match client
            .post(embed_url)
            .timeout(Duration::from_millis(request_timeout_ms))
            .json(&body)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                // Release permit before sleeping on backoff.
                drop(permit);
                drop(input_permit);
                if log_retry_and_sleep(
                    attempt,
                    max_attempts,
                    "transport_error",
                    embed_url,
                    &err.to_string(),
                )
                .await
                {
                    continue;
                }
                let safe_url = redact_url_for_log(embed_url);
                return Err(format!(
                    "TEI request transport error for {safe_url} after {attempt}/{max_attempts} attempts: {err}"
                ).into());
            }
        };
        let status = resp.status();
        if status.is_success() {
            let result = backend.decode_vectors(resp).await;
            // Release permit after response body is consumed.
            drop(permit);
            drop(input_permit);
            match result {
                Ok(v) => return Ok(ChunkOutcome::Vectors(v)),
                Err(err) => {
                    if log_retry_and_sleep(
                        attempt,
                        max_attempts,
                        "decode_error",
                        embed_url,
                        &err.to_string(),
                    )
                    .await
                    {
                        continue;
                    }
                    let safe_url = redact_url_for_log(embed_url);
                    return Err(format!(
                        "TEI response decode error for {safe_url} after {attempt}/{max_attempts} attempts: {err}"
                    ).into());
                }
            }
        }
        // Non-success path: consume/drop the response body before releasing
        // the semaphore permit.  The permit gates TCP-level concurrency to TEI;
        // dropping it while the response body is still live can return the
        // connection to the pool mid-stream, corrupting the next request on
        // that socket.
        let err_body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<response body unavailable>".to_string());
        drop(permit);
        drop(input_permit);
        // 413 = payload too large; 422 with "batch size" body = TEI batch limit exceeded.
        // Both mean "chunk is too big for the server" — split and retry.
        let is_batch_too_large = (status == StatusCode::PAYLOAD_TOO_LARGE)
            || (status == StatusCode::UNPROCESSABLE_ENTITY && err_body.contains("batch size"));
        if is_batch_too_large && chunk.len() > 1 {
            return Ok(ChunkOutcome::Split);
        }
        if is_retryable_status(status) && attempt < max_attempts {
            let delay = retry_delay(attempt);
            let safe_url = redact_url_for_log(embed_url);
            log_warn(&format!(
                "tei_embed retry status attempt={attempt}/{max_attempts} delay_ms={} url={safe_url} status={status}",
                delay.as_millis()
            ));
            tokio::time::sleep(delay).await;
            continue;
        }
        let body = err_body;
        let body_preview: String = body.chars().take(240).collect();
        let safe_url = redact_url_for_log(embed_url);
        return Err(format!(
            "TEI request failed with status {status} for {safe_url} after {attempt}/{max_attempts} attempts; body={body_preview}"
        ).into());
    }
    Err(format!(
        "TEI embed exhausted {max_attempts} attempts for {}",
        redact_url_for_log(embed_url)
    )
    .into())
}

async fn tei_embed_raw(cfg: &Config, inputs: &[String]) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let client = internal_service_http_client()?;

    let backend = EmbedBackend::from_config(cfg);
    let batch_size = embed_client_batch_size(cfg, &backend);
    // tei_max_retries is the number of RETRY attempts after the initial request,
    // so total attempts = retries + 1. Default 5 retries → 6 attempts max.
    // .max(1) on the sum ensures at least one request even if a future config
    // shape allowed retries to underflow.
    let max_attempts = cfg.tei_max_retries.saturating_add(1).max(1);
    let request_timeout_ms = cfg.tei_request_timeout_ms.clamp(1000, 300_000);
    let safe_embed_url = redact_url_for_log(backend.url());

    log_debug(&format!(
        "tei_embed start chunk_count={} url={}",
        inputs.len(),
        safe_embed_url
    ));
    let tei_start = std::time::Instant::now();

    // Initial batches, in document order. Each entry carries its byte-range
    // [start, end) within `inputs` so its output vectors can be written back to
    // the correct positions regardless of completion order. (PERF-M3)
    let initial: Vec<(usize, &[String])> = {
        let mut acc = Vec::new();
        let mut start = 0usize;
        for chunk in inputs.chunks(batch_size) {
            acc.push((start, chunk));
            start += chunk.len();
        }
        acc
    };

    // Pre-size the output so out-of-order completions can index directly into it.
    // Each input maps to exactly one embedding vector at the same index.
    let mut slots: Vec<Vec<f32>> = vec![Vec::new(); inputs.len()];

    // Process batches with bounded concurrency. On a 413 split, the two halves
    // are re-queued and likewise drained concurrently — previously the split
    // drain was strictly serial (single VecDeque, one chunk at a time), so a
    // pathological 413 on a 64-item batch fanned out to up to 64 serial RTTs
    // that could blow the 300s doc timeout. Bounded concurrency keeps the same
    // embeddings and the same input→vector position mapping while overlapping
    // the re-sends. (PERF-M3)
    use futures_util::stream::{FuturesUnordered, StreamExt};

    // Concurrency for the split-drain fan-out. Bounded so a deep split cascade
    // cannot itself become a thundering herd; the backend-specific request
    // semaphore in send_chunk_with_retries remains the hard ceiling on
    // in-flight requests to the embedding server.
    let split_concurrency = embed_split_concurrency(&backend);

    // Bind `Copy` references once so each spawned future captures only cheap
    // copies (the `&'static` client, a `&str` view of the URL) under `async
    // move` — capturing `&embed_url` directly would try to move the owned
    // `String` into the first future.
    let backend_ref = &backend;

    // Work queue of (offset, sub-slice) pairs still to embed.
    let mut pending: VecDeque<(usize, &[String])> = initial.into_iter().collect();
    let mut in_flight = FuturesUnordered::new();

    loop {
        // Top up in-flight futures up to the concurrency bound.
        while in_flight.len() < split_concurrency
            && let Some((offset, chunk)) = pending.pop_front()
        {
            in_flight.push(async move {
                let outcome = send_chunk_with_retries(
                    client,
                    backend_ref,
                    chunk,
                    max_attempts,
                    request_timeout_ms,
                )
                .await;
                (offset, chunk, outcome)
            });
        }

        let Some((offset, chunk, outcome)) = in_flight.next().await else {
            // No in-flight work and nothing pending → done.
            if pending.is_empty() {
                break;
            }
            continue;
        };

        match outcome? {
            ChunkOutcome::Vectors(batch) => {
                // Position-stable write-back: the i-th vector belongs to the
                // input at `offset + i`. Preserves overall input ordering even
                // though batches may complete out of order.
                debug_assert_eq!(batch.len(), chunk.len());
                for (i, vec) in batch.into_iter().enumerate() {
                    slots[offset + i] = vec;
                }
            }
            ChunkOutcome::Split => {
                log_warn(&format!(
                    "tei_embed 413_split chunk_len={} splitting_at={}",
                    chunk.len(),
                    chunk.len() / 2
                ));
                let mid = chunk.len() / 2;
                let (left, right) = chunk.split_at(mid);
                // Re-queue both halves; offsets are preserved so write-back
                // still lands at the correct absolute input positions.
                pending.push_back((offset, left));
                pending.push_back((offset + mid, right));
            }
        }
    }

    let vectors = slots;

    log_debug(&format!(
        "tei_embed done vectors={} duration_ms={}",
        vectors.len(),
        tei_start.elapsed().as_millis()
    ));

    Ok(vectors)
}

fn redact_url_for_log(url: &str) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(url) else {
        return url.split('?').next().unwrap_or(url).to_string();
    };
    if !parsed.username().is_empty() {
        let _ = parsed.set_username("<redacted>");
    }
    if parsed.password().is_some() {
        let _ = parsed.set_password(Some("<redacted>"));
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

#[cfg(test)]
#[path = "tei_client_tests.rs"]
mod tests;
