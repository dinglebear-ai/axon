use super::*;

#[test]
fn service_urls_default_is_empty() {
    let s = ServiceUrls::default();
    assert!(s.pg_url.is_empty());
    assert!(s.tavily_api_key.is_empty());
}

#[test]
fn service_urls_debug_redacts_secrets() {
    let s = ServiceUrls {
        pg_url: "postgresql://user:password@host/db".to_string(),
        redis_url: "redis://:secret@host:6379".to_string(),
        amqp_url: "amqp://user:pass@host/%2f".to_string(),
        tavily_api_key: "tvly-supersecret".to_string(),
        qdrant_url: "http://localhost:6333".to_string(),
        tei_url: "http://localhost:8080".to_string(),
    };
    let dbg = format!("{s:?}");
    assert!(!dbg.contains("password"), "pg_url password leaked");
    assert!(!dbg.contains("secret@"), "redis_url secret leaked");
    assert!(!dbg.contains("tvly-supersecret"), "tavily_api_key leaked");
    assert!(dbg.contains("[REDACTED]"), "no [REDACTED] marker");
    // Non-secret fields must be visible.
    assert!(dbg.contains("http://localhost:6333"), "qdrant_url missing");
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
    assert_eq!(c.ask_max_context_chars, 300_000);
    assert_eq!(c.ask_candidate_limit, 250);
    assert_eq!(c.ask_hybrid_candidates, 150);
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
    assert_eq!(c.max_depth, 10);
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
