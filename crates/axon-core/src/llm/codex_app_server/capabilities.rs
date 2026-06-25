//! Lightweight Codex app-server capability probe.
//!
//! After the `initialize` handshake, sends `model/list` and
//! `account/rateLimits/read` requests and parses the v2 responses. Used by
//! `axon doctor` to surface available models and rate-limit headroom when the
//! configured LLM backend is `codex-app-server`.
//!
//! The probe is best-effort: individual method failures are captured as
//! `None`/error strings rather than propagated, so a Codex version that omits
//! one of the methods does not fail the doctor report.

use serde_json::{Value, json};
use std::error::Error as StdError;

type BoxError = Box<dyn StdError + Send + Sync>;

/// JSON-RPC ids used exclusively by the capability probe (distinct from the
/// synthesis handshake ids 0–2 in `protocol.rs`).
const ID_INITIALIZE: i64 = 0;
const ID_MODEL_LIST: i64 = 10;
const ID_RATE_LIMITS: i64 = 11;

/// A single model entry from `model/list`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexModelInfo {
    pub id: String,
    /// Default reasoning effort reported by the server (e.g. `"medium"`).
    pub default_effort: Option<String>,
}

/// Rate-limit snapshot from `account/rateLimits/read`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexRateLimits {
    /// Requests remaining in the current window (if reported).
    pub requests_remaining: Option<u64>,
    /// Tokens remaining in the current window (if reported).
    pub tokens_remaining: Option<u64>,
    /// Raw snapshot for fields Axon doesn't explicitly decode.
    pub raw: Option<Value>,
}

/// Result of a capability probe against the Codex app-server.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexCapabilities {
    /// Available models, or an error string if `model/list` failed.
    pub models: Result<Vec<CodexModelInfo>, String>,
    /// Rate-limit headroom, or an error string if `account/rateLimits/read` failed.
    pub rate_limits: Result<CodexRateLimits, String>,
}

impl CodexCapabilities {
    /// Converts the capability snapshot to a `serde_json::Value` suitable for
    /// inclusion in the doctor services map.
    pub fn to_json(&self) -> Value {
        json!({
            "models": match &self.models {
                Ok(models) => json!(models),
                Err(e) => json!({ "error": e }),
            },
            "rate_limits": match &self.rate_limits {
                Ok(rl) => json!(rl),
                Err(e) => json!({ "error": e }),
            },
        })
    }
}

/// Parse a `model/list` response body into a list of [`CodexModelInfo`].
pub fn parse_model_list(result: &Value) -> Vec<CodexModelInfo> {
    let models = result
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    models
        .iter()
        .filter_map(|m| {
            let id = m.get("id").and_then(Value::as_str)?.to_string();
            let default_effort = m
                .get("defaultEffort")
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(CodexModelInfo { id, default_effort })
        })
        .collect()
}

/// Parse an `account/rateLimits/read` response body into [`CodexRateLimits`].
pub fn parse_rate_limits(result: &Value) -> CodexRateLimits {
    let requests_remaining = result
        .get("requestsRemaining")
        .and_then(Value::as_u64)
        .or_else(|| result.get("requests_remaining").and_then(Value::as_u64));
    let tokens_remaining = result
        .get("tokensRemaining")
        .and_then(Value::as_u64)
        .or_else(|| result.get("tokens_remaining").and_then(Value::as_u64));
    CodexRateLimits {
        requests_remaining,
        tokens_remaining,
        raw: Some(result.clone()),
    }
}

