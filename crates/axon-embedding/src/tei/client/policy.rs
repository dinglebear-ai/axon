//! TEI batching, retry, and endpoint transport policy.

use std::time::{Duration, Instant};

use reqwest::StatusCode;

use super::{BatchLimits, IndexedBatch, MAX_BACKOFF_MS, MAX_CLIENT_BATCH_SIZE};

pub(super) fn credential_transport_is_safe(endpoint: &url::Url, credentials_present: bool) -> bool {
    !credentials_present
        || endpoint.scheme() == "https"
        || endpoint.host_str().is_some_and(|host| {
            host.eq_ignore_ascii_case("localhost")
                || host
                    .parse::<std::net::IpAddr>()
                    .is_ok_and(|address| address.is_loopback())
        })
}

pub(super) fn estimated_tokens(text: &str) -> usize {
    let (ascii, non_ascii_bytes) = text.chars().fold((0_usize, 0_usize), |counts, ch| {
        if ch.is_ascii() {
            (counts.0 + 1, counts.1)
        } else {
            (counts.0, counts.1 + ch.len_utf8())
        }
    });
    ascii.div_ceil(2).saturating_add(non_ascii_bytes).max(1)
}

pub(super) fn pack_batches(
    inputs: &[String],
    limits: BatchLimits,
) -> Result<Vec<IndexedBatch<'_>>, &'static str> {
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
            batches.push((vec![index], vec![text.as_str()]));
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
        texts.push(text.as_str());
        batch_tokens = batch_tokens.saturating_add(tokens);
        batch_bytes = batch_bytes.saturating_add(bytes);
    }
    push_batch(&mut batches, &mut indices, &mut texts);
    Ok(batches)
}

fn push_batch<'a>(
    batches: &mut Vec<IndexedBatch<'a>>,
    indices: &mut Vec<usize>,
    texts: &mut Vec<&'a str>,
) {
    if !texts.is_empty() {
        batches.push((std::mem::take(indices), std::mem::take(texts)));
    }
}

pub(super) fn resolve_batch_size(config_batch: usize) -> usize {
    config_batch.clamp(1, MAX_CLIENT_BATCH_SIZE)
}

pub(super) fn parse_retry_after(value: &reqwest::header::HeaderValue) -> Option<Duration> {
    value
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

pub fn is_retryable_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

pub fn is_batch_too_large(status: StatusCode) -> bool {
    status == StatusCode::PAYLOAD_TOO_LARGE
}

pub(super) fn error_category(err: &reqwest::Error) -> &'static str {
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

pub fn retry_delay(attempt: usize, started: Instant, base_ms: u64) -> Duration {
    let exponent = (attempt as u32).saturating_sub(1);
    let scaled_ms = base_ms.saturating_mul(2u64.saturating_pow(exponent));
    let capped_ms = scaled_ms.min(MAX_BACKOFF_MS);
    let jitter_ms = (started.elapsed().subsec_nanos() as u64) % 500;
    Duration::from_millis(capped_ms + jitter_ms)
}
