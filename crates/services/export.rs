mod helpers;
mod query;
mod seeds;
mod verify;

use seeds::RebuildSeedsInput;

use crate::crates::core::config::Config;
use crate::crates::jobs::common::make_pool;
use crate::crates::services::types::{
    ExportManifest, ExportMetadata, ExportVerifyReport, IngestExports, RefreshExports, ScrapeExport,
};
use anyhow::Result;
use sqlx::PgPool;

pub(super) const EXPORT_SCHEMA_VERSION: u32 = 3;
pub(super) const REQUIRED_TOP_LEVEL_KEYS: &[&str] = &[
    "version",
    "exported_at",
    "collection",
    "metadata",
    "settings_snapshot",
    "integrity",
    "rebuild_seeds",
    "crawls",
    "scrapes",
    "extractions",
    "embeds",
    "ingests",
    "refreshes",
    "watches",
    "qdrant_summary",
];

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    pub include_history: bool,
    pub statuses: Vec<String>,
}

pub async fn export_manifest(
    cfg: &Config,
    pool: &PgPool,
    opts: &ExportOptions,
) -> Result<ExportManifest> {
    let (crawls, extractions, embeds, ingests, refreshes, watches, query_history, scrape_history) =
        tokio::try_join!(
            query::query_crawl_jobs(pool, &opts.statuses),
            query::query_extract_jobs(pool, &opts.statuses),
            query::query_embed_jobs(pool, &opts.statuses),
            query::query_ingest_jobs(pool, &opts.statuses),
            query::query_refresh_data(pool, &opts.statuses),
            query::query_watch_defs(pool),
            query::query_query_history(pool),
            query::query_scrape_history(pool),
        )?;
    let qdrant_summary = query::query_qdrant_summary(cfg).await?;
    let rebuild_seeds = seeds::build_rebuild_seeds(RebuildSeedsInput {
        crawls: &crawls,
        extractions: &extractions,
        embeds: &embeds,
        ingests: &ingests,
        watches: &watches,
        scrape_history: &scrape_history,
        query_history_search_queries: &query_history.search_queries,
        query_history_research_queries: &query_history.research_queries,
        query_history_search_requests: &query_history.search_requests,
        query_history_research_requests: &query_history.research_requests,
    });
    let settings_snapshot = seeds::build_settings_snapshot(cfg);
    let integrity = helpers::build_integrity(&rebuild_seeds);

    Ok(ExportManifest {
        version: EXPORT_SCHEMA_VERSION,
        exported_at: chrono::Utc::now().to_rfc3339(),
        collection: cfg.collection.clone(),
        metadata: ExportMetadata {
            schema_version: EXPORT_SCHEMA_VERSION,
            generated_by: "axon".to_string(),
            generated_by_version: env!("CARGO_PKG_VERSION").to_string(),
            history_included: opts.include_history,
        },
        settings_snapshot,
        integrity,
        rebuild_seeds,
        crawls: if opts.include_history { crawls } else { vec![] },
        scrapes: if opts.include_history {
            scrape_history
                .requests
                .into_iter()
                .map(|seed| ScrapeExport {
                    url: seed.url,
                    scraped_at: seed.created_at,
                })
                .collect()
        } else {
            vec![]
        },
        extractions: if opts.include_history {
            extractions
        } else {
            vec![]
        },
        embeds: if opts.include_history { embeds } else { vec![] },
        ingests: if opts.include_history {
            ingests
        } else {
            IngestExports {
                github: vec![],
                reddit: vec![],
                youtube: vec![],
                sessions: vec![],
            }
        },
        refreshes: RefreshExports {
            schedules: refreshes.schedules,
            jobs: if opts.include_history {
                refreshes.jobs
            } else {
                vec![]
            },
        },
        watches,
        qdrant_summary,
    })
}

pub async fn export_manifest_for_config(
    cfg: &Config,
    opts: &ExportOptions,
) -> Result<ExportManifest> {
    if cfg.lite_mode {
        anyhow::bail!("export is not available in lite mode");
    }
    let pool = make_pool(cfg).await?;
    export_manifest(cfg, &pool, opts).await
}

pub fn verify_manifest_json(raw_json: &str) -> Result<ExportVerifyReport> {
    let value: serde_json::Value = serde_json::from_str(raw_json)?;
    verify::verify_manifest_value(&value)
}

#[cfg(test)]
mod tests {
    use super::verify::verify_manifest_value;
    use super::*;
    use crate::crates::jobs::common::resolve_test_pg_url;
    use crate::crates::vector::ops::qdrant::qdrant_base;
    use httpmock::Method::{GET, POST};
    use httpmock::MockServer;
    use serial_test::serial;
    use sqlx::postgres::PgPoolOptions;
    use uuid::Uuid;

