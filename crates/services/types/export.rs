use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level export manifest for rebuilding indexed knowledge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportManifest {
    pub version: u32,
    pub exported_at: String,
    pub collection: String,
    pub metadata: ExportMetadata,
    pub settings_snapshot: SettingsSnapshot,
    pub integrity: ExportIntegrity,
    pub rebuild_seeds: RebuildSeedsExport,
    pub crawls: Vec<CrawlExport>,
    pub scrapes: Vec<ScrapeExport>,
    pub extractions: Vec<ExtractionExport>,
    pub embeds: Vec<EmbedExport>,
    pub ingests: IngestExports,
    pub refreshes: RefreshExports,
    pub watches: Vec<WatchExport>,
    pub qdrant_summary: QdrantSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebuildSeedsExport {
    pub crawl_seed_urls: Vec<String>,
    pub scrape_urls: Vec<String>,
    pub scrape_requests: Vec<ScrapeSeedExport>,
    pub github_repos: Vec<String>,
    pub github_requests: Vec<GithubSeedExport>,
    pub reddit_targets: Vec<String>,
    pub youtube_targets: Vec<String>,
    pub session_targets: Vec<String>,
    pub local_paths: Vec<String>,
    pub extraction_requests: Vec<ExtractionSeedExport>,
    pub search_requests: Vec<QuerySeedExport>,
    pub research_requests: Vec<QuerySeedExport>,
    pub search_queries: Vec<String>,
    pub research_queries: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionSeedExport {
    pub request_id: String,
    pub created_at: Option<String>,
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    pub config: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuerySeedExport {
    pub request_id: String,
    pub created_at: Option<String>,
    pub query: String,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeSeedExport {
    pub request_id: String,
    pub created_at: Option<String>,
    pub url: String,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GithubSeedExport {
    pub request_id: String,
    pub created_at: Option<String>,
    pub target: String,
    pub options: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportMetadata {
    pub schema_version: u32,
    pub generated_by: String,
    pub generated_by_version: String,
    pub history_included: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsSnapshot {
    pub collection: String,
    pub performance_profile: String,
    pub render_mode: String,
    pub max_pages: u32,
    pub max_depth: usize,
    pub include_subdomains: bool,
    pub respect_robots: bool,
    pub min_markdown_chars: usize,
    pub drop_thin_markdown: bool,
    pub discover_sitemaps: bool,
    pub sitemap_since_days: u32,
    pub request_timeout_ms: Option<u64>,
    pub fetch_retries: usize,
    pub retry_backoff_ms: u64,
    pub batch_concurrency: usize,
    pub crawl_queue: String,
    pub extract_queue: String,
    pub embed_queue: String,
    pub ingest_queue: String,
    pub graph_queue: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportIntegrity {
    pub counts: HashMap<String, u64>,
    pub hashes: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportVerifyMismatch {
    pub key: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportVerifyReport {
    pub valid: bool,
    pub version: Option<u32>,
    pub required_keys_checked: Vec<String>,
    pub missing_required_keys: Vec<String>,
    pub parse_error: Option<String>,
    pub hash_mismatches: Vec<ExportVerifyMismatch>,
    pub count_mismatches: Vec<ExportVerifyMismatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlExport {
    pub job_id: String,
    pub seed_url: String,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub config: serde_json::Value,
    pub pages_crawled: Option<u64>,
    pub pages_discovered: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeExport {
    pub url: String,
    pub scraped_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionExport {
    pub job_id: String,
    pub urls: Vec<String>,
    pub prompt: Option<String>,
    pub config: serde_json::Value,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub total_items: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbedExport {
    pub job_id: String,
    pub input: String,
    pub collection: String,
    pub status: String,
    pub source_type: Option<String>,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub chunks_embedded: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestExports {
    pub github: Vec<IngestSourceExport>,
    pub reddit: Vec<IngestSourceExport>,
    pub youtube: Vec<IngestSourceExport>,
    pub sessions: Vec<IngestSourceExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestSourceExport {
    pub job_id: String,
    pub target: String,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub config: serde_json::Value,
    pub chunks_embedded: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshExports {
    pub schedules: Vec<RefreshScheduleExport>,
    pub jobs: Vec<RefreshJobExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshScheduleExport {
    pub id: String,
    pub name: String,
    pub seed_url: Option<String>,
    pub urls: Vec<String>,
    pub every_seconds: i64,
    pub enabled: bool,
    pub source_type: Option<String>,
    pub target: Option<String>,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshJobExport {
    pub job_id: String,
    pub urls: Vec<String>,
    pub status: String,
    pub created_at: Option<String>,
    pub finished_at: Option<String>,
    pub checked: Option<u64>,
    pub changed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WatchExport {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub task_payload: serde_json::Value,
    pub every_seconds: i64,
    pub enabled: bool,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QdrantSummary {
    pub total_points: u64,
    pub source_type_counts: HashMap<String, u64>,
    pub domain_counts: HashMap<String, u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_manifest_serializes_to_json() {
        let manifest = ExportManifest {
            version: 3,
            exported_at: "2026-03-19T12:00:00Z".into(),
            collection: "cortex".into(),
            metadata: ExportMetadata {
                schema_version: 3,
                generated_by: "axon".into(),
                generated_by_version: "0.0.0-test".into(),
                history_included: false,
            },
            settings_snapshot: SettingsSnapshot {
                collection: "cortex".into(),
                performance_profile: "high-stable".into(),
                render_mode: "auto-switch".into(),
                max_pages: 0,
                max_depth: 5,
                include_subdomains: false,
                respect_robots: false,
                min_markdown_chars: 200,
                drop_thin_markdown: true,
                discover_sitemaps: true,
                sitemap_since_days: 0,
                request_timeout_ms: None,
                fetch_retries: 2,
                retry_backoff_ms: 250,
                batch_concurrency: 16,
                crawl_queue: "axon.crawl.jobs".into(),
                extract_queue: "axon.extract.jobs".into(),
                embed_queue: "axon.embed.jobs".into(),
                ingest_queue: "axon.ingest.jobs".into(),
                graph_queue: "axon.graph.jobs".into(),
            },
            integrity: ExportIntegrity {
                counts: HashMap::new(),
                hashes: HashMap::new(),
            },
            rebuild_seeds: RebuildSeedsExport {
                crawl_seed_urls: vec![],
                scrape_urls: vec![],
                scrape_requests: vec![],
                github_repos: vec![],
                github_requests: vec![],
                reddit_targets: vec![],
                youtube_targets: vec![],
                session_targets: vec![],
                local_paths: vec![],
                extraction_requests: vec![],
                search_requests: vec![],
                research_requests: vec![],
                search_queries: vec![],
                research_queries: vec![],
            },
            crawls: vec![],
            scrapes: vec![],
            extractions: vec![],
            embeds: vec![],
            ingests: IngestExports {
                github: vec![],
                reddit: vec![],
                youtube: vec![],
                sessions: vec![],
            },
            refreshes: RefreshExports {
                schedules: vec![],
                jobs: vec![],
            },
            watches: vec![],
            qdrant_summary: QdrantSummary {
                total_points: 0,
                source_type_counts: HashMap::new(),
                domain_counts: HashMap::new(),
            },
        };

        let json = serde_json::to_string(&manifest).expect("manifest serializes");
        assert!(json.contains("\"version\":3"));
        assert!(json.contains("\"crawls\":[]"));
        assert!(!json.contains("indexed_urls"));
    }
}
