//! stdout/stderr redaction applied before CLI tool output is returned for
//! persistence. Secret classification is owned by `axon-core`; this adapter
//! must not maintain a broader substring policy of its own.

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
    let redacted = axon_core::redact::redact_secrets(text);
    let changed = redacted != text;
    (redacted, changed)
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
