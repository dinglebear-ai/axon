//! Provider-matched counts from TEI's authenticated, batched `/tokenize` API.
//! No local tokenizer artifacts or model downloads are needed. The API performs
//! tokenization again at embedding time, so expose that overhead separately.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use super::{IndexedBatch, TeiClient, policy, response};

const TOKENIZE_BODY_LIMIT: usize = 16 * 1024 * 1024;
const TOKENIZE_REQUEST_BYTES: usize = 256 * 1024;
const TOKENIZE_BUDGET: Duration = Duration::from_secs(5);

#[derive(Debug, Default)]
pub(crate) struct TokenizationState {
    unsupported: AtomicBool,
    established: AtomicBool,
    probe: tokio::sync::Mutex<()>,
}

#[derive(serde::Serialize)]
struct TokenizeRequest<'a> {
    inputs: &'a [String],
    add_special_tokens: bool,
}

#[derive(serde::Deserialize)]
struct Token {
    // Checking the id prevents a successful unrelated endpoint from being
    // accepted as tokenizer output. Text/offsets need not be allocated.
    #[serde(rename = "id")]
    _id: u32,
}

impl TokenizationState {
    #[cfg(test)]
    pub(crate) fn mark_unsupported(&self) {
        self.unsupported.store(true, Ordering::Release);
    }
}

impl TeiClient {
    pub(super) async fn token_counts(
        &self,
        inputs: &[String],
    ) -> (Vec<usize>, Option<&'static str>) {
        let fallback = |reason| {
            metrics::counter!("axon_tei_tokenization_fallback_total", "reason" => reason)
                .increment(1);
            tracing::warn!(
                code = "embedding.tei.tokenization_fallback",
                reason,
                "TEI packing uses conservative UTF-8 byte counts; actual token usage is unknown. Enable authenticated POST /tokenize on the configured embedding endpoint."
            );
            (
                inputs
                    .iter()
                    .map(|text| policy::estimated_tokens(text))
                    .collect(),
                Some(reason),
            )
        };
        if self.tokenization.unsupported.load(Ordering::Acquire) {
            return fallback("unsupported_endpoint");
        }
        let started = Instant::now();
        let budget = self.request_timeout.min(TOKENIZE_BUDGET);
        // Only initial capability discovery is serialized across provider
        // instances. Normal counting uses the same admission gates as embed.
        let _probe = if !self.tokenization.established.load(Ordering::Acquire) {
            match tokio::time::timeout(budget, self.tokenization.probe.lock()).await {
                Ok(guard) => Some(guard),
                Err(_) => return fallback("tokenization_probe_timeout"),
            }
        } else {
            None
        };
        if self.tokenization.unsupported.load(Ordering::Acquire) {
            return fallback("unsupported_endpoint");
        }
        let result = tokio::time::timeout(
            budget.saturating_sub(started.elapsed()),
            self.count_windows(inputs),
        )
        .await;
        metrics::histogram!("axon_tei_tokenization_seconds")
            .record(started.elapsed().as_secs_f64());
        match result {
            Ok(Ok(counts)) => {
                self.tokenization.established.store(true, Ordering::Release);
                (counts, None)
            }
            Ok(Err(reason)) => fallback(reason),
            Err(_) => fallback("tokenization_timeout"),
        }
    }

    async fn count_windows(&self, inputs: &[String]) -> Result<Vec<usize>, &'static str> {
        let mut counts = Vec::with_capacity(inputs.len());
        let mut start = 0;
        while start < inputs.len() {
            let mut end = start;
            let mut bytes = 64_usize;
            while end < inputs.len() && end - start < self.max_batch_inputs {
                let next = policy::serialized_input_bytes(&inputs[end]);
                if bytes.saturating_add(next) > TOKENIZE_REQUEST_BYTES {
                    break;
                }
                bytes += next;
                end += 1;
            }
            if end == start {
                return Err("tokenization_payload_limit");
            }
            let window = &inputs[start..end];
            let (_profile, _request, _inputs) = self
                .acquire_admission(window.len())
                .await
                .map_err(|_| "tokenization_admission_closed")?;
            let mut request = self.client.post(&self.tokenize_url);
            if let Some(token) = &self.bearer_token {
                request = request.bearer_auth(token);
            }
            metrics::counter!("axon_tei_tokenization_requests_total").increment(1);
            let resp = request
                .timeout(self.request_timeout.min(TOKENIZE_BUDGET))
                .json(&TokenizeRequest {
                    inputs: window,
                    add_special_tokens: true,
                })
                .send()
                .await
                .map_err(|_| "tokenization_transport")?;
            if matches!(resp.status().as_u16(), 404 | 405 | 501) {
                self.tokenization.unsupported.store(true, Ordering::Release);
                return Err("unsupported_endpoint");
            }
            if !resp.status().is_success() {
                return Err(match resp.status().as_u16() {
                    401 | 403 => "tokenization_authorization_denied",
                    413 => "tokenization_payload_rejected",
                    429 => "tokenization_overloaded",
                    _ => "tokenization_http_error",
                });
            }
            let rows: Vec<Vec<Token>> = response::read_json(resp, TOKENIZE_BODY_LIMIT)
                .await
                .map_err(|_| "tokenization_invalid_or_oversized_response")?;
            if rows.len() != window.len() || rows.iter().any(Vec::is_empty) {
                return Err("tokenization_count_mismatch");
            }
            counts.extend(rows.iter().map(Vec::len));
            start = end;
        }
        Ok(counts)
    }
}

pub(super) fn record_packing(
    batches: &[IndexedBatch<'_>],
    counts: &[usize],
    token_limit: usize,
    exact: bool,
) {
    let mode = if exact {
        "provider_tokens"
    } else {
        "utf8_bytes"
    };
    for (indices, _) in batches {
        let tokens = indices
            .iter()
            .fold(0_usize, |sum, index| sum.saturating_add(counts[*index]));
        metrics::histogram!("axon_tei_packed_budget_utilization", "accounting" => mode)
            .record(tokens as f64 / token_limit.max(1) as f64);
        let padded = indices
            .iter()
            .map(|index| counts[*index])
            .max()
            .unwrap_or(0)
            .saturating_mul(indices.len());
        if padded > 0 {
            metrics::histogram!("axon_tei_packed_padding_ratio", "accounting" => mode)
                .record(1.0 - tokens as f64 / padded as f64);
        }
        metrics::histogram!("axon_tei_packed_inputs", "accounting" => mode)
            .record(indices.len() as f64);
    }
}

#[cfg(test)]
#[path = "tokenization_tests.rs"]
mod tests;
