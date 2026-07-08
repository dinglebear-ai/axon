use crate::schema::{ExtractRequest, McpRenderMode, SearchTimeRange};
use axon_core::config::Config;
use axon_core::config::RenderMode;
use axon_core::error::taxonomy_from_error;
use axon_core::redact::{DefaultRedactor, RedactionContext, Redactor};
use axon_services::transport;
use axon_services::types::ServiceTimeRange;
use rmcp::ErrorData;
use uuid::Uuid;

// Re-export artifact helpers that are imported by multiple handler modules.
pub(super) use super::artifacts::InlineHint;
pub(super) use super::artifacts::respond_with_mode;

pub(super) const MCP_TOOL_SCHEMA_URI: &str = "axon://schema/mcp-tool";

tokio::task_local! {
    /// The [`axon_services::prune::PruneAuthz`] resolved for the in-flight
    /// `axon` tool call, when the action is `prune`.
    ///
    /// `call_tool`'s scope gate resolves the caller's real `AuthContext`
    /// scopes before dispatch (see `server.rs`), then scopes this task-local
    /// around the `tool_router` call so `handle_prune` can read the honest,
    /// per-request authorization without threading an extra parameter through
    /// the rmcp `#[tool]` macro (which does not expose `RequestContext` to
    /// the top-level `axon` tool method the way it does to `ServerHandler`
    /// trait methods). Task-locals are per-async-task, so concurrent MCP
    /// calls on the shared `AxonMcpServer` never observe each other's value.
    pub(super) static CURRENT_PRUNE_AUTHZ: axon_services::prune::PruneAuthz;
}

// --- Error constructors ---

pub(super) fn invalid_params(msg: impl Into<String>) -> ErrorData {
    ErrorData::invalid_params(msg.into(), None)
}

pub(super) fn internal_error(msg: impl Into<String>) -> ErrorData {
    ErrorData::internal_error(msg.into(), None)
}

