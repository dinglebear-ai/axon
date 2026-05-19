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

pub async fn build_doctor_report(cfg: &Config) -> Result<Value, Box<dyn Error>> {
    sqlite::build(cfg).await
}
