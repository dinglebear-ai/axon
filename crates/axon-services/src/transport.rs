//! Shared transport request policy for CLI, MCP, and HTTP adapters.
//!
//! Transport layers should map wire requests into these helpers and leave
//! default/cap behavior here so presentation surfaces cannot drift.

use crate::types::{MapOptions, Pagination, RetrieveOptions, SearchOptions, ServiceTimeRange};
use axon_core::config::{Config, ConfigOverrides, RenderMode};

pub const PAGE_LIMIT_MAX: usize = 500;
pub const DISCOVERY_PAGE_DEFAULT: usize = 25;
pub const DOMAIN_SOURCES_PAGE_MAX: usize = 10_000;
pub const RETRIEVE_TOKEN_BUDGET_MAX: usize = 50_000;
pub const JOB_LIST_DEFAULT: i64 = 20;
pub const JOB_LIST_MAX: i64 = 500;

pub fn pagination(limit: Option<usize>, offset: Option<usize>, default: usize) -> Pagination {
    pagination_with_max(limit, offset, default, PAGE_LIMIT_MAX)
}

pub fn pagination_with_max(
    limit: Option<usize>,
    offset: Option<usize>,
    default: usize,
    max: usize,
) -> Pagination {
    Pagination {
        limit: limit.unwrap_or(default).clamp(1, max),
        offset: offset.unwrap_or(0),
    }
}

pub fn discovery_pagination(limit: Option<usize>, offset: Option<usize>) -> Pagination {
    pagination(limit, offset, DISCOVERY_PAGE_DEFAULT)
}

pub fn domain_sources_pagination(limit: Option<usize>, offset: Option<usize>) -> Pagination {
    pagination_with_max(
        limit,
        offset,
        DISCOVERY_PAGE_DEFAULT,
        DOMAIN_SOURCES_PAGE_MAX,
    )
}

pub fn job_list_pagination(limit: Option<i64>, offset: Option<usize>) -> (i64, i64) {
    let limit = limit.unwrap_or(JOB_LIST_DEFAULT).clamp(1, JOB_LIST_MAX);
    let offset = offset.unwrap_or(0).min(i64::MAX as usize) as i64;
    (limit, offset)
}

pub fn job_list_pagination_signed(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    let limit = limit.unwrap_or(JOB_LIST_DEFAULT).clamp(1, JOB_LIST_MAX);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

pub fn map_options(limit: Option<usize>, offset: Option<usize>) -> MapOptions {
    MapOptions {
        // 0 means unbounded for map and matches the CLI's omitted-limit behavior.
        limit: limit.unwrap_or(0),
        offset: offset.unwrap_or(0),
    }
}

pub fn retrieve_options(
    max_points: Option<usize>,
    cursor: Option<String>,
    token_budget: Option<usize>,
) -> RetrieveOptions {
    RetrieveOptions {
        max_points,
        cursor,
        token_budget: token_budget.map(|budget| budget.clamp(1, RETRIEVE_TOKEN_BUDGET_MAX)),
    }
}

pub fn search_options(
    limit: Option<usize>,
    offset: Option<usize>,
    time_range: Option<ServiceTimeRange>,
    default_limit: usize,
) -> SearchOptions {
    SearchOptions {
        limit: limit.unwrap_or(default_limit).clamp(1, PAGE_LIMIT_MAX),
        offset: offset.unwrap_or(0),
        time_range,
    }
}

pub fn parse_service_time_range(value: &str) -> Result<ServiceTimeRange, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "day" => Ok(ServiceTimeRange::Day),
        "week" => Ok(ServiceTimeRange::Week),
        "month" => Ok(ServiceTimeRange::Month),
        "year" => Ok(ServiceTimeRange::Year),
        other => Err(format!(
            "invalid time_range: {other}; expected day|week|month|year"
        )),
    }
}

#[derive(Debug, Clone, Default)]
pub struct CrawlTransportOverrides {
    pub max_pages: Option<u32>,
    pub max_depth: Option<usize>,
    pub include_subdomains: Option<bool>,
    pub respect_robots: Option<bool>,
    pub discover_sitemaps: Option<bool>,
    pub max_sitemaps: Option<usize>,
    pub sitemap_since_days: Option<u32>,
    pub discover_llms_txt: Option<bool>,
    pub max_llms_txt_urls: Option<usize>,
    pub render_mode: Option<RenderMode>,
    pub delay_ms: Option<u64>,
    pub collection: Option<String>,
    pub headers: Vec<String>,
}

