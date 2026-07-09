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
//! NOTE: as of this writing this module is not yet wired into every `--json`
//! call site in `commands/*.rs` — most call `println!` directly. This is a
//! known scope gap (see the redaction boundary extension plan); the CLI JSON
//! chokepoint is not currently a single shared function the way vector
//! payloads / job events / graph evidence / memory rows are. `common_jobs.rs`
//! is wired as the flagship adoption; broader retrofit across all call sites
//! is tracked separately.

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
