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
    // Conservative byte-based upper bound, not a model tokenizer count.
    text.len().max(1)
}

#[cfg(test)]
pub(super) fn pack_batches(
    inputs: &[String],
    limits: BatchLimits,
) -> Result<Vec<IndexedBatch<'_>>, &'static str> {
    let counts = inputs
        .iter()
        .map(|text| estimated_tokens(text))
        .collect::<Vec<_>>();
    pack_batches_counted(inputs, &counts, limits)
}

pub(super) fn pack_batches_counted<'a>(
    inputs: &'a [String],
    counts: &[usize],
    limits: BatchLimits,
) -> Result<Vec<IndexedBatch<'a>>, &'static str> {
    if counts.len() != inputs.len() {
        return Err("token counts do not match embedding inputs");
    }
    let mut ordered = inputs
        .iter()
        .enumerate()
        .map(|(index, text)| (counts[index], text.len(), index, text))
        .collect::<Vec<_>>();
    ordered.sort_by_key(|&(tokens, chars, index, _)| (tokens, chars, index));
    let mut batches = Vec::new();
    let mut indices = Vec::new();
    let mut texts = Vec::new();
    let mut batch_tokens = 0_usize;
    let mut batch_bytes = 32_usize;
    for (tokens, _, index, text) in ordered {
        let bytes = serialized_input_bytes(text);
        if bytes.saturating_add(32) > limits.max_batch_bytes {
            return Err("one embedding input exceeds the configured payload limit");
        }
        if tokens > limits.max_input_tokens || tokens > limits.max_batch_tokens {
            return Err("one embedding input exceeds the configured token limit");
        }
        if !texts.is_empty()
            && (texts.len() >= limits.max_inputs
                || batch_tokens.saturating_add(tokens) > limits.max_batch_tokens
                || batch_bytes.saturating_add(bytes) > limits.max_batch_bytes)
        {
            push_batch(&mut batches, &mut indices, &mut texts);
            batch_tokens = 0;
            batch_bytes = 32;
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

/// JSON string bytes plus comma; reserve envelope bytes separately.
pub(super) fn serialized_input_bytes(text: &str) -> usize {
    text.bytes().fold(3_usize, |size, byte| {
        size.saturating_add(match byte {
            b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
            0..=31 => 6,
            _ => 1,
        })
    })
}
