use super::helpers::{
    dedup_github_seed_requests, dedup_query_requests, dedup_scrape_requests, dedup_sorted,
};
use super::query::ScrapeHistoryExport;
use crate::crates::core::config::Config;
use crate::crates::services::types::{
    CrawlExport, EmbedExport, ExtractionExport, ExtractionSeedExport, GithubSeedExport,
    IngestExports, QuerySeedExport, RebuildSeedsExport, SettingsSnapshot, WatchExport,
};

pub(super) struct RebuildSeedsInput<'a> {
    pub(super) crawls: &'a [CrawlExport],
    pub(super) extractions: &'a [ExtractionExport],
    pub(super) embeds: &'a [EmbedExport],
    pub(super) ingests: &'a IngestExports,
    pub(super) watches: &'a [WatchExport],
    pub(super) scrape_history: &'a ScrapeHistoryExport,
    pub(super) query_history_search_queries: &'a [String],
    pub(super) query_history_research_queries: &'a [String],
    pub(super) query_history_search_requests: &'a [QuerySeedExport],
    pub(super) query_history_research_requests: &'a [QuerySeedExport],
}

pub(super) fn build_rebuild_seeds(input: RebuildSeedsInput<'_>) -> RebuildSeedsExport {
    let crawl_seed_urls = collect_crawl_seed_urls(input.crawls);
    let scrape_requests = dedup_scrape_requests(input.scrape_history.requests.to_vec());
    let scrape_urls = dedup_sorted(scrape_requests.iter().map(|r| r.url.as_str()));
    let github_requests = dedup_github_seed_requests(
        input
            .ingests
            .github
            .iter()
            .map(|ingest| GithubSeedExport {
                request_id: ingest.job_id.clone(),
                created_at: ingest.created_at.clone(),
                target: ingest.target.clone(),
                options: ingest
                    .config
                    .get("source")
                    .cloned()
                    .unwrap_or_else(|| ingest.config.clone()),
            })
            .collect(),
    );
    let github_repos = dedup_sorted(input.ingests.github.iter().map(|v| v.target.as_str()));
    let reddit_targets = dedup_sorted(input.ingests.reddit.iter().map(|v| v.target.as_str()));
    let youtube_targets = dedup_sorted(input.ingests.youtube.iter().map(|v| v.target.as_str()));
    let session_targets = dedup_sorted(input.ingests.sessions.iter().map(|v| v.target.as_str()));
    let local_paths = dedup_sorted(
        input
            .embeds
            .iter()
            .filter(|embed| embed.source_type.as_deref() == Some("embed"))
            .map(|embed| embed.input.as_str())
            .filter(|input| !input.starts_with("http://") && !input.starts_with("https://")),
    );
    let extraction_requests = input
        .extractions
        .iter()
        .map(|extract| ExtractionSeedExport {
            request_id: extract.job_id.clone(),
            created_at: extract.created_at.clone(),
            urls: extract.urls.clone(),
            prompt: extract.prompt.clone(),
            config: extract.config.clone(),
        })
        .collect::<Vec<_>>();

    let mut search_queries = dedup_sorted(
        input
            .watches
            .iter()
            .filter(|watch| watch.task_type == "search")
            .filter_map(|watch| watch.task_payload.get("query"))
            .filter_map(serde_json::Value::as_str),
    );
    let mut research_queries = dedup_sorted(
        input
            .watches
            .iter()
            .filter(|watch| watch.task_type == "research")
            .filter_map(|watch| watch.task_payload.get("query"))
            .filter_map(serde_json::Value::as_str),
    );
    search_queries.extend_from_slice(input.query_history_search_queries);
    research_queries.extend_from_slice(input.query_history_research_queries);
    search_queries = dedup_sorted(search_queries.iter().map(String::as_str));
    research_queries = dedup_sorted(research_queries.iter().map(String::as_str));
    let search_requests = dedup_query_requests(input.query_history_search_requests.to_vec());
    let research_requests = dedup_query_requests(input.query_history_research_requests.to_vec());

    RebuildSeedsExport {
        crawl_seed_urls,
        scrape_urls,
        scrape_requests,
        github_repos,
        github_requests,
        reddit_targets,
        youtube_targets,
        session_targets,
        local_paths,
        extraction_requests,
        search_requests,
        research_requests,
        search_queries,
        research_queries,
    }
}

fn collect_crawl_seed_urls(crawls: &[CrawlExport]) -> Vec<String> {
    let mut urls = crawls
        .iter()
        .map(|crawl| crawl.seed_url.trim())
        .filter(|url| !url.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    urls.sort();
    urls.dedup();
    urls
}

pub(super) fn build_settings_snapshot(cfg: &Config) -> SettingsSnapshot {
    SettingsSnapshot {
        collection: cfg.collection.clone(),
        performance_profile: format!("{:?}", cfg.performance_profile).to_lowercase(),
        render_mode: cfg.render_mode.to_string(),
        max_pages: cfg.max_pages,
        max_depth: cfg.max_depth,
        include_subdomains: cfg.include_subdomains,
        respect_robots: cfg.respect_robots,
        min_markdown_chars: cfg.min_markdown_chars,
        drop_thin_markdown: cfg.drop_thin_markdown,
        discover_sitemaps: cfg.discover_sitemaps,
        sitemap_since_days: cfg.sitemap_since_days,
        request_timeout_ms: cfg.request_timeout_ms,
        fetch_retries: cfg.fetch_retries,
        retry_backoff_ms: cfg.retry_backoff_ms,
        batch_concurrency: cfg.batch_concurrency,
        crawl_queue: cfg.crawl_queue.clone(),
        extract_queue: cfg.extract_queue.clone(),
        embed_queue: cfg.embed_queue.clone(),
        ingest_queue: cfg.ingest_queue.clone(),
        graph_queue: cfg.graph_queue.clone(),
    }
}
