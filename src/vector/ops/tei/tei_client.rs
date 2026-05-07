use crate::core::config::Config;
use crate::core::http::http_client;
use crate::core::logging::{log_debug, log_warn};
use crate::vector::ops::qdrant::env_usize_clamped;
use rand::RngExt as _;
use reqwest::StatusCode;
use std::error::Error;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Semaphore;

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

const TEI_MAX_RETRIES_DEFAULT: usize = 5;
const TEI_REQUEST_TIMEOUT_MS_DEFAULT: u64 = 30_000;
const TEI_REQUEST_TIMEOUT_MS_MIN: u64 = 100;
const TEI_REQUEST_TIMEOUT_MS_MAX: u64 = 600_000;
const TEI_MAX_BACKOFF_MS: u64 = 60_000;

/// Global process-wide limit on concurrent in-flight TEI /embed requests.
/// Prevents thundering-herd TCP saturation when multiple embed workers run in parallel.
/// Each permit covers one batch sent to TEI; the permit is held until the response returns.
/// Tunable via AXON_TEI_MAX_CONCURRENT (default 8, range 1–64).
static TEI_CONCURRENCY: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(env_usize_clamped("AXON_TEI_MAX_CONCURRENT", 8, 1, 64)));

// ── Cached env vars for hot-path embed operations ──────────────────────────
// These are read once at process startup via LazyLock instead of calling
// std::env::var() (which acquires a global lock) on every batch invocation.

static TEI_BATCH_SIZE: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("TEI_MAX_CLIENT_BATCH_SIZE", 64, 1, 128));

static TEI_MAX_ATTEMPTS: LazyLock<usize> =
    LazyLock::new(|| env_usize_clamped("TEI_MAX_RETRIES", TEI_MAX_RETRIES_DEFAULT, 1, 20));

static TEI_TIMEOUT_MS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("TEI_REQUEST_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(|v| v.clamp(TEI_REQUEST_TIMEOUT_MS_MIN, TEI_REQUEST_TIMEOUT_MS_MAX))
        .unwrap_or(TEI_REQUEST_TIMEOUT_MS_DEFAULT)
});

fn retry_delay(attempt: usize) -> Duration {
    let base_ms = 1000_u64.saturating_mul(2u64.saturating_pow(attempt as u32 - 1));
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
    embed_url: &str,
    chunk: &[String],
    max_attempts: usize,
    request_timeout_ms: u64,
) -> Result<ChunkOutcome, Box<dyn Error>> {
    for attempt in 1..=max_attempts {
        // Acquire the concurrency permit just before the HTTP request and drop
        // it immediately after the response completes.  Previously the permit
        // was held across retry backoff sleeps, meaning a transient 429/503
        // would hold a semaphore slot for up to 16s+ of backoff, exhausting
        // the global concurrency limit and stalling unrelated embed requests.
        let permit = TEI_CONCURRENCY
            .acquire()
            .await
            .map_err(|e| -> Box<dyn Error> { format!("TEI semaphore closed: {e}").into() })?;
        let resp = match client
            .post(embed_url)
            .timeout(Duration::from_millis(request_timeout_ms))
            .json(&serde_json::json!({"inputs": chunk}))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                // Release permit before sleeping on backoff.
                drop(permit);
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
            let result = resp.json::<Vec<Vec<f32>>>().await;
            // Release permit after response body is consumed.
            drop(permit);
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
    let client = http_client()?;
    let mut vectors = Vec::new();

    let batch_size = *TEI_BATCH_SIZE;
    let embed_url = format!("{}/embed", cfg.tei_url.trim_end_matches('/'));
    let max_attempts = *TEI_MAX_ATTEMPTS;
    let request_timeout_ms = *TEI_TIMEOUT_MS;
    let safe_embed_url = redact_url_for_log(&embed_url);

    log_debug(&format!(
        "tei_embed start chunk_count={} url={}",
        inputs.len(),
        safe_embed_url
    ));
    let tei_start = std::time::Instant::now();

    let mut stack: Vec<&[String]> = inputs.chunks(batch_size).collect();
    stack.reverse();

    while let Some(chunk) = stack.pop() {
        match send_chunk_with_retries(client, &embed_url, chunk, max_attempts, request_timeout_ms)
            .await?
        {
            ChunkOutcome::Vectors(mut batch) => vectors.append(&mut batch),
            ChunkOutcome::Split => {
                log_warn(&format!(
                    "tei_embed 413_split chunk_len={} splitting_at={}",
                    chunk.len(),
                    chunk.len() / 2
                ));
                let mid = chunk.len() / 2;
                let (left, right) = chunk.split_at(mid);
                stack.push(right);
                stack.push(left);
            }
        }
    }

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
mod tests {
    use super::redact_url_for_log;

    #[test]
    fn redact_url_for_log_removes_credentials_query_and_fragment() {
        let redacted =
            redact_url_for_log("http://user:secret@tei.example:8080/embed?token=abc#frag");

        assert_eq!(
            redacted,
            "http://%3Credacted%3E:%3Credacted%3E@tei.example:8080/embed"
        );
        assert!(!redacted.contains("secret"));
        assert!(!redacted.contains("token=abc"));
    }

    #[test]
    fn redact_url_for_log_handles_unparseable_urls() {
        assert_eq!(redact_url_for_log("not a url?token=secret"), "not a url");
    }
}
