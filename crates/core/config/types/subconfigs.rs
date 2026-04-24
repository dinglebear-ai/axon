/// Sub-config structs for eventual Config decomposition.
///
/// These structs are scaffolded here as part of A-H-01. The `Config` struct
/// currently holds 90+ fields as a flat god-object. The intent is to migrate
/// call sites to use these sub-configs one group at a time, starting with
/// `IngestConfig` (fewest cross-cutting dependencies), then `AskConfig`,
/// then `ServiceUrls`.
///
/// Migration is tracked in: `docs/config-decomposition-plan.md`
///
/// DO NOT add these as fields to `Config` yet — that migration touches hundreds
/// of call sites and must be done sequentially after other agents merge. This
/// file defines the target shapes so the team can agree on them first.
use std::fmt;

/// Connection URLs and API credentials for all external services.
///
/// `Debug` is implemented manually to redact all secret fields. Do not add
/// `#[derive(Debug)]` — that would print `openai_api_key`, `tavily_api_key`,
/// and connection URLs containing passwords in plaintext.
///
/// TODO(A-M-07): Wrap `openai_api_key`, `tavily_api_key`, and `github_token`
/// with `Secret<String>` after migration is complete.
#[allow(dead_code)]
#[derive(Clone, Default)]
pub struct ServiceUrls {
    pub pg_url: String,
    pub redis_url: String,
    pub amqp_url: String,
    pub qdrant_url: String,
    pub tei_url: String,
    pub openai_base_url: String,
    pub openai_api_key: String, // TODO(A-M-07): Secret<String>
    pub openai_model: String,
    pub tavily_api_key: String, // TODO(A-M-07): Secret<String>
}

impl fmt::Debug for ServiceUrls {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceUrls")
            .field("pg_url", &"[REDACTED]")
            .field("redis_url", &"[REDACTED]")
            .field("amqp_url", &"[REDACTED]")
            .field("qdrant_url", &self.qdrant_url)
            .field("tei_url", &self.tei_url)
            .field("openai_base_url", &self.openai_base_url)
            .field("openai_api_key", &"[REDACTED]")
            .field("openai_model", &self.openai_model)
            .field("tavily_api_key", &"[REDACTED]")
            .finish()
    }
}

/// Configuration for the GitHub/Reddit/YouTube ingest commands.
///
/// Default values match [`Config::default()`] exactly.
///
/// TODO(A-M-07): Wrap `github_token` and `reddit_client_secret` with
/// `Secret<String>` after migration is complete.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct IngestConfig {
    pub github_token: Option<String>, // TODO(A-M-07): Option<Secret<String>>
    pub github_include_source: bool,
    pub github_max_issues: usize,
    pub github_max_prs: usize,
    pub reddit_client_id: Option<String>,
    pub reddit_client_secret: Option<String>, // TODO(A-M-07): Option<Secret<String>>
    pub reddit_max_posts: usize,
    pub reddit_min_score: i32,
    pub reddit_depth: usize,
    pub reddit_scrape_links: bool,
}

impl Default for IngestConfig {
    fn default() -> Self {
        Self {
            github_token: None,
            github_include_source: true,
            github_max_issues: 100,
            github_max_prs: 100,
            reddit_client_id: None,
            reddit_client_secret: None,
            // These defaults must stay in sync with Config::default() in config_impls.rs.
            reddit_max_posts: 25,
            reddit_min_score: 0,
            reddit_depth: 2,
            reddit_scrape_links: false,
        }
    }
}

/// Configuration for the `ask`, `evaluate`, and `query` RAG pipeline.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AskConfig {
    pub ask_max_context_chars: usize,
    pub ask_candidate_limit: usize,
    pub ask_chunk_limit: usize,
    pub ask_full_docs: usize,
    pub ask_backfill_chunks: usize,
    pub ask_doc_fetch_concurrency: usize,
    pub ask_doc_chunk_limit: usize,
    pub ask_min_relevance_score: f64,
    pub ask_authoritative_domains: Vec<String>,
    pub ask_authoritative_boost: f64,
    pub ask_authoritative_allowlist: Vec<String>,
    pub ask_min_citations_nontrivial: usize,
    pub ask_diagnostics: bool,
}