pub fn apply_crawl_overrides(cfg: &Config, req: &CrawlTransportOverrides) -> Config {
    let mut cfg = cfg.apply_overrides(&ConfigOverrides {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        include_subdomains: req.include_subdomains,
        respect_robots: req.respect_robots,
        discover_sitemaps: req.discover_sitemaps,
        max_sitemaps: req.max_sitemaps,
        sitemap_since_days: req.sitemap_since_days,
        discover_llms_txt: req.discover_llms_txt,
        max_llms_txt_urls: req.max_llms_txt_urls,
        render_mode: req.render_mode,
        delay_ms: req.delay_ms,
        collection: req.collection.clone(),
        ..ConfigOverrides::default()
    });
    if !req.headers.is_empty() {
        cfg.custom_headers = req.headers.clone();
    }
    cfg
}

#[derive(Debug, Clone, Default)]
pub struct ExtractTransportOverrides {
    pub prompt: Option<String>,
    pub max_pages: Option<u32>,
    pub render_mode: Option<RenderMode>,
    pub embed: Option<bool>,
    pub collection: Option<String>,
    pub headers: Vec<String>,
}

pub fn apply_extract_overrides(cfg: &Config, req: &ExtractTransportOverrides) -> Config {
    let mut cfg = cfg.apply_overrides(&ConfigOverrides {
        query: Some(req.prompt.clone()),
        max_pages: req.max_pages,
        render_mode: req.render_mode,
        embed: req.embed,
        collection: req.collection.clone(),
        ..ConfigOverrides::default()
    });
    if !req.headers.is_empty() {
        cfg.custom_headers = req.headers.clone();
    }
    cfg
}

#[derive(Debug, Clone, Default)]
pub struct AskTransportOverrides {
    pub collection: Option<String>,
    pub since: Option<String>,
    pub before: Option<String>,
    pub diagnostics: Option<bool>,
    pub explain: Option<bool>,
    pub hybrid_search: Option<bool>,
    pub ask_chunk_limit: Option<usize>,
    pub ask_full_docs: Option<usize>,
    pub ask_max_context_chars: Option<usize>,
    pub ask_hybrid_candidates: Option<usize>,
    pub ask_min_relevance_score: Option<f64>,
    pub ask_doc_chunk_limit: Option<usize>,
    pub ask_doc_fetch_concurrency: Option<usize>,
    pub ask_backfill_chunks: Option<usize>,
    pub ask_candidate_limit: Option<usize>,
    pub ask_min_citations_nontrivial: Option<usize>,
    pub ask_authoritative_domains: Option<Vec<String>>,
    pub ask_authoritative_boost: Option<f64>,
}

