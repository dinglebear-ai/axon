use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::core::logging::{log_debug, log_info, log_warn};
use crate::crates::vector::ops::qdrant::env_usize_clamped;
use rand::RngExt as _;
use reqwest::StatusCode;
use std::error::Error;
use std::time::Duration;

const TEI_MAX_RETRIES_DEFAULT: usize = 5;
const TEI_REQUEST_TIMEOUT_MS_DEFAULT: u64 = 30_000;
const TEI_REQUEST_TIMEOUT_MS_MIN: u64 = 100;
const TEI_REQUEST_TIMEOUT_MS_MAX: u64 = 600_000;
const TEI_MAX_BACKOFF_MS: u64 = 60_000;

fn retry_delay(attempt: usize) -> Duration {
    let base_ms = 1000_u64.saturating_mul(2u64.saturating_pow(attempt as u32 - 1));
    let capped_ms = base_ms.min(TEI_MAX_BACKOFF_MS);
    let jitter = Duration::from_millis(rand::rng().random_range(0..500));
    Duration::from_millis(capped_ms) + jitter
}

fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

fn request_timeout_ms_from_env() -> u64 {
    std::env::var("TEI_REQUEST_TIMEOUT_MS")
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .map(|v| v.clamp(TEI_REQUEST_TIMEOUT_MS_MIN, TEI_REQUEST_TIMEOUT_MS_MAX))
        .unwrap_or(TEI_REQUEST_TIMEOUT_MS_DEFAULT)
}

enum ChunkOutcome {
    Vectors(Vec<Vec<f32>>),
    /// Chunk was too large (HTTP 413); caller should split and retry.
    Split,
}

async fn send_chunk_with_retries(
    client: &reqwest::Client,
    embed_url: &str,
    chunk: &[String],
    max_attempts: usize,
    request_timeout_ms: u64,
) -> Result<ChunkOutcome, Box<dyn Error>> {
    for attempt in 1..=max_attempts {
        let resp = match client
            .post(embed_url)
            .timeout(Duration::from_millis(request_timeout_ms))
            .json(&serde_json::json!({"inputs": chunk}))
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(err) => {
                if attempt < max_attempts {
                    let delay = retry_delay(attempt);
                    log_warn(&format!(
                        "tei_embed retry transport_error attempt={attempt}/{max_attempts} delay_ms={} url={} err={}",
                        delay.as_millis(),
                        embed_url,
                        err
                    ));
                    tokio::time::sleep(delay).await;
                    continue;
                }
                return Err(format!(
                    "TEI request transport error for {} after {attempt}/{max_attempts} attempts: {err}",
                    embed_url
                )
                .into());
            }
        };

        let status = resp.status();
        if status.is_success() {
            match resp.json::<Vec<Vec<f32>>>().await {
                Ok(v) => return Ok(ChunkOutcome::Vectors(v)),
                Err(err) => {
                    if attempt < max_attempts {
                        let delay = retry_delay(attempt);
                        log_warn(&format!(
                            "tei_embed retry decode_error attempt={attempt}/{max_attempts} delay_ms={} url={} err={}",
                            delay.as_millis(),
                            embed_url,
                            err
                        ));
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(format!(
                        "TEI response decode error for {} after {attempt}/{max_attempts} attempts: {err}",
                        embed_url
                    )
                    .into());
                }
            }
        }

        if status == StatusCode::PAYLOAD_TOO_LARGE && chunk.len() > 1 {
            return Ok(ChunkOutcome::Split);
        }

        if is_retryable_status(status) && attempt < max_attempts {
            let delay = retry_delay(attempt);
            log_warn(&format!(
                "tei_embed retry status attempt={attempt}/{max_attempts} delay_ms={} url={} status={}",
                delay.as_millis(),
                embed_url,
                status
            ));
            tokio::time::sleep(delay).await;
            continue;
        }

        let body = resp
            .text()
            .await
            .unwrap_or_else(|_| "<response body unavailable>".to_string());
        let body_preview: String = body.chars().take(240).collect();
        return Err(format!(
            "TEI request failed with status {} for {} after {attempt}/{max_attempts} attempts; body={}",
            status, embed_url, body_preview
        )
        .into());
    }
    Err(format!("TEI embed exhausted {max_attempts} attempts for {embed_url}").into())
}

pub(crate) async fn tei_embed(
    cfg: &Config,
    inputs: &[String],
) -> Result<Vec<Vec<f32>>, Box<dyn Error>> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let client = http_client()?;
    let mut vectors = Vec::new();

    let configured = env_usize_clamped("TEI_MAX_CLIENT_BATCH_SIZE", 128, 1, 4096);
    let batch_size = configured.min(128);
    let embed_url = format!("{}/embed", cfg.tei_url.trim_end_matches('/'));
    let max_attempts = env_usize_clamped("TEI_MAX_RETRIES", TEI_MAX_RETRIES_DEFAULT, 1, 20);
    let request_timeout_ms = request_timeout_ms_from_env();

    log_info(&format!(
        "tei_embed start chunk_count={} url={}",
        inputs.len(),
        embed_url
    ));
    let _tei_start = std::time::Instant::now();

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
        _tei_start.elapsed().as_millis()
    ));

    Ok(vectors)
}
