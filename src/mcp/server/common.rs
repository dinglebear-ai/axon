use crate::core::config::{Config, ConfigOverrides, RenderMode, ScrapeFormat};
use crate::mcp::schema::{CrawlRequest, McpRenderMode, McpScrapeFormat, SearchTimeRange};
use crate::services::types::{
    MapOptions, Pagination, RetrieveOptions, SearchOptions, ServiceTimeRange,
};
use rmcp::ErrorData;
use std::path::{Path, PathBuf};
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

/// Log the raw error server-side and return a generic MCP error so internal
/// details (DSNs, file paths, stack traces) are never forwarded to clients.
pub(super) fn logged_internal_error(
    context: &str,
    e: &(dyn std::error::Error + 'static),
) -> ErrorData {
    tracing::error!("{context}: {e}");
    ErrorData::internal_error(format!("{context} failed"), None)
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

const MCP_EMBED_ALLOWED_ROOTS_ENV: &str = "AXON_MCP_EMBED_ALLOWED_ROOTS";
const MCP_EMBED_MAX_LOCAL_BYTES_ENV: &str = "AXON_MCP_EMBED_MAX_LOCAL_BYTES";
const DEFAULT_MCP_EMBED_MAX_LOCAL_BYTES: u64 = 10 * 1024 * 1024;

pub(super) fn validate_mcp_embed_input(input: &str) -> Result<String, ErrorData> {
    validate_mcp_embed_input_with_roots(
        input,
        &mcp_embed_allowed_roots_from_env(),
        mcp_embed_max_local_bytes_from_env(),
    )
}

fn mcp_embed_allowed_roots_from_env() -> Vec<PathBuf> {
    std::env::var(MCP_EMBED_ALLOWED_ROOTS_ENV)
        .ok()
        .map(|raw| {
            raw.split(',')
                .filter_map(|part| {
                    let trimmed = part.trim();
                    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn mcp_embed_max_local_bytes_from_env() -> u64 {
    std::env::var(MCP_EMBED_MAX_LOCAL_BYTES_ENV)
        .ok()
        .and_then(|raw| raw.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MCP_EMBED_MAX_LOCAL_BYTES)
}

fn validate_mcp_embed_input_with_roots(
    input: &str,
    allowed_roots: &[PathBuf],
    max_file_bytes: u64,
) -> Result<String, ErrorData> {
    let input = input.trim();
    if input.is_empty() {
        return Err(invalid_params("input is required for embed.start"));
    }
    if input.starts_with("http://") || input.starts_with("https://") {
        validate_mcp_url(input)?;
        return Ok(input.to_string());
    }
    let path = Path::new(input);
    if !path.exists() {
        return Ok(input.to_string());
    }
    if allowed_roots.is_empty() {
        return Err(invalid_params(format!(
            "local file embedding via MCP is disabled; set {MCP_EMBED_ALLOWED_ROOTS_ENV} to allow specific roots"
        )));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|e| invalid_params(format!("invalid embed path: {e}")))?;
    let root = allowed_roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .find(|root| canonical.starts_with(root))
        .ok_or_else(|| {
            invalid_params(format!(
                "local embed path must be under one of {MCP_EMBED_ALLOWED_ROOTS_ENV}"
            ))
        })?;
    validate_local_embed_entry(path, &canonical, &root, max_file_bytes)?;
    Ok(canonical.to_string_lossy().to_string())
}

fn validate_local_embed_entry(
    original: &Path,
    canonical: &Path,
    allowed_root: &Path,
    max_file_bytes: u64,
) -> Result<(), ErrorData> {
    if std::fs::symlink_metadata(original)
        .map_err(|e| invalid_params(format!("invalid embed path metadata: {e}")))?
        .file_type()
        .is_symlink()
    {
        return Err(invalid_params("local embed path must not be a symlink"));
    }
    validate_local_embed_relative_path(canonical, allowed_root)?;
    let meta = std::fs::metadata(canonical)
        .map_err(|e| invalid_params(format!("invalid embed path metadata: {e}")))?;
    if meta.is_file() {
        return validate_local_embed_file(canonical, allowed_root, meta.len(), max_file_bytes);
    }
    if meta.is_dir() {
        for entry in std::fs::read_dir(canonical)
            .map_err(|e| invalid_params(format!("invalid embed directory: {e}")))?
        {
            let entry = entry.map_err(|e| invalid_params(format!("invalid embed entry: {e}")))?;
            let child = entry.path();
            let child_meta = std::fs::symlink_metadata(&child)
                .map_err(|e| invalid_params(format!("invalid embed entry metadata: {e}")))?;
            if child_meta.file_type().is_symlink() {
                return Err(invalid_params(
                    "local embed directory must not contain symlinks",
                ));
            }
            if child_meta.is_file() {
                let child_canonical = std::fs::canonicalize(&child)
                    .map_err(|e| invalid_params(format!("invalid embed path: {e}")))?;
                validate_local_embed_file(
                    &child_canonical,
                    allowed_root,
                    child_meta.len(),
                    max_file_bytes,
                )?;
            }
        }
        return Ok(());
    }
    Err(invalid_params(
        "local embed path must be a regular file or directory",
    ))
}

fn validate_local_embed_file(
    canonical: &Path,
    allowed_root: &Path,
    size: u64,
    max_file_bytes: u64,
) -> Result<(), ErrorData> {
    validate_local_embed_relative_path(canonical, allowed_root)?;
    if size > max_file_bytes {
        return Err(invalid_params(format!(
            "local embed file exceeds {max_file_bytes} byte limit"
        )));
    }
    Ok(())
}

fn validate_local_embed_relative_path(
    canonical: &Path,
    allowed_root: &Path,
) -> Result<(), ErrorData> {
    let relative = canonical
        .strip_prefix(allowed_root)
        .map_err(|_| invalid_params("local embed path is outside the allowed root"))?;
    for component in relative.components() {
        let name = component.as_os_str().to_string_lossy();
        let lower = name.to_ascii_lowercase();
        if name.starts_with('.') {
            return Err(invalid_params("local embed path must not include dotfiles"));
        }
        if lower == "id_rsa"
            || lower == "id_dsa"
            || lower == "id_ecdsa"
            || lower == "id_ed25519"
            || lower.ends_with(".pem")
            || lower.ends_with(".key")
            || lower.contains("secret")
            || lower.contains("credential")
            || lower.contains("token")
        {
            return Err(invalid_params(
                "local embed path appears to contain secret material",
            ));
        }
    }
    Ok(())
}

// --- Param parsers ---

pub(super) fn parse_job_id(job_id: Option<&str>) -> Result<Uuid, ErrorData> {
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
