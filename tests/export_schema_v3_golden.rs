use axon::crates::services::types::{
    ExportIntegrity, ExportManifest, ExportMetadata, IngestExports, QdrantSummary,
    RebuildSeedsExport, RefreshExports, SettingsSnapshot,
};
use std::collections::HashMap;

const EMPTY_SHA256: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

fn integrity_counts() -> HashMap<String, u64> {
    let mut counts = HashMap::new();
    for key in [
        "crawl_seed_urls",
        "scrape_urls",
        "scrape_requests",
        "github_repos",
        "github_requests",
        "reddit_targets",
        "youtube_targets",
        "session_targets",
        "local_paths",
        "extraction_requests",
        "search_requests",
        "research_requests",
    ] {
        counts.insert(key.to_string(), 0);
    }
    counts
}

fn integrity_hashes() -> HashMap<String, String> {
    let mut hashes = HashMap::new();
    for key in [
        "crawl_seed_urls",
        "scrape_urls",
        "github_repos",
        "reddit_targets",
        "youtube_targets",
        "session_targets",
        "local_paths",
        "search_queries",
        "research_queries",
    ] {
        hashes.insert(key.to_string(), EMPTY_SHA256.to_string());
    }
    hashes
}

fn sample_manifest() -> ExportManifest {
    ExportManifest {
        version: 3,
        exported_at: "2026-03-21T00:00:00Z".into(),
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
            counts: integrity_counts(),
            hashes: integrity_hashes(),
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
    }
}

#[test]
fn export_schema_v3_matches_golden_fixture() {
    let fixture_raw = std::fs::read_to_string("tests/fixtures/export_schema_v3.golden.json")
        .expect("read golden fixture");
    let fixture_value: serde_json::Value =
        serde_json::from_str(&fixture_raw).expect("parse golden fixture json");

    let generated_value = serde_json::to_value(sample_manifest()).expect("serialize manifest");

    assert_eq!(
        generated_value, fixture_value,
        "export schema drifted from v3 golden fixture"
    );
}
