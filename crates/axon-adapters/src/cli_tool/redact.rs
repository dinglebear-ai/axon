//! stdout/stderr redaction applied before CLI tool output is returned for
//! persistence. Mirrors the secret shapes in `mcp_tool::redact`.

const SECRET_PATTERNS: &[&str] = &[
    "authorization",
    "Authorization",
    "Bearer ",
    "bearer ",
    "api_key",
    "apikey",
    "api-key",
    "secret",
    "password",
    "passwd",
    "token=",
    "AKIA",
];

/// Redacts lines that look like they carry a secret. Conservative by
/// design: a whole matching line is replaced rather than attempting to
/// splice out just the secret substring, since token boundaries in
/// untrusted tool output cannot be trusted.
///
/// Returns `(redacted_text, any_line_redacted)`. The bool is tracked
/// explicitly rather than derived by comparing the output to the input,
/// because line-splitting/rejoining alone (independent of any redaction)
/// changes trailing-newline byte content and would otherwise read as a
/// false-positive redaction.
pub(super) fn redact_text(text: &str) -> (String, bool) {
    if text.is_empty() {
        return (String::new(), false);
    }
    let mut any_redacted = false;
    let line_redacted = text
        .lines()
        .map(|line| {
            if line_looks_sensitive(line) {
                any_redacted = true;
                "[redacted-secret]"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let out = axon_core::redact::redact_secrets(&line_redacted);
    let core_redacted = out != line_redacted;
    (out, any_redacted || core_redacted)
}

fn line_looks_sensitive(line: &str) -> bool {
    SECRET_PATTERNS.iter().any(|pattern| line.contains(pattern))
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
