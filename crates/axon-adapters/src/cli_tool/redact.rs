//! stdout/stderr redaction applied before CLI tool output is returned for
//! persistence. Secret classification is owned by `axon-core`; this adapter
//! must not maintain a broader substring policy of its own.

/// Replaces detector-confirmed secret spans while preserving surrounding tool
/// output. Downstream persistence boundaries remain responsible for rejecting
/// any residual secret shape that cannot be safely isolated here.
///
/// Returns `(redacted_text, was_redacted)`; the flag records whether shared
/// redaction changed the text.
pub(super) fn redact_text(text: &str) -> (String, bool) {
    let redacted = axon_core::redact::redact_retrievable_body_secrets(text);
    let changed = redacted != text;
    (redacted, changed)
}

#[cfg(test)]
#[path = "redact_tests.rs"]
mod tests;
