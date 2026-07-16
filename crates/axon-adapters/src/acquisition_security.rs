//! SSRF audit wiring for user-supplied source URLs.
//!
//! `axon_core::http::validate_url_with_audit` and
//! `axon_observe::security_audit::emit_security_audit` are well-implemented
//! but, before this module, nothing in the production dispatch/fetch path
//! called them — every real caller used the plain `validate_url`, so the
//! "SSRF Policy" section of `docs/pipeline-unification/runtime/security-contract.md`
//! ("every fetched URL records requested URL, canonical URL, resolved IP
//! class, redirect chain position, policy decision, and a redacted-headers
//! indicator") was satisfied by code that existed but never ran.
//!
//! [`validate_source_url`] is the one seam every acquire path that validates
//! a user-supplied source URL before a real side effect (clone, HTTP fetch,
//! `yt-dlp` invocation) now calls through, so the SSRF policy decision on that
//! URL is actually recorded — see `git_acquire::clone_git_repo`,
//! `feed_acquire::fetch_feed_to_file`, and
//! `youtube_acquire::{fetch_videos, fetch_playlist}`.
//!
//! Uses a fresh [`TracingObservabilitySink`] per call rather than threading a
//! shared, job-correlated sink through `ServiceContext` and every acquire
//! function's signature (none of `clone_git_repo`/`fetch_feed_to_file`/
//! `fetch_videos`/`fetch_playlist` receive a `ServiceContext` today, and
//! neither do the `dispatch::dispatch_{git,feed,youtube}` callers between them
//! and `source::index_source_with_auth`). A per-URL SSRF check is not part of
//! a job's sequenced progress stream, so a fresh sink's per-call sequence
//! number (always `1`) and empty `job_id` cost nothing — the record still
//! carries the requested/canonical URL, resolved IP class, and policy
//! decision via `tracing`. Wiring a job-correlated sink through every acquire
//! call site is a larger, separate change than this audit finding asked for.

use axon_core::http::{HttpError, validate_url_with_audit};
use axon_observe::collector::ObservabilitySink;
use axon_observe::security_audit::emit_security_audit;
use axon_observe::sink::TracingObservabilitySink;

/// Validate `url` for SSRF policy and emit the resulting
/// [`axon_api::source::SecurityAuditEvent`] through `sink`, returning the same
/// `Result` [`axon_core::http::validate_url`] would.
///
/// Split out of [`validate_source_url`] so tests can inject an in-memory sink
/// (`axon_observe::testing::InMemoryObservabilitySink`) and assert an event
/// was actually recorded, instead of only asserting the pass/fail `Result`.
pub(crate) async fn validate_source_url_audited(
    url: &str,
    sink: &dyn ObservabilitySink,
) -> Result<(), HttpError> {
    // redirect_chain_index=0: this validates the original request URL, not a
    // resolved redirect hop (the acquire paths that call this do not follow
    // redirects manually before this check). headers_present=false: none of
    // the current call sites attach caller-supplied headers to the request
    // this check gates.
    let (result, event) = validate_url_with_audit(url, 0, false);
    if let Err(emit_err) = emit_security_audit(sink, &event).await {
        // The audit trail is best-effort: a sink failure must not turn an
        // otherwise-allowed fetch into a denial, and must not mask the real
        // SSRF policy error when the check itself failed.
        tracing::warn!(error = %emit_err, "failed to emit ssrf security audit event");
    }
    result
}

/// Production entrypoint used by the git/feed/youtube acquire paths: validates
/// `url` for SSRF policy and audits the outcome via a fresh
/// [`TracingObservabilitySink`]. See the module doc comment for why a fresh
/// per-call sink (rather than a shared, job-correlated one) is the right
/// tradeoff at these call sites today.
pub async fn validate_source_url(url: &str) -> Result<(), HttpError> {
    validate_source_url_audited(url, &TracingObservabilitySink::new()).await
}

#[cfg(test)]
#[path = "acquisition_security_tests.rs"]
mod tests;