impl Default for AskConfig {
    fn default() -> Self {
        Self {
            ask_max_context_chars: 120_000,
            ask_candidate_limit: 150,
            ask_chunk_limit: 10,
            ask_full_docs: 4,
            ask_backfill_chunks: 3,
            ask_doc_fetch_concurrency: 4,
            ask_doc_chunk_limit: 192,
            ask_min_relevance_score: 0.45,
            ask_authoritative_domains: vec![],
            ask_authoritative_boost: 0.0,
            ask_authoritative_allowlist: vec![],
            ask_min_citations_nontrivial: 2,
            ask_diagnostics: false,
        }
    }
}

/// Configuration for Chrome-based crawling and screenshot capture.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ChromeConfig {
    pub chrome_remote_url: Option<String>,
    pub chrome_proxy: Option<String>,
    pub chrome_user_agent: Option<String>,
    pub chrome_headless: bool,
    pub chrome_anti_bot: bool,
    pub chrome_intercept: bool,
    pub chrome_stealth: bool,
    pub chrome_bootstrap: bool,
    pub chrome_bootstrap_timeout_ms: u64,
    pub chrome_bootstrap_retries: usize,
    pub chrome_network_idle_timeout_secs: u64,
    pub chrome_wait_for_selector: Option<String>,
    pub chrome_screenshot: bool,
    pub screenshot_full_page: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub bypass_csp: bool,
    pub accept_invalid_certs: bool,
}

impl Default for ChromeConfig {
    fn default() -> Self {
        Self {
            chrome_remote_url: None,
            chrome_proxy: None,
            chrome_user_agent: None,
            chrome_headless: true,
            chrome_anti_bot: true,
            chrome_intercept: true,
            chrome_stealth: true,
            chrome_bootstrap: true,
            chrome_bootstrap_timeout_ms: 3_000,
            chrome_bootstrap_retries: 2,
            chrome_network_idle_timeout_secs: 15,
            chrome_wait_for_selector: None,
            chrome_screenshot: false,
            screenshot_full_page: true,
            viewport_width: 1920,
            viewport_height: 1080,
            bypass_csp: false,
            accept_invalid_certs: false,
        }
    }
}

/// Configuration for crawler behavior: depth, filtering, sitemaps, thin pages.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct CrawlConfig {
    pub max_pages: u32,
    pub max_depth: usize,
    pub include_subdomains: bool,
    pub exclude_path_prefix: Vec<String>,
    pub respect_robots: bool,
    pub min_markdown_chars: usize,
    pub drop_thin_markdown: bool,
    pub discover_sitemaps: bool,
    pub sitemap_since_days: u32,
    pub sitemap_only: bool,
    pub delay_ms: u64,
    pub url_whitelist: Vec<String>,
    pub block_assets: bool,
    pub max_page_bytes: Option<u64>,
    pub redirect_policy_strict: bool,
    pub custom_headers: Vec<String>,
    pub auto_switch_thin_ratio: f64,
    pub auto_switch_min_pages: usize,
    pub crawl_broadcast_buffer_min: usize,
    pub crawl_broadcast_buffer_max: usize,
}

impl Default for CrawlConfig {
    fn default() -> Self {
        Self {
            max_pages: 0,
            max_depth: 5,
            include_subdomains: false,
            exclude_path_prefix: vec![],
            respect_robots: false,
            min_markdown_chars: 200,
            drop_thin_markdown: true,
            discover_sitemaps: true,
            sitemap_since_days: 0,
            sitemap_only: false,
            delay_ms: 0,
            url_whitelist: vec![],
            block_assets: false,
            max_page_bytes: None,
            redirect_policy_strict: false,
            custom_headers: vec![],
            auto_switch_thin_ratio: 0.60,
            auto_switch_min_pages: 10,
            crawl_broadcast_buffer_min: 4096,
            crawl_broadcast_buffer_max: 16_384,
        }
    }
}

/// AMQP queue names and related routing configuration.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct QueueConfig {
    pub shared_queue: bool,
    pub crawl_queue: String,
    pub refresh_queue: String,
    pub extract_queue: String,
    pub embed_queue: String,
    pub ingest_queue: String,
}

