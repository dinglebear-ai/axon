//! Session target parsing — normalizes a `session:<provider>:<session_id>`
//! request string into provider / session_id parts. Mirrors the router's
//! `canonical_session()` parser in `axon-route::canonical`, minus the
//! canonical-URI construction (the adapter only needs the parsed parts).

use axon_api::source::ApiError;
use axon_error::ErrorStage;

use crate::adapter::Result;

/// A parsed session target: which agent/tool produced the transcript, and
/// which session it identifies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTarget {
    pub provider: String,
    pub session_id: String,
}

fn err(code: &str, message: impl Into<String>) -> ApiError {
    ApiError::new(
        format!("adapter.session.{code}"),
        ErrorStage::Planning,
        message,
    )
}

/// Parse a `session:<provider>:<session_id>` request string.
///
/// Matches `axon_route::canonical::canonical_session`: strips the leading
/// `session:` prefix, then splits the remainder on the first `:` into
/// provider and session_id. Both parts must be non-empty after trimming.
pub fn parse_session_target(input: &str) -> Result<SessionTarget> {
    let raw = input.trim();
    let rest = raw.strip_prefix("session:").ok_or_else(|| {
        err(
            "target.scheme",
            "session adapter requires a session: target",
        )
    })?;
    let (provider, session_id) = rest.split_once(':').ok_or_else(|| {
        err(
            "target.format",
            "session target must be session:<provider>:<id>",
        )
    })?;
    let provider = provider.trim();
    let session_id = session_id.trim();
    if provider.is_empty() {
        return Err(err(
            "target.provider",
            "session target is missing a provider",
        ));
    }
    if session_id.is_empty() {
        return Err(err(
            "target.session_id",
            "session target is missing a session id",
        ));
    }
    Ok(SessionTarget {
        provider: provider.to_string(),
        session_id: session_id.to_string(),
    })
}

#[cfg(test)]
#[path = "target_tests.rs"]
mod tests;
