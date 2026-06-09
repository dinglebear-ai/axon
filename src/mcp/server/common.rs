use crate::core::config::{Config, ConfigOverrides, RenderMode, ScrapeFormat};
use crate::mcp::schema::{CrawlRequest, McpRenderMode, McpScrapeFormat, SearchTimeRange};
use crate::services::types::{
    MapOptions, Pagination, RetrieveOptions, SearchOptions, ServiceTimeRange,
};
use rmcp::ErrorData;
use uuid::Uuid;

// Re-export artifact helpers that are imported by multiple handler modules.
pub(super) use super::artifacts::InlineHint;
pub(super) use super::artifacts::respond_with_mode;

pub(super) const MCP_TOOL_SCHEMA_URI: &str = "axon://schema/mcp-tool";

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
/// Only the error's own `Display` (`{e}`) is forwarded — NOT the `{e:#}` source
/// chain — so deeper internal details (DSNs, file paths, nested transport URLs)
/// stay in the server log. Top-level messages here are descriptive and safe
/// (e.g. "AXON_OPENAI_BASE_URL is required for ask", "Failed to retrieve any
/// context sources for ask", "LLM answer generation failed: ...").
pub(super) fn logged_internal_error(
    context: &str,
    e: &(dyn std::error::Error + 'static),
) -> ErrorData {
    // Walk the source chain explicitly: once the anyhow error is erased to a
    // `dyn Error` trait object, `{e:#}` no longer expands `.source()`, so build
    // the full chain by hand for the server log. The depth cap defends against a
    // pathological self-referential `source()` (would otherwise loop forever and
    // grow `chain` unbounded).
    let mut chain = e.to_string();
    let mut src = e.source();
    for _ in 0..16 {
        let Some(cause) = src else { break };
        chain.push_str(&format!(": {cause}"));
        src = cause.source();
    }
    tracing::error!("{context}: {chain}");
    ErrorData::internal_error(format!("{context} failed: {e}"), None)
}

// --- URL validation wrappers ---

/// Validate a single URL via the SSRF guard, converting errors to MCP `invalid_params`.
pub(super) fn validate_mcp_url(url: &str) -> Result<(), ErrorData> {
    crate::core::http::validate_url(url).map_err(|e| invalid_params(e.to_string()))
}

/// Validate every URL in a slice via the SSRF guard.
/// Error messages include the index so the client can identify which URL failed.
pub(super) fn validate_mcp_urls(urls: &[String]) -> Result<(), ErrorData> {
    for (i, url) in urls.iter().enumerate() {
        crate::core::http::validate_url(url)
            .map_err(|e| invalid_params(format!("urls[{i}]: {e}")))?;
    }
    Ok(())
}

pub(super) fn validate_mcp_collection(collection: &str) -> Result<String, ErrorData> {
    let collection = collection.trim();
    crate::core::config::validate_collection_name(collection)
        .map_err(|reason| invalid_params(format!("invalid collection name: {reason}")))?;
    Ok(collection.to_string())
}

pub(super) fn validate_mcp_embed_input_with_config(
    cfg: &Config,
    input: &str,
) -> Result<String, ErrorData> {
    crate::services::embed::validate_server_embed_input_with_config(cfg, input)
        .map_err(|err| invalid_params(err.to_string()))
}

// --- Param parsers ---

pub(super) fn parse_job_id(job_id: Option<&str>) -> Result<Uuid, ErrorData> {
    let raw = job_id.ok_or_else(|| invalid_params("job_id is required for this subaction"))?;
    Uuid::parse_str(raw).map_err(|e| invalid_params(format!("invalid job_id: {e}")))
}

pub(super) fn parse_limit(limit: Option<i64>, default: i64) -> i64 {
    limit.unwrap_or(default).clamp(1, 500)
}

pub(super) fn parse_offset(offset: Option<usize>) -> usize {
    offset.unwrap_or(0)
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

// --- Crawl config helpers ---

pub(super) fn apply_crawl_overrides(cfg: &Config, req: &CrawlRequest) -> Config {
    cfg.apply_overrides(&ConfigOverrides {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        include_subdomains: req.include_subdomains,
        respect_robots: req.respect_robots,
        discover_sitemaps: req.discover_sitemaps,
        sitemap_since_days: req.sitemap_since_days,
        discover_llms_txt: req.discover_llms_txt,
        max_llms_txt_urls: req.max_llms_txt_urls,
        render_mode: req.render_mode.map(map_render_mode),
        delay_ms: req.delay_ms,
        ..ConfigOverrides::default()
    })
}

pub(super) fn map_render_mode(mode: McpRenderMode) -> RenderMode {
    match mode {
        McpRenderMode::Http => RenderMode::Http,
        McpRenderMode::Chrome => RenderMode::Chrome,
        McpRenderMode::AutoSwitch => RenderMode::AutoSwitch,
    }
}

pub(super) fn map_scrape_format(f: McpScrapeFormat) -> ScrapeFormat {
    match f {
        McpScrapeFormat::Markdown => ScrapeFormat::Markdown,
        McpScrapeFormat::Html => ScrapeFormat::Html,
        McpScrapeFormat::RawHtml => ScrapeFormat::RawHtml,
        McpScrapeFormat::Json => ScrapeFormat::Json,
        McpScrapeFormat::Llm => ScrapeFormat::Llm,
    }
}

// --- Pagination helpers ---

/// Map MCP limit/offset params to service `Pagination`, clamping limit to [1, 500].
/// `default` is used when `limit` is `None`; callers should pass `cfg.search_limit`.
pub fn to_pagination(limit: Option<usize>, offset: Option<usize>, default: usize) -> Pagination {
    Pagination {
        limit: limit.unwrap_or(default).clamp(1, 500),
        offset: offset.unwrap_or(0),
    }
}

/// Map MCP limit/offset params to service `MapOptions`.
/// `limit=0` (or `None`) means "no limit" — matches the CLI default.
/// Any positive value is honored as-is (no upper clamp) so callers can request large sets.
pub fn to_map_options(limit: Option<usize>, offset: Option<usize>) -> MapOptions {
    MapOptions {
        limit: limit.unwrap_or(0),
        offset: offset.unwrap_or(0),
    }
}

/// Map MCP retrieve params to service `RetrieveOptions`.
pub fn to_retrieve_options(
    max_points: Option<usize>,
    cursor: Option<String>,
    token_budget: Option<usize>,
) -> RetrieveOptions {
    RetrieveOptions {
        max_points,
        cursor,
        token_budget: token_budget.map(|budget| budget.clamp(1, 50_000)),
    }
}

/// Map MCP `SearchTimeRange` enum to service `ServiceTimeRange`.
pub fn to_service_time_range(tr: SearchTimeRange) -> ServiceTimeRange {
    match tr {
        SearchTimeRange::Day => ServiceTimeRange::Day,
        SearchTimeRange::Week => ServiceTimeRange::Week,
        SearchTimeRange::Month => ServiceTimeRange::Month,
        SearchTimeRange::Year => ServiceTimeRange::Year,
    }
}

/// Map MCP search params to service `SearchOptions`.
/// `default` is used when `limit` is `None`; callers should pass `cfg.search_limit`.
pub fn to_search_options(
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<SearchTimeRange>,
    default: usize,
) -> SearchOptions {
    SearchOptions {
        limit: limit.unwrap_or(default).clamp(1, 500),
        offset: offset.unwrap_or(0),
        time_range: time_range.map(to_service_time_range),
    }
}

#[cfg(test)]
#[path = "common_tests.rs"]
mod tests;
