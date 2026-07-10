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
    let mut redacted = output.to_string();
    let mut changed = false;
    for pattern in SECRET_PATTERNS {
        if redacted.contains(pattern) {
            changed = true;
            redacted = redacted.replace(pattern, "[redacted-secret]");
        }
    }
    (redacted, changed)
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
