//! Redaction applied to MCP tool call output before it is returned for
//! persistence or embedding. Value classification is owned by `axon-core`;
//! JSON key classification remains structural to this adapter.

/// Returns `(redacted_payload, was_redacted)`. `was_redacted` is tracked
/// explicitly rather than derived from `redacted != raw` so the flag stays
/// accurate regardless of future normalization changes to the redacted
/// text.
pub(super) fn redact_mcp_output(output: &str) -> (String, bool) {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(output) {
        let mut changed = false;
        redact_json_value(&mut value, &mut changed);
        let serialized = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        return (serialized, changed);
    }

    let redacted = axon_core::redact::redact_secrets(output);
    let changed = redacted != output;
    (redacted, changed)
}

fn redact_json_value(value: &mut serde_json::Value, changed: &mut bool) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if key_is_sensitive(key) {
                    *value = serde_json::Value::String("[redacted-secret]".to_string());
                    *changed = true;
                } else {
                    redact_json_value(value, changed);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                redact_json_value(value, changed);
            }
        }
        serde_json::Value::String(text) => {
            let core_redacted = axon_core::redact::redact_secrets(text);
            if core_redacted != *text {
                *text = core_redacted;
                *changed = true;
            }
        }
        _ => {}
    }
}

fn key_is_sensitive(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase().replace(['-', '_'], "");
    [
        "authorization",
        "apikey",
        "password",
        "passwd",
        "secret",
        "token",
    ]
    .iter()
    .any(|name| normalized.contains(name))
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