    #[test]
    fn dedup_query_requests_dedupes_by_query_and_options() {
        use crate::crates::services::types::QuerySeedExport;
        let requests = vec![
            QuerySeedExport {
                request_id: "1".to_string(),
                created_at: None,
                query: "rust qdrant".to_string(),
                options: serde_json::json!({"limit": 5, "offset": 0}),
            },
            QuerySeedExport {
                request_id: "2".to_string(),
                created_at: None,
                query: "rust qdrant".to_string(),
                options: serde_json::json!({"limit": 5, "offset": 0}),
            },
            QuerySeedExport {
                request_id: "3".to_string(),
                created_at: None,
                query: "rust qdrant".to_string(),
                options: serde_json::json!({"limit": 10, "offset": 0}),
            },
        ];
        let deduped = helpers::dedup_query_requests(requests);
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].query, "rust qdrant");
        assert_eq!(deduped[1].query, "rust qdrant");
        assert_eq!(deduped[0].options["limit"], 5);
        assert_eq!(deduped[1].options["limit"], 10);
    }

    #[test]
    fn verify_manifest_detects_integrity_drift() {
        let manifest = serde_json::json!({
            "version": 3,
            "exported_at": "2026-03-21T00:00:00Z",
            "collection": "cortex",
            "metadata": {
                "schema_version": 3,
                "generated_by": "axon",
                "generated_by_version": "0.0.0",
                "history_included": false
            },
            "settings_snapshot": {
                "collection": "cortex",
                "performance_profile": "high-stable",
                "render_mode": "auto-switch",
                "max_pages": 0,
                "max_depth": 5,
                "include_subdomains": false,
                "respect_robots": false,
                "min_markdown_chars": 200,
                "drop_thin_markdown": true,
                "discover_sitemaps": true,
                "sitemap_since_days": 0,
                "request_timeout_ms": null,
                "fetch_retries": 2,
                "retry_backoff_ms": 250,
                "batch_concurrency": 16,
                "crawl_queue": "axon.crawl.jobs",
                "extract_queue": "axon.extract.jobs",
                "embed_queue": "axon.embed.jobs",
                "ingest_queue": "axon.ingest.jobs",
                "graph_queue": "axon.graph.jobs"
            },
            "integrity": {
                "counts": {"crawl_seed_urls": 999},
                "hashes": {"crawl_seed_urls": "deadbeef"}
            },
            "rebuild_seeds": {
                "crawl_seed_urls": ["https://example.com"],
                "scrape_urls": [],
                "scrape_requests": [],
                "github_repos": [],
                "github_requests": [],
                "reddit_targets": [],
                "youtube_targets": [],
                "session_targets": [],
                "local_paths": [],
                "extraction_requests": [],
                "search_requests": [],
                "research_requests": [],
                "search_queries": [],
                "research_queries": []
            },
            "crawls": [],
            "scrapes": [],
            "extractions": [],
            "embeds": [],
            "ingests": {"github": [], "reddit": [], "youtube": [], "sessions": []},
            "refreshes": {"schedules": [], "jobs": []},
            "watches": [],
            "qdrant_summary": {"total_points": 0, "source_type_counts": {}, "domain_counts": {}}
        });

        let report = verify_manifest_value(&manifest).expect("verify report");
        assert!(!report.valid);
        assert!(!report.hash_mismatches.is_empty());
        assert!(!report.count_mismatches.is_empty());
    }

    #[tokio::test]
    #[serial]
    async fn seed_only_export_contains_only_seed_urls_not_crawl_fanout_urls() -> Result<()> {
        let Some(pg_url) = resolve_test_pg_url() else {
            return Ok(());
        };
        let pool = match PgPoolOptions::new()
            .max_connections(1)
            .connect(&pg_url)
            .await
        {
            Ok(pool) => pool,
            Err(_) => return Ok(()),
        };

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_crawl_jobs (
                id UUID PRIMARY KEY,
                url TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                finished_at TIMESTAMPTZ,
                config_json JSONB NOT NULL,
                result_json JSONB
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_extract_jobs (
                id UUID PRIMARY KEY,
                status TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                finished_at TIMESTAMPTZ,
                urls_json JSONB NOT NULL,
                config_json JSONB NOT NULL,
                result_json JSONB
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_embed_jobs (
                id UUID PRIMARY KEY,
                input_text TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                finished_at TIMESTAMPTZ,
                config_json JSONB NOT NULL,
                result_json JSONB
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_ingest_jobs (
                id UUID PRIMARY KEY,
                source_type TEXT NOT NULL,
                target TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                finished_at TIMESTAMPTZ,
                config_json JSONB NOT NULL,
                result_json JSONB
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_refresh_schedules (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                seed_url TEXT,
                urls_json JSONB,
                every_seconds BIGINT NOT NULL,
                enabled BOOLEAN NOT NULL DEFAULT TRUE,
                source_type TEXT,
                target TEXT,
                next_run_at TIMESTAMPTZ,
                last_run_at TIMESTAMPTZ
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_refresh_jobs (
                id UUID PRIMARY KEY,
                urls_json JSONB NOT NULL,
                status TEXT NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                finished_at TIMESTAMPTZ,
                result_json JSONB
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_watch_defs (
                id UUID PRIMARY KEY,
                name TEXT NOT NULL,
                task_type TEXT NOT NULL,
                task_payload JSONB NOT NULL,
                every_seconds BIGINT NOT NULL,
                enabled BOOLEAN NOT NULL DEFAULT TRUE,
                next_run_at TIMESTAMPTZ,
                last_run_at TIMESTAMPTZ,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_query_history (
                id BIGSERIAL PRIMARY KEY,
                kind TEXT NOT NULL,
                query_text TEXT NOT NULL,
                options_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS axon_scrape_seeds (
                id BIGSERIAL PRIMARY KEY,
                url TEXT NOT NULL,
                options_json JSONB NOT NULL DEFAULT '{}'::jsonb,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .execute(&pool)
        .await?;

        let seed = format!("https://seed-{}.example.com/docs", Uuid::new_v4());
        let fanout = format!("https://seed-{}.example.com/issues/42", Uuid::new_v4());
        let repo = format!(
            "owner{}/repo{}",
            Uuid::new_v4().simple(),
            Uuid::new_v4().simple()
        );

        sqlx::query("INSERT INTO axon_crawl_jobs (id, url, status, config_json, result_json) VALUES ($1,$2,'completed',$3,$4)")
            .bind(Uuid::new_v4())
            .bind(&seed)
            .bind(serde_json::json!({"max_depth": 2}))
            .bind(serde_json::json!({"pages": [{"url": fanout}]}))
            .execute(&pool)
            .await?;

        sqlx::query("INSERT INTO axon_ingest_jobs (id, source_type, target, status, config_json, result_json) VALUES ($1,'github',$2,'completed',$3,$4)")
            .bind(Uuid::new_v4())
            .bind(&repo)
            .bind(serde_json::json!({"source": {"include_issues": true, "include_prs": true}}))
            .bind(serde_json::json!({"chunks_embedded": 10}))
            .execute(&pool)
            .await?;

        sqlx::query("INSERT INTO axon_query_history (kind, query_text, options_json) VALUES ('search',$1,$2), ('research',$3,$4)")
            .bind("how to rust")
            .bind(serde_json::json!({"limit": 10}))
            .bind("rust qdrant benchmark")
            .bind(serde_json::json!({"limit": 5}))
            .execute(&pool)
            .await?;

        sqlx::query("INSERT INTO axon_scrape_seeds (url, options_json) VALUES ($1,$2)")
            .bind("https://example.com/scrape-seed")
            .bind(serde_json::json!({"format": "markdown"}))
            .execute(&pool)
            .await?;

        let server = MockServer::start();
        let coll_path = "/collections/cortex";
        let facet_path = "/collections/cortex/facet";

        let _collection = server.mock(|when, then| {
            when.method(GET).path(coll_path);
            then.status(200)
                .json_body(serde_json::json!({"result": {"points_count": 123}}));
        });
        let _facet = server.mock(|when, then| {
            when.method(POST).path(facet_path);
            then.status(200)
                .json_body(serde_json::json!({"result": {"hits": []}}));
        });

        let mut cfg = Config::test_default();
        cfg.pg_url = pg_url;
        cfg.qdrant_url = server.base_url();
        cfg.collection = "cortex".to_string();

        let manifest = export_manifest(
            &cfg,
            &pool,
            &ExportOptions {
                include_history: false,
                statuses: vec![],
            },
        )
        .await?;

        assert!(manifest.crawls.is_empty());
        assert!(manifest.scrapes.is_empty());
        assert!(manifest.extractions.is_empty());
        assert!(manifest.embeds.is_empty());
        assert!(manifest.ingests.github.is_empty());
        assert!(manifest.refreshes.jobs.is_empty());

        assert!(manifest.rebuild_seeds.crawl_seed_urls.contains(&seed));
        assert!(manifest.rebuild_seeds.github_repos.contains(&repo));

        let raw = serde_json::to_string(&manifest)?;
        assert!(
            !raw.contains(&fanout),
            "crawl fanout url leaked into seed-only export"
        );

        Ok(())
    }

    #[test]
    fn qdrant_base_reads_from_config() {
        let mut cfg = Config::test_default();
        cfg.qdrant_url = "http://127.0.0.1:6333".to_string();
        assert_eq!(qdrant_base(&cfg), "http://127.0.0.1:6333");
    }
}
