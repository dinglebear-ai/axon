pub(crate) mod sqlite;

use crate::cli::commands::probe::with_path;
use crate::core::config::Config;
use serde_json::Value;
use std::error::Error;
use std::future::Future;
use std::time::Instant;

pub(super) fn elapsed_ms(start: Instant) -> u64 {
    let ms = start.elapsed().as_millis();
    if ms > u128::from(u64::MAX) {
        u64::MAX
    } else {
        ms as u64
    }
}

pub(super) async fn probe_tei_info(
    url: &str,
    client: &reqwest::Client,
) -> (Option<Value>, Option<String>) {
    if url.trim().is_empty() {
        return (None, Some("not configured".to_string()));
    }

    let mut last_error = None;
    for path in ["/info", "/v1/info"] {
        let endpoint = with_path(url, path);
        match client.get(endpoint).send().await {
            Ok(resp) if resp.status().is_success() => {
                let status = resp.status();
                match resp.json::<Value>().await {
                    Ok(json) => return (Some(json), Some(format!("{path} {status}"))),
                    Err(err) => last_error = Some(format!("{path} invalid json: {err}")),
                }
            }
            Ok(resp) => last_error = Some(format!("{path} {}", resp.status())),
            Err(err) => last_error = Some(err.to_string()),
        }
    }

    (None, last_error)
}

pub(super) fn tei_model_from_info(info: &Value) -> Option<String> {
    let str_field = |key: &str| info.get(key).and_then(Value::as_str).map(str::to_string);
    str_field("model_id")
        .or_else(|| str_field("model_name"))
        .or_else(|| {
            let model = info.get("model")?;
            model
                .as_str()
                .or_else(|| model.get("id").and_then(Value::as_str))
                .or_else(|| model.get("name").and_then(Value::as_str))
                .map(str::to_string)
        })
}

pub(super) fn tei_info_summary(info: &Value) -> Option<String> {
    let mut parts = Vec::new();
    for key in [
        "model_sha",
        "max_concurrent_requests",
        "max_client_batch_size",
        "max_batch_total_tokens",
        "max_input_tokens",
        "max_input_length",
    ] {
        if let Some(value) = info.get(key) {
            parts.push(format!("{key}={value}"));
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

pub(super) async fn timed_probe<T, F>(future: F) -> (T, u64)
where
    F: Future<Output = T>,
{
    let start = Instant::now();
    let value = future.await;
    (value, elapsed_ms(start))
}

/// Hard ceiling on how long the doctor LLM round-trip is allowed to take.
/// Independent of `completion_timeout_secs` (which can be 300s) so `doctor`
/// stays fast and never hangs on an unreachable backend.
const LLM_PROBE_TIMEOUT_SECS: u64 = 12;

/// Deep LLM probe: attempt a minimal real completion through the configured
/// backend (gemini-headless or openai-compat — both dispatch via
/// `core::llm::complete_text`). This catches the most common production failure
/// that the shallow command-presence check misses: expired Gemini credentials
/// or an unreachable OpenAI-compatible endpoint.
///
/// Non-fatal and bounded: returns `(ok, detail)` and is wrapped in a hard
/// timeout. Any error — including timeout — is reported as `(false, detail)`,
/// never propagated, so a broken LLM leg degrades the report instead of
/// failing `doctor`.
pub(super) async fn probe_llm_roundtrip(cfg: &Config) -> (bool, String) {
    use crate::core::llm::{CompletionRequest, LlmBackendConfig, complete_text};
    use std::time::Duration;

    // Build a request from cfg but clamp the per-call timeout to the probe
    // ceiling so a misconfigured long timeout can't stall the doctor.
    let mut backend = LlmBackendConfig::from_config(cfg);
    backend.completion_timeout_secs = backend
        .completion_timeout_secs
        .clamp(1, LLM_PROBE_TIMEOUT_SECS);

    let req = CompletionRequest {
        system_prompt: Some("Reply with the single word: ok".to_string()),
        user_prompt: "ping".to_string(),
        model: None,
        stream: false,
        effort: None,
        backend,
    };

    let probe = complete_text(req);
    match tokio::time::timeout(Duration::from_secs(LLM_PROBE_TIMEOUT_SECS), probe).await {
        Ok(Ok(resp)) => {
            let preview: String = resp.text.trim().chars().take(40).collect();
            (
                true,
                format!("LLM round-trip succeeded (reply: {preview:?})"),
            )
        }
        Ok(Err(err)) => {
            // Truncate so a verbose upstream body doesn't bloat the report.
            let detail: String = err.to_string().chars().take(240).collect();
            (false, format!("LLM round-trip failed: {detail}"))
        }
        Err(_) => (
            false,
            format!("LLM round-trip timed out after {LLM_PROBE_TIMEOUT_SECS}s"),
        ),
    }
}

pub(super) fn build_browser_runtime(
    diagnostics: &crate::core::health::BrowserDiagnosticsPattern,
) -> Value {
    serde_json::json!({
        "selection": "chrome",
        "diagnostics": {
            "enabled": diagnostics.enabled,
            "screenshot": diagnostics.screenshot,
            "events": diagnostics.events,
            "output_dir": diagnostics.output_dir,
        },
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorModeReport {
    pub local_runtime: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorCapability {
    pub tier: String,
    pub available: bool,
    pub impact: Vec<String>,
    pub remedies: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DoctorReport {
    pub mode: DoctorModeReport,
    pub capabilities: Vec<DoctorCapability>,
    pub recommendations: Vec<String>,
    pub services: Value,
}

impl DoctorReport {
    pub fn sample_for_tests() -> Self {
        Self {
            mode: DoctorModeReport {
                local_runtime: "sqlite_in_process".to_string(),
            },
            capabilities: vec![DoctorCapability {
                tier: "tier_1_crawl_retrieve".to_string(),
                available: true,
                impact: vec!["crawl and retrieve are available".to_string()],
                remedies: vec![],
            }],
            recommendations: vec!["start qdrant and tei with `just services-up`".to_string()],
            services: serde_json::json!({
                "qdrant": {
                    "ok": true,
                    "configured_url": "http://axon-qdrant:6333",
                    "effective_url": "http://127.0.0.1:53333"
                }
            }),
        }
    }
}

pub async fn build_doctor_report(cfg: &Config) -> Result<Value, Box<dyn Error>> {
    sqlite::build(cfg).await
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
