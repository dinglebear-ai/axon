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
mod tests {
    use super::*;

    #[test]
    fn apply_overrides_sets_collection() {
        let base = Config::default();
        let cfg = base.apply_overrides(&ConfigOverrides {
            collection: Some("custom-col".to_string()),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.collection, "custom-col");
    }

    #[test]
    fn apply_overrides_leaves_unchanged_fields() {
        let base = Config::default();
        let original_depth = base.max_depth;
        let cfg = base.apply_overrides(&ConfigOverrides {
            collection: Some("custom-col".to_string()),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.max_depth, original_depth);
    }

    #[test]
    fn apply_overrides_does_not_mutate_base() {
        let base = Config::default();
        let original_collection = base.collection.clone();
        let _cfg = base.apply_overrides(&ConfigOverrides {
            collection: Some("custom-col".to_string()),
            ..ConfigOverrides::default()
        });
        // base must be unchanged
        assert_eq!(base.collection, original_collection);
    }

    #[test]
    fn apply_overrides_sets_limit() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            limit: Some(25),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.search_limit, 25);
    }

    #[test]
    fn apply_overrides_sets_render_mode() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            render_mode: Some(RenderMode::Chrome),
            ..ConfigOverrides::default()
        });
        assert!(matches!(cfg.render_mode, RenderMode::Chrome));
    }

    #[test]
    fn apply_overrides_sets_scrape_fields() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            format: Some(ScrapeFormat::Html),
            root_selector: Some("main".to_string()),
            exclude_selector: Some("nav".to_string()),
            ..ConfigOverrides::default()
        });
        assert!(matches!(cfg.format, ScrapeFormat::Html));
        assert_eq!(cfg.root_selector.as_deref(), Some("main"));
        assert_eq!(cfg.exclude_selector.as_deref(), Some("nav"));
    }

    #[test]
    fn apply_overrides_sets_query_filters_and_hybrid() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            since: Some("7d".to_string()),
            before: Some("2026-05-07".to_string()),
            hybrid_search_enabled: Some(false),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.since.as_deref(), Some("7d"));
        assert_eq!(cfg.before.as_deref(), Some("2026-05-07"));
        assert!(!cfg.hybrid_search_enabled);
    }

    #[test]
    fn apply_overrides_sets_ask_flags() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            ask_graph: Some(true),
            ask_diagnostics: Some(true),
            ..ConfigOverrides::default()
        });
        assert!(cfg.ask_graph);
        assert!(cfg.ask_diagnostics);
    }

    #[test]
    fn apply_overrides_sets_query_even_to_none() {
        let base = Config {
            query: Some("existing".to_string()),
            ..Config::default()
        };
        let cfg = base.apply_overrides(&ConfigOverrides {
            query: Some(None),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.query, None);
    }

    #[test]
    fn apply_overrides_sets_screenshot_fields() {
        let output = PathBuf::from("/tmp/axon-shot.png");
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            viewport_width: Some(800),
            viewport_height: Some(600),
            screenshot_full_page: Some(false),
            output_path: Some(Some(output.clone())),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.viewport_width, 800);
        assert_eq!(cfg.viewport_height, 600);
        assert!(!cfg.screenshot_full_page);
        assert_eq!(cfg.output_path, Some(output));
    }

    #[test]
    fn apply_overrides_sets_wait() {
        let base = Config::default();
        assert!(!base.wait);
        let cfg = base.apply_overrides(&ConfigOverrides {
            wait: Some(true),
            ..ConfigOverrides::default()
        });
        assert!(cfg.wait);
    }

    #[test]
    fn apply_overrides_sets_embed_false() {
        let base = Config::default();
        assert!(base.embed);
        let cfg = base.apply_overrides(&ConfigOverrides {
            embed: Some(false),
            ..ConfigOverrides::default()
        });
        assert!(!cfg.embed);
    }

    #[test]
    fn apply_overrides_sets_include_subdomains() {
        let base = Config::default();
        assert!(!base.include_subdomains);
        let cfg = base.apply_overrides(&ConfigOverrides {
            include_subdomains: Some(true),
            ..ConfigOverrides::default()
        });
        assert!(cfg.include_subdomains);
    }

    #[test]
    fn apply_overrides_sets_respect_robots() {
        let base = Config::default();
        assert!(!base.respect_robots);
        let cfg = base.apply_overrides(&ConfigOverrides {
            respect_robots: Some(true),
            ..ConfigOverrides::default()
        });
        assert!(cfg.respect_robots);
    }

    #[test]
    fn apply_overrides_sets_discover_sitemaps_false() {
        let base = Config::default();
        assert!(base.discover_sitemaps);
        let cfg = base.apply_overrides(&ConfigOverrides {
            discover_sitemaps: Some(false),
            ..ConfigOverrides::default()
        });
        assert!(!cfg.discover_sitemaps);
    }

    #[test]
    fn apply_overrides_sets_sitemap_since_days() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            sitemap_since_days: Some(7),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.sitemap_since_days, 7);
    }

    #[test]
    fn apply_overrides_sets_delay_ms() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            delay_ms: Some(500),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.delay_ms, 500);
    }

    #[test]
    fn apply_overrides_sets_min_markdown_chars() {
        let cfg = Config::default().apply_overrides(&ConfigOverrides {
            min_markdown_chars: Some(500),
            ..ConfigOverrides::default()
        });
        assert_eq!(cfg.min_markdown_chars, 500);
    }

    #[test]
    fn apply_overrides_sets_drop_thin_markdown_false() {
        let base = Config::default();
        assert!(base.drop_thin_markdown);
        let cfg = base.apply_overrides(&ConfigOverrides {
            drop_thin_markdown: Some(false),
            ..ConfigOverrides::default()
        });
        assert!(!cfg.drop_thin_markdown);
    }

    #[test]
    fn apply_overrides_all_none_is_noop() {
        let base = Config::default();
        let cfg = base.apply_overrides(&ConfigOverrides::default());
        // Spot-check key fields are unchanged
        assert_eq!(cfg.collection, base.collection);
        assert_eq!(cfg.max_depth, base.max_depth);
        assert_eq!(cfg.search_limit, base.search_limit);
        assert_eq!(cfg.wait, base.wait);
        assert_eq!(cfg.embed, base.embed);
        assert_eq!(cfg.include_subdomains, base.include_subdomains);
        assert_eq!(cfg.respect_robots, base.respect_robots);
        assert_eq!(cfg.discover_sitemaps, base.discover_sitemaps);
        assert_eq!(cfg.delay_ms, base.delay_ms);
    }
}