/// Drive the capability probe handshake against an already-open `lines` reader
/// and `stdin` writer.
///
/// Sends `initialize` → collects the init response → sends `model/list` +
/// `account/rateLimits/read` → collects both responses.
pub async fn run_capability_probe(
    stdin: &mut tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
    version: &str,
) -> CodexCapabilities {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let init_line = json!({
        "method": "initialize",
        "id": ID_INITIALIZE,
        "params": {
            "clientInfo": { "name": "axon", "title": "Axon", "version": version },
            "capabilities": Value::Null,
        }
    })
    .to_string();

    let mut lines = BufReader::new(stdout).lines();

    // Send initialize.
    if let Err(e) = write_line(stdin, &init_line).await {
        let msg = format!("failed to send initialize: {e}");
        return CodexCapabilities {
            models: Err(msg.clone()),
            rate_limits: Err(msg),
        };
    }

    // Wait for the initialize response (id == 0).
    let mut got_init = false;
    for _ in 0..20 {
        match lines.next_line().await {
            Ok(Some(line)) if line.trim().is_empty() => continue,
            Ok(Some(line)) => {
                if let Ok(val) = serde_json::from_str::<Value>(line.trim())
                    && val.get("id").and_then(Value::as_i64) == Some(ID_INITIALIZE)
                {
                    got_init = true;
                    break;
                }
            }
            _ => break,
        }
    }

    if !got_init {
        let msg = "codex app-server did not respond to initialize within read budget".to_string();
        return CodexCapabilities {
            models: Err(msg.clone()),
            rate_limits: Err(msg),
        };
    }

    // Send initialized notification + both queries.
    let send_result: Result<(), BoxError> = async {
        write_line(
            stdin,
            &json!({ "method": "initialized", "params": {} }).to_string(),
        )
        .await?;
        write_line(
            stdin,
            &json!({ "method": "model/list", "id": ID_MODEL_LIST, "params": {} }).to_string(),
        )
        .await?;
        write_line(
            stdin,
            &json!({
                "method": "account/rateLimits/read",
                "id": ID_RATE_LIMITS,
                "params": {}
            })
            .to_string(),
        )
        .await?;
        stdin.flush().await?;
        Ok(())
    }
    .await;

    if let Err(e) = send_result {
        let msg = format!("failed to send capability queries: {e}");
        return CodexCapabilities {
            models: Err(msg.clone()),
            rate_limits: Err(msg),
        };
    }

    // Collect both responses; stop once both arrive or after a read budget.
    let mut models_result: Option<Result<Vec<CodexModelInfo>, String>> = None;
    let mut rate_limits_result: Option<Result<CodexRateLimits, String>> = None;

    for _ in 0..40 {
        if models_result.is_some() && rate_limits_result.is_some() {
            break;
        }
        match lines.next_line().await {
            Ok(Some(line)) if line.trim().is_empty() => continue,
            Ok(Some(line)) => {
                if let Ok(val) = serde_json::from_str::<Value>(line.trim()) {
                    match val.get("id").and_then(Value::as_i64) {
                        Some(id) if id == ID_MODEL_LIST => {
                            models_result = Some(
                                extract_result(&val)
                                    .map(|r| parse_model_list(&r))
                                    .map_err(|e| format!("model/list error: {e}")),
                            );
                        }
                        Some(id) if id == ID_RATE_LIMITS => {
                            rate_limits_result = Some(
                                extract_result(&val)
                                    .map(|r| parse_rate_limits(&r))
                                    .map_err(|e| format!("account/rateLimits/read error: {e}")),
                            );
                        }
                        _ => {}
                    }
                }
            }
            _ => break,
        }
    }

    CodexCapabilities {
        models: models_result
            .unwrap_or_else(|| Err("model/list: no response received".to_string())),
        rate_limits: rate_limits_result
            .unwrap_or_else(|| Err("account/rateLimits/read: no response received".to_string())),
    }
}

/// Extract the `result` field from a JSON-RPC response, or surface the `error`.
fn extract_result(response: &Value) -> Result<Value, String> {
    if let Some(err) = response.get("error") {
        return Err(err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("unknown RPC error")
            .to_string());
    }
    Ok(response
        .get("result")
        .cloned()
        .unwrap_or(Value::Object(Default::default())))
}

async fn write_line(stdin: &mut tokio::process::ChildStdin, line: &str) -> Result<(), BoxError> {
    use tokio::io::AsyncWriteExt;
    stdin.write_all(line.as_bytes()).await?;
    stdin.write_all(b"\n").await?;
    Ok(())
}

#[cfg(test)]
#[path = "capabilities_tests.rs"]
mod tests;