pub fn apply_ask_overrides(cfg: &Config, req: AskTransportOverrides) -> Config {
    let mut cfg = cfg.clone();
    if let Some(c) = req.collection {
        cfg.collection = c;
    }
    if let Some(s) = req.since {
        cfg.since = Some(s);
    }
    if let Some(b) = req.before {
        cfg.before = Some(b);
    }
    if let Some(d) = req.diagnostics {
        cfg.ask_diagnostics = d;
    }
    if let Some(explain) = req.explain {
        cfg.ask_explain = explain;
        if explain {
            cfg.ask_diagnostics = true;
        }
    }
    if let Some(h) = req.hybrid_search {
        cfg.hybrid_search_enabled = h;
    }
    if let Some(v) = req.ask_chunk_limit {
        cfg.ask_chunk_limit = v.clamp(3, 40);
    }
    if let Some(v) = req.ask_full_docs {
        cfg.ask_full_docs = v.clamp(1, 20);
        cfg.ask_full_docs_explicit = true;
    }
    if let Some(v) = req.ask_max_context_chars {
        cfg.ask_max_context_chars = v.clamp(20_000, 1_000_000);
    }
    if let Some(v) = req.ask_hybrid_candidates {
        cfg.ask_hybrid_candidates = v.clamp(10, 500);
    }
    if let Some(v) = req.ask_min_relevance_score {
        cfg.ask_min_relevance_score = v.clamp(-1.0, 2.0);
    }
    if let Some(v) = req.ask_doc_chunk_limit {
        cfg.ask_doc_chunk_limit = v.clamp(8, 2000);
    }
    if let Some(v) = req.ask_doc_fetch_concurrency {
        cfg.ask_doc_fetch_concurrency = v.clamp(1, 16);
    }
    if let Some(v) = req.ask_backfill_chunks {
        cfg.ask_backfill_chunks = v.clamp(0, 20);
    }
    if let Some(v) = req.ask_candidate_limit {
        cfg.ask_candidate_limit = v.clamp(8, 300);
    }
    if let Some(v) = req.ask_min_citations_nontrivial {
        cfg.ask_min_citations_nontrivial = v.clamp(1, 5);
    }
    if let Some(v) = req.ask_authoritative_domains {
        cfg.ask_authoritative_domains = v;
    }
    if let Some(v) = req.ask_authoritative_boost {
        cfg.ask_authoritative_boost = v.clamp(0.0, 0.5);
    }
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_omitted_limit_is_unbounded_across_transports() {
        assert_eq!(map_options(None, None).limit, 0);
        assert_eq!(map_options(Some(100), Some(2)).offset, 2);
    }

    #[test]
    fn search_defaults_use_caller_config_and_shared_cap() {
        let opts = search_options(None, None, None, 37);
        assert_eq!(opts.limit, 37);
        let opts = search_options(Some(10_000), None, Some(ServiceTimeRange::Week), 37);
        assert_eq!(opts.limit, PAGE_LIMIT_MAX);
        assert_eq!(opts.time_range, Some(ServiceTimeRange::Week));
    }

    #[test]
    fn discovery_defaults_are_single_source() {
        assert_eq!(
            discovery_pagination(None, None).limit,
            DISCOVERY_PAGE_DEFAULT
        );
        assert_eq!(
            discovery_pagination(Some(10_000), None).limit,
            PAGE_LIMIT_MAX
        );
        assert_eq!(
            domain_sources_pagination(Some(10_000), None).limit,
            DOMAIN_SOURCES_PAGE_MAX
        );
    }

    #[test]
    fn retrieve_token_budget_uses_shared_cap() {
        assert_eq!(
            retrieve_options(None, None, Some(usize::MAX)).token_budget,
            Some(RETRIEVE_TOKEN_BUDGET_MAX)
        );
    }

    #[test]
    fn job_list_uses_shared_default_and_cap() {
        assert_eq!(job_list_pagination(None, None), (JOB_LIST_DEFAULT, 0));
        assert_eq!(
            job_list_pagination(Some(10_000), Some(3)),
            (JOB_LIST_MAX, 3)
        );
        assert_eq!(
            job_list_pagination_signed(None, Some(-3)),
            (JOB_LIST_DEFAULT, 0)
        );
    }

    #[test]
    fn ask_overrides_apply_shared_clamps() {
        let cfg = Config::default_minimal();
        let cfg = apply_ask_overrides(
            &cfg,
            AskTransportOverrides {
                ask_full_docs: Some(999),
                ask_authoritative_boost: Some(10.0),
                explain: Some(true),
                ..AskTransportOverrides::default()
            },
        );
        assert_eq!(cfg.ask_full_docs, 20);
        assert_eq!(cfg.ask_authoritative_boost, 0.5);
        assert!(cfg.ask_explain);
        assert!(cfg.ask_diagnostics);
    }

    #[test]
    fn extract_overrides_include_render_mode_embed_and_collection() {
        let cfg = Config::default_minimal();
        let req = ExtractTransportOverrides {
            render_mode: Some(RenderMode::Chrome),
            embed: Some(false),
            collection: Some("review-collection".to_string()),
            prompt: Some("extract prices".to_string()),
            max_pages: Some(17),
            headers: vec!["X-Test: 1".to_string()],
        };

        let cfg = apply_extract_overrides(&cfg, &req);

        assert_eq!(cfg.render_mode, RenderMode::Chrome);
        assert!(!cfg.embed);
        assert_eq!(cfg.collection, "review-collection");
        assert_eq!(cfg.query.as_deref(), Some("extract prices"));
        assert_eq!(cfg.max_pages, 17);
        assert_eq!(cfg.custom_headers, vec!["X-Test: 1".to_string()]);
    }

    #[test]
    fn crawl_overrides_include_rest_collection_and_headers() {
        let cfg = Config::default_minimal();
        let req = CrawlTransportOverrides {
            max_pages: Some(17),
            render_mode: Some(RenderMode::Chrome),
            collection: Some("crawl-review".to_string()),
            headers: vec!["X-Test: 1".to_string()],
            ..CrawlTransportOverrides::default()
        };

        let cfg = apply_crawl_overrides(&cfg, &req);

        assert_eq!(cfg.max_pages, 17);
        assert_eq!(cfg.render_mode, RenderMode::Chrome);
        assert_eq!(cfg.collection, "crawl-review");
        assert_eq!(cfg.custom_headers, vec!["X-Test: 1".to_string()]);
    }
}
