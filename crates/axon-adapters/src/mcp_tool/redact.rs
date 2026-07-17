//! Redaction applied to MCP tool call output before it is returned for
//! persistence or embedding. Mirrors `cli_tool::redact`'s secret shapes.

const SECRET_PATTERNS: &[&str] = &[
    "authorization",
    "Authorization",
    "Bearer secret",
    "bearer ",
    "api_key",
    "apikey",
    "secret",
    "password",
    "AKIA",
];

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

    let mut redacted = output.to_string();
    let mut changed = false;
    for pattern in SECRET_PATTERNS {
        if redacted.contains(pattern) {
            changed = true;
            redacted = redacted.replace(pattern, "[redacted-secret]");
        }
    }
    let core_redacted = axon_core::redact::redact_secrets(&redacted);
    let core_changed = core_redacted != redacted;
    (core_redacted, changed || core_changed)
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
            if core_redacted != *text || text_looks_sensitive(text) {
                *text = if core_redacted != *text {
                    core_redacted
                } else {
                    "[redacted-secret]".to_string()
                };
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

fn text_looks_sensitive(text: &str) -> bool {
    SECRET_PATTERNS.iter().any(|pattern| text.contains(pattern))
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
