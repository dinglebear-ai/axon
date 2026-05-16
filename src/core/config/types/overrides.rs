use super::config::Config;
use super::enums::{RenderMode, ScrapeFormat};
use std::path::PathBuf;

/// Fields that can be overridden per-request, used by both MCP and CLI paths.
///
/// `ConfigOverrides` captures a sparse set of field overrides. Apply them to an
/// existing `Config` with [`Config::apply_overrides`]. Fields set to `None` are
/// left unchanged.
///
/// # Example
///
/// ```rust,ignore
/// let mut cfg = Config::default();
/// let overrides = ConfigOverrides {
///     collection: Some("my-collection".to_string()),
///     max_pages: Some(100),
///     ..ConfigOverrides::default()
/// };
/// cfg.apply_overrides(&overrides);
/// assert_eq!(cfg.collection, "my-collection");
/// assert_eq!(cfg.max_pages, 100);
/// ```
#[derive(Debug, Default, Clone)]
pub struct ConfigOverrides {
    /// Override `Config::max_pages` (0 = uncapped).
    pub max_pages: Option<u32>,

    /// Override `Config::max_depth`.
    pub max_depth: Option<usize>,

    /// Override `Config::collection` (Qdrant collection name).
    pub collection: Option<String>,

    /// Override `Config::search_limit` (result count for query/search commands).
    pub limit: Option<usize>,

    /// Override `Config::embed` (auto-embed after scrape/crawl).
    pub embed: Option<bool>,

    /// Override `Config::query`.
    pub query: Option<Option<String>>,

    /// Override `Config::render_mode` (http / chrome / auto-switch).
    pub render_mode: Option<RenderMode>,

    /// Override `Config::format` (markdown / html / rawHtml / json).
    pub format: Option<ScrapeFormat>,

    /// Override `Config::root_selector`.
    pub root_selector: Option<String>,

    /// Override `Config::exclude_selector`.
    pub exclude_selector: Option<String>,

    /// Override `Config::since` (`scraped_at` lower-bound filter).
    pub since: Option<String>,

    /// Override `Config::before` (`scraped_at` upper-bound filter).
    pub before: Option<String>,

    /// Override `Config::hybrid_search_enabled`.
    pub hybrid_search_enabled: Option<bool>,

    /// Override `Config::ask_graph`.
    pub ask_graph: Option<bool>,

    /// Override `Config::ask_diagnostics`.
    pub ask_diagnostics: Option<bool>,

    /// Override `Config::ask_explain`.
    pub ask_explain: Option<bool>,

    /// Override `Config::viewport_width`.
    pub viewport_width: Option<u32>,

    /// Override `Config::viewport_height`.
    pub viewport_height: Option<u32>,

    /// Override `Config::screenshot_full_page`.
    pub screenshot_full_page: Option<bool>,

    /// Override `Config::output_path`.
    pub output_path: Option<Option<PathBuf>>,

    /// Override `Config::include_subdomains`.
    pub include_subdomains: Option<bool>,

    /// Override `Config::wait` (block until async jobs complete).
    pub wait: Option<bool>,

    /// Override `Config::respect_robots`.
    pub respect_robots: Option<bool>,

    /// Override `Config::discover_sitemaps`.
    pub discover_sitemaps: Option<bool>,

    /// Override `Config::sitemap_since_days`.
    pub sitemap_since_days: Option<u32>,

    /// Override `Config::delay_ms` (inter-request delay for polite crawling).
    pub delay_ms: Option<u64>,

    /// Override `Config::min_markdown_chars` (thin-page threshold).
    pub min_markdown_chars: Option<usize>,

    /// Override `Config::drop_thin_markdown` (skip thin pages entirely).
    pub drop_thin_markdown: Option<bool>,
}

impl Config {
    /// Apply per-request field overrides and return a new `Config`.
    ///
    /// Each `Some(v)` in `overrides` replaces the corresponding field in the
    /// returned copy. Fields set to `None` are left unchanged. The receiver is
    /// not modified — callers get an independent, fully-configured `Config`
    /// value that can be passed to a handler without affecting the shared base.
    ///
    /// This is the canonical way for MCP handler code and CLI sub-commands to
    /// layer per-call options on top of a shared base `Config`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let cfg = Config::default().apply_overrides(&ConfigOverrides {
    ///     collection: Some("my-collection".to_string()),
    ///     max_pages: Some(100),
    ///     ..ConfigOverrides::default()
    /// });
    /// assert_eq!(cfg.collection, "my-collection");
    /// ```
    pub fn apply_overrides(&self, overrides: &ConfigOverrides) -> Config {
        let mut cfg = self.clone();
        if let Some(v) = overrides.max_pages {
            cfg.max_pages = v;
        }
        if let Some(v) = overrides.max_depth {
            cfg.max_depth = v;
        }
        if let Some(ref v) = overrides.collection {
            cfg.collection = v.clone();
        }
        if let Some(v) = overrides.limit {
            cfg.search_limit = v;
        }
        if let Some(v) = overrides.embed {
            cfg.embed = v;
        }
        if let Some(ref v) = overrides.query {
            cfg.query = v.clone();
        }
        if let Some(v) = overrides.render_mode {
            cfg.render_mode = v;
        }
        if let Some(v) = overrides.format {
            cfg.format = v;
        }
        if let Some(ref v) = overrides.root_selector {
            cfg.root_selector = Some(v.clone());
        }
        if let Some(ref v) = overrides.exclude_selector {
            cfg.exclude_selector = Some(v.clone());
        }
        if let Some(ref v) = overrides.since {
            cfg.since = Some(v.clone());
        }
        if let Some(ref v) = overrides.before {
            cfg.before = Some(v.clone());
        }
        if let Some(v) = overrides.hybrid_search_enabled {
            cfg.hybrid_search_enabled = v;
        }
        if let Some(v) = overrides.ask_graph {
            cfg.ask_graph = v;
        }
        if let Some(v) = overrides.ask_diagnostics {
            cfg.ask_diagnostics = v;
        }
        if let Some(v) = overrides.ask_explain {
            cfg.ask_explain = v;
            if v {
                cfg.ask_diagnostics = true;
            }
        }
        if let Some(v) = overrides.viewport_width {
            cfg.viewport_width = v;
        }
        if let Some(v) = overrides.viewport_height {
            cfg.viewport_height = v;
        }
        if let Some(v) = overrides.screenshot_full_page {
            cfg.screenshot_full_page = v;
        }
        if let Some(ref v) = overrides.output_path {
            cfg.output_path = v.clone();
        }
        if let Some(v) = overrides.include_subdomains {
            cfg.include_subdomains = v;
        }
        if let Some(v) = overrides.wait {
            cfg.wait = v;
        }
        if let Some(v) = overrides.respect_robots {
            cfg.respect_robots = v;
        }
        if let Some(v) = overrides.discover_sitemaps {
            cfg.discover_sitemaps = v;
        }
        if let Some(v) = overrides.sitemap_since_days {
            cfg.sitemap_since_days = v;
        }
        if let Some(v) = overrides.delay_ms {
            cfg.delay_ms = v;
        }
        if let Some(v) = overrides.min_markdown_chars {
            cfg.min_markdown_chars = v;
        }
        if let Some(v) = overrides.drop_thin_markdown {
            cfg.drop_thin_markdown = v;
        }
        cfg
    }
}

#[cfg(test)]
#[path = "overrides_tests.rs"]
mod tests;
