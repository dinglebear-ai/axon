use crate::crates::core::config::{Config, RenderMode, ScrapeFormat};
use crate::crates::mcp::schema::{CrawlRequest, McpRenderMode, McpScrapeFormat, SearchTimeRange};
use crate::crates::services::types::{
    MapOptions, Pagination, RetrieveOptions, SearchOptions, ServiceTimeRange,
};
use rmcp::ErrorData;
use uuid::Uuid;

// Re-export artifact helpers that are imported by multiple handler modules.
pub(super) use super::artifacts::respond_with_mode;

pub(super) const MCP_TOOL_SCHEMA_URI: &str = "axon://schema/mcp-tool";

// --- Error constructors ---

pub(super) fn invalid_params(msg: impl Into<String>) -> ErrorData {
    ErrorData::invalid_params(msg.into(), None)
}

pub(super) fn internal_error(msg: impl Into<String>) -> ErrorData {
    ErrorData::internal_error(msg.into(), None)
}

/// Log the raw error server-side and return a generic MCP error so internal
/// details (DSNs, file paths, stack traces) are never forwarded to clients.
pub(super) fn logged_internal_error(context: &str, e: impl std::fmt::Display) -> ErrorData {
    tracing::error!("{context}: {e}");
    internal_error(format!("{context} failed"))
}

// --- Param parsers ---

pub(super) fn parse_job_id(job_id: Option<&String>) -> Result<Uuid, ErrorData> {
    let raw = job_id.ok_or_else(|| invalid_params("job_id is required for this subaction"))?;
    Uuid::parse_str(raw).map_err(|e| invalid_params(format!("invalid job_id: {e}")))
}

pub(super) fn parse_limit(limit: Option<i64>, default: i64) -> i64 {
    limit.unwrap_or(default).clamp(1, 500)
}

pub(super) fn parse_limit_usize(limit: Option<usize>, default: usize, max: usize) -> usize {
    limit.unwrap_or(default).clamp(1, max)
}

pub(super) fn parse_offset(offset: Option<usize>) -> usize {
    offset.unwrap_or(0)
}

pub(super) fn paginate_vec<T: Clone>(items: &[T], offset: usize, limit: usize) -> Vec<T> {
    items.iter().skip(offset).take(limit).cloned().collect()
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
    } else {
        trimmed
    }
}

// --- Crawl config helpers ---

pub(super) fn apply_crawl_overrides(cfg: &Config, req: &CrawlRequest) -> Config {
    let mut out = cfg.clone();
    if let Some(max_pages) = req.max_pages {
        out.max_pages = max_pages;
    }
    if let Some(max_depth) = req.max_depth {
        out.max_depth = max_depth;
    }
    if let Some(include_subdomains) = req.include_subdomains {
        out.include_subdomains = include_subdomains;
    }
    if let Some(respect_robots) = req.respect_robots {
        out.respect_robots = respect_robots;
    }
    if let Some(discover_sitemaps) = req.discover_sitemaps {
        out.discover_sitemaps = discover_sitemaps;
    }
    if let Some(sitemap_since_days) = req.sitemap_since_days {
        out.sitemap_since_days = sitemap_since_days;
    }
    if let Some(render_mode) = req.render_mode {
        out.render_mode = map_render_mode(render_mode);
    }
    if let Some(delay_ms) = req.delay_ms {
        out.delay_ms = delay_ms;
    }
    out
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

/// Map MCP `RetrieveOptions` (max_points field) to service `RetrieveOptions`.
pub fn to_retrieve_options(max_points: Option<usize>) -> RetrieveOptions {
    RetrieveOptions { max_points }
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