impl Default for QueueConfig {
    fn default() -> Self {
        Self {
            shared_queue: true,
            crawl_queue: "axon.crawl.jobs".to_string(),
            refresh_queue: "axon.refresh.jobs".to_string(),
            extract_queue: "axon.extract.jobs".to_string(),
            embed_queue: "axon.embed.jobs".to_string(),
            ingest_queue: "axon.ingest.jobs".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_urls_default_is_empty() {
        let s = ServiceUrls::default();
        assert!(s.pg_url.is_empty());
        assert!(s.openai_api_key.is_empty());
        assert!(s.tavily_api_key.is_empty());
    }

    #[test]
    fn service_urls_debug_redacts_secrets() {
        let s = ServiceUrls {
            pg_url: "postgresql://user:password@host/db".to_string(),
            redis_url: "redis://:secret@host:6379".to_string(),
            amqp_url: "amqp://user:pass@host/%2f".to_string(),
            openai_api_key: "sk-supersecret".to_string(),
            tavily_api_key: "tvly-supersecret".to_string(),
            qdrant_url: "http://localhost:6333".to_string(),
            tei_url: "http://localhost:8080".to_string(),
            openai_base_url: "http://localhost:11434/v1".to_string(),
            openai_model: "llama3".to_string(),
        };
        let dbg = format!("{s:?}");
        assert!(!dbg.contains("password"), "pg_url password leaked");
        assert!(!dbg.contains("secret@"), "redis_url secret leaked");
        assert!(!dbg.contains("sk-supersecret"), "openai_api_key leaked");
        assert!(!dbg.contains("tvly-supersecret"), "tavily_api_key leaked");
        assert!(dbg.contains("[REDACTED]"), "no [REDACTED] marker");
        // Non-secret fields must be visible.
        assert!(dbg.contains("http://localhost:6333"), "qdrant_url missing");
        assert!(dbg.contains("llama3"), "openai_model missing");
    }

    #[test]
    fn ingest_config_default() {
        let c = IngestConfig::default();
        assert!(c.github_token.is_none());
        assert!(c.reddit_client_id.is_none());
        // Defaults must match Config::default() in config_impls.rs.
        assert_eq!(c.reddit_max_posts, 25);
        assert_eq!(c.reddit_min_score, 0);
        assert_eq!(c.reddit_depth, 2);
        assert!(!c.reddit_scrape_links);
        assert!(c.github_include_source);
        assert_eq!(c.github_max_issues, 100);
        assert_eq!(c.github_max_prs, 100);
    }

    #[test]
    fn ask_config_default_values() {
        let c = AskConfig::default();
        assert_eq!(c.ask_max_context_chars, 120_000);
        assert_eq!(c.ask_candidate_limit, 150);
        assert!((c.ask_min_relevance_score - 0.45).abs() < f64::EPSILON);
        assert_eq!(c.ask_min_citations_nontrivial, 2);
        assert!(c.ask_authoritative_domains.is_empty());
    }

    #[test]
    fn chrome_config_default_values() {
        let c = ChromeConfig::default();
        assert!(c.chrome_headless);
        assert!(c.chrome_anti_bot);
        assert!(c.chrome_stealth);
        assert_eq!(c.chrome_network_idle_timeout_secs, 15);
        assert_eq!(c.viewport_width, 1920);
        assert_eq!(c.viewport_height, 1080);
    }

    #[test]
    fn crawl_config_default_values() {
        let c = CrawlConfig::default();
        assert_eq!(c.max_pages, 0);
        assert_eq!(c.max_depth, 5);
        assert!(!c.include_subdomains);
        assert!(c.drop_thin_markdown);
        assert!(c.discover_sitemaps);
    }

    #[test]
    fn queue_config_default_names() {
        let q = QueueConfig::default();
        assert!(q.shared_queue);
        assert_eq!(q.crawl_queue, "axon.crawl.jobs");
        assert_eq!(q.embed_queue, "axon.embed.jobs");
        assert_eq!(q.ingest_queue, "axon.ingest.jobs");
    }
}
