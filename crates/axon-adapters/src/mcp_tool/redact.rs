//! Redaction applied to MCP tool call output before it is returned for
//! persistence or embedding. Value classification is owned by `axon-core`;
//! JSON key classification remains structural to this adapter.

/// Returns `(redacted_payload, was_redacted)`. Parsed JSON tracks recursive
/// field/value replacements explicitly; free text derives the flag by
/// comparing shared redaction output with the original string.
pub(super) fn redact_mcp_output(output: &str) -> (String, bool) {
    if let Ok(mut value) = serde_json::from_str::<serde_json::Value>(output) {
        let mut changed = false;
        redact_json_value(&mut value, &mut changed);
        let serialized = serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string());
        return (serialized, changed);
    }

    let redacted = axon_core::redact::redact_retrievable_body_secrets(output);
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
            let core_redacted = axon_core::redact::redact_retrievable_body_secrets(text);
            if core_redacted != *text {
                *text = core_redacted;
                *changed = true;
            }
        }
        _ => {}
    }
}

fn key_is_sensitive(key: &str) -> bool {
    axon_core::redact::is_secret_like(key)
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