/// Log the full error (with source chain) server-side, and return an MCP error
/// that includes the top-level error message so callers get an actionable cause
/// instead of a bare `"<context> failed"`.
///
/// Only the error's own `Display` (`{e}`) is forwarded to the client — NOT the
/// `{e:#}` source chain — so deeper details (DSNs, file paths, nested transport
/// URLs) that may surface in `.source()` stay in the server log. This is a
/// general helper called from every MCP handler, so **callers are responsible
/// for ensuring the top-level message is client-safe**: pass an error whose own
/// `Display` is descriptive and free of secrets (e.g. a service-layer error
/// like `"Failed to retrieve any context sources for ask"`), not a raw
/// transport error whose `Display` might embed a URL.
pub(super) fn logged_internal_error(
    context: &str,
    e: &(dyn std::error::Error + 'static),
) -> ErrorData {
    // Walk the source chain explicitly for the server log. anyhow's chain-aware
    // `{e:#}` formatting lives on `anyhow::Error`'s own `Display`; once the error
    // is erased to a `&dyn Error` trait object that wrapper is bypassed, so we
    // follow `.source()` by hand. The depth cap defends against a pathological
    // self-referential `source()` (which would otherwise loop forever and grow
    // `chain` unbounded).
    const MAX_CHAIN_DEPTH: usize = 16;
    let mut chain = e.to_string();
    let mut src = e.source();
    let mut depth = 0;
    while let Some(cause) = src {
        if depth >= MAX_CHAIN_DEPTH {
            // Mark truncation so a reader can't mistake a clipped chain for a
            // genuinely terminated one.
            chain.push_str(&format!(" … (source chain truncated at {MAX_CHAIN_DEPTH})"));
            break;
        }
        chain.push_str(&format!(": {cause}"));
        src = cause.source();
        depth += 1;
    }
    tracing::error!("{context}: {chain}");
    let data = taxonomy_from_error(e).map(|taxonomy| taxonomy.to_mcp_envelope());
    // Fail-closed redaction boundary: callers are asked to pass a
    // client-safe `Display`, but this is the last-mile MCP transport
    // boundary — scrub before the write, not trust alone, in case a caller
    // slips a raw transport error (URL, connection string) through.
    let redactor = DefaultRedactor::new();
    let message = redactor.redact_text(
        &format!("{context} failed: {e}"),
        &RedactionContext::transport_response(),
    );
    ErrorData::internal_error(message, data)
}

// --- URL validation wrappers ---

/// Validate a single URL via the SSRF guard, converting errors to MCP `invalid_params`.
pub(super) fn validate_mcp_url(url: &str) -> Result<(), ErrorData> {
    axon_core::http::validate_url(url).map_err(|e| invalid_params(e.to_string()))
}

/// Validate every URL in a slice via the SSRF guard.
/// Error messages include the index so the client can identify which URL failed.
pub(super) fn validate_mcp_urls(urls: &[String]) -> Result<(), ErrorData> {
    for (i, url) in urls.iter().enumerate() {
        axon_core::http::validate_url(url)
            .map_err(|e| invalid_params(format!("urls[{i}]: {e}")))?;
    }
    Ok(())
}

pub(super) fn validate_mcp_collection(collection: &str) -> Result<String, ErrorData> {
    let collection = collection.trim();
    axon_core::config::validate_collection_name(collection)
        .map_err(|reason| invalid_params(format!("invalid collection name: {reason}")))?;
    Ok(collection.to_string())
}

// --- Param parsers ---

pub(super) fn parse_job_id(job_id: Option<&str>) -> Result<Uuid, ErrorData> {
    let raw = job_id.ok_or_else(|| invalid_params("job_id is required for this subaction"))?;
    Uuid::parse_str(raw).map_err(|e| invalid_params(format!("invalid job_id: {e}")))
}

pub use transport::{pagination as to_pagination, retrieve_options as to_retrieve_options};

pub fn to_service_time_range(value: SearchTimeRange) -> ServiceTimeRange {
    match value {
        SearchTimeRange::Day => ServiceTimeRange::Day,
        SearchTimeRange::Week => ServiceTimeRange::Week,
        SearchTimeRange::Month => ServiceTimeRange::Month,
        SearchTimeRange::Year => ServiceTimeRange::Year,
    }
}

pub fn to_search_options(
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<SearchTimeRange>,
    default_limit: usize,
) -> axon_services::types::SearchOptions {
    transport::search_options(
        limit,
        offset,
        time_range.map(to_service_time_range),
        default_limit,
    )
}

pub(super) fn map_render_mode(mode: McpRenderMode) -> RenderMode {
    match mode {
        McpRenderMode::Http => RenderMode::Http,
        McpRenderMode::Chrome => RenderMode::Chrome,
        McpRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}

pub(super) fn apply_extract_overrides(cfg: &Config, req: &ExtractRequest) -> Config {
    transport::apply_extract_overrides(
        cfg,
        &transport::ExtractTransportOverrides {
            prompt: req.prompt.clone(),
            max_pages: req.max_pages,
            render_mode: req.render_mode.map(map_render_mode),
            embed: req.embed,
            collection: None,
            headers: Vec::new(),
        },
    )
}

// --- General-purpose slug utility (used by query handlers for artifact stems) ---

pub(super) fn slugify(value: &str, max_len: usize) -> String {
    let mut out = String::with_capacity(value.len().min(max_len));
    let mut prev_dash = false;
    for ch in value.chars() {
        let c = ch.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
        if out.len() >= max_len {
            break;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "artifact".to_string()
    } else if trimmed.len() < 4 {
        // Non-ASCII input degenerates to very short slugs — append a hash
        // of the original value so distinct inputs produce distinct stems.
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        value.hash(&mut hasher);
        let hash = hasher.finish();
        let combined = format!("{trimmed}-{hash:08x}");
        if combined.len() > max_len {
            combined[..max_len].to_string()
        } else {
            combined
        }
    } else {
        trimmed
    }
}

// --- Pagination helpers ---

/// Map MCP limit/offset params to service `MapOptions`.
/// `limit=0` (or `None`) means "no limit" — matches the CLI default.
/// Any positive value is honored as-is (no upper clamp) so callers can request large sets.
pub fn to_map_options(
    limit: Option<usize>,
    offset: Option<usize>,
) -> axon_services::types::MapOptions {
    transport::map_options(limit, offset)
}

#[cfg(test)]
#[path = "common_tests.rs"]
mod tests;
