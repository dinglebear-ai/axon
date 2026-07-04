pub(crate) mod cutover;
pub(crate) mod sqlite;

use crate::config::Config;
use crate::http::with_path;
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

/// Precomputed LLM-leg results for the doctor report.
///
/// The real LLM backends live in `axon-llm`, which depends on `axon-core`, so
/// `axon-core` cannot execute a completion itself without a crate cycle. The
/// caller (a crate above `axon-llm`, e.g. `axon-services`) runs the bounded
/// LLM probes via `axon_llm::doctor_probe` and injects the results here. When a
/// caller has no LLM access it can pass [`LlmDoctorProbe::unavailable`].
#[derive(Debug, Clone)]
pub struct LlmDoctorProbe {
    /// Deep round-trip: `(ok, detail)` from a minimal real completion.
    pub roundtrip: (bool, String),
    /// Shallow gemini-headless command/config validation: `(ok, detail)`.
    pub gemini_validation: (bool, String),
    /// Codex capability document JSON, when the backend is codex-app-server.
    pub codex_capabilities: Option<Value>,
}

impl LlmDoctorProbe {
    /// Placeholder used when the caller cannot run LLM probes.
    #[must_use]
    pub fn unavailable() -> Self {
        Self {
            roundtrip: (false, "LLM probe not run".to_string()),
            gemini_validation: (false, "LLM probe not run".to_string()),
            codex_capabilities: None,
        }
    }
}

pub(super) fn build_browser_runtime(
    diagnostics: &crate::health::BrowserDiagnosticsPattern,
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

pub async fn build_doctor_report(
    cfg: &Config,
    pending_jobs: i64,
    llm_probe: LlmDoctorProbe,
) -> Result<Value, Box<dyn Error>> {
    sqlite::build(cfg, pending_jobs, llm_probe).await
}

#[cfg(test)]
#[path = "doctor_tests.rs"]
mod tests;
