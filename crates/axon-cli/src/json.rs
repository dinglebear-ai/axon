//! Shared `--json` stdout render gate.
//!
//! Every CLI command that emits machine-readable `--json` output should
//! route its result payload through [`print_json_gated`] rather than calling
//! `println!("{}", serde_json::to_string_pretty(...))` directly. This runs
//! the payload through the shared redaction boundary
//! (`axon_core::redact::RedactionContext::cli_json`) before it reaches
//! stdout — the last-mile boundary before a caller scripts against or pastes
//! this output.
//!
//! Fail-closed: redaction itself is infallible for JSON values (`redact_json`
//! never fails — it scrubs/drops offending fields in place), so there is no
//! error path to propagate here. The gate's job is simply to guarantee the
//! render call site cannot skip the scrub, not to introduce a new failure
//! mode.
//!
//! Explicit secret-reveal and file-export paths serialize separately by design;
//! normal machine-readable command output uses this gate.

use axon_core::redact::{DefaultRedactor, RedactionContext, Redactor};
use serde::Serialize;
use std::error::Error;

/// Serialize `value` to pretty-printed JSON, redact it through the CLI JSON
/// surface, and print the result to stdout.
pub fn print_json_gated<T: Serialize>(value: &T) -> Result<(), Box<dyn Error>> {
    let json = serde_json::to_value(value)?;
    let (redacted, _report) =
        DefaultRedactor::new().redact_json(json, &RedactionContext::cli_json());
    println!("{}", serde_json::to_string_pretty(&redacted)?);
    Ok(())
}

#[cfg(test)]
#[path = "json_tests.rs"]
mod tests;
