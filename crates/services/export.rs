use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::services::types::{
    CrawlExport, EmbedExport, ExportManifest, ExtractionExport, IngestExports, IngestSourceExport,
    QdrantSummary, RefreshExports, RefreshJobExport, RefreshScheduleExport, ScrapeExport,
};
use crate::crates::vector::ops::qdrant::{qdrant_base, qdrant_facet, qdrant_url_facets};
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{PgPool, Row};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub include_urls: bool,
    pub url_limit: usize,
    pub statuses: Vec<String>,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            include_urls: true,
            url_limit: 100_000,
            statuses: vec![],
        }
    }
}

pub async fn export_manifest(
    cfg: &Config,
    pool: &PgPool,
    opts: &ExportOptions,
) -> Result<ExportManifest> {
    let (crawls, extractions, embeds, ingests, refreshes, qdrant_summary) = tokio::try_join!(
        query_crawl_jobs(pool, &opts.statuses),
        query_extract_jobs(pool, &opts.statuses),
        query_embed_jobs(pool, &opts.statuses),
        query_ingest_jobs(pool, &opts.statuses),
        query_refresh_data(pool, &opts.statuses),
        query_qdrant_summary(cfg, opts),
    )?;

    Ok(ExportManifest {
        version: 1,
        exported_at: Utc::now().to_rfc3339(),
        collection: cfg.collection.clone(),
        crawls,
        scrapes: extract_scrape_urls_from_embed_jobs(&embeds),
        extractions,
        embeds,
        ingests,
        refreshes,
        qdrant_summary,
    })
}

async fn query_crawl_jobs(pool: &PgPool, statuses: &[String]) -> Result<Vec<CrawlExport>> {
    let rows = sqlx::query(
        r#"SELECT id, url, status, created_at, finished_at, config_json,
                  result_json->'pages_crawled' AS pages_crawled,
                  result_json->'pages_discovered' AS pages_discovered
           FROM axon_crawl_jobs
           ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status: String = row.try_get("status")?;
        if !status_matches(&status, statuses) {
            continue;
        }

        out.push(CrawlExport {
            job_id: row.try_get::<uuid::Uuid, _>("id")?.to_string(),
            seed_url: row.try_get("url")?,
            status,
            created_at: to_rfc3339_opt(row.try_get("created_at")?),
            finished_at: to_rfc3339_opt(row.try_get("finished_at")?),
            config: row.try_get("config_json")?,
            pages_crawled: json_num_to_u64(row.try_get("pages_crawled")?),
            pages_discovered: json_num_to_u64(row.try_get("pages_discovered")?),
        });
    }

    Ok(out)
}

async fn query_extract_jobs(pool: &PgPool, statuses: &[String]) -> Result<Vec<ExtractionExport>> {
    let rows = sqlx::query(
        r#"SELECT id, status, created_at, finished_at, urls_json,
                  config_json->'prompt' AS prompt,
                  result_json->'total_items' AS total_items
           FROM axon_extract_jobs
           ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status: String = row.try_get("status")?;
        if !status_matches(&status, statuses) {
            continue;
        }

        out.push(ExtractionExport {
            job_id: row.try_get::<uuid::Uuid, _>("id")?.to_string(),
            urls: json_array_to_strings(row.try_get("urls_json")?),
            prompt: json_string_opt(row.try_get("prompt")?),
            status,
            created_at: to_rfc3339_opt(row.try_get("created_at")?),
            finished_at: to_rfc3339_opt(row.try_get("finished_at")?),
            total_items: json_num_to_u64(row.try_get("total_items")?),
        });
    }

    Ok(out)
}

async fn query_embed_jobs(pool: &PgPool, statuses: &[String]) -> Result<Vec<EmbedExport>> {
    let rows = sqlx::query(
        r#"SELECT id, input_text, status, created_at, finished_at, config_json,
                  result_json->'chunks_embedded' AS chunks_embedded
           FROM axon_embed_jobs
           ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        let status: String = row.try_get("status")?;
        if !status_matches(&status, statuses) {
            continue;
        }

        let config: serde_json::Value = row.try_get("config_json")?;
        out.push(EmbedExport {
            job_id: row.try_get::<uuid::Uuid, _>("id")?.to_string(),
            input: row.try_get("input_text")?,
            collection: config
                .get("collection")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string(),
            status,
            source_type: config
                .get("source_type")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string),
            created_at: to_rfc3339_opt(row.try_get("created_at")?),
            finished_at: to_rfc3339_opt(row.try_get("finished_at")?),
            chunks_embedded: json_num_to_u64(row.try_get("chunks_embedded")?),
        });
    }

    Ok(out)
}

async fn query_ingest_jobs(pool: &PgPool, statuses: &[String]) -> Result<IngestExports> {
    let rows = sqlx::query(
        r#"SELECT id, source_type, target, status, created_at, finished_at,
                  config_json, result_json->'chunks_embedded' AS chunks_embedded
           FROM axon_ingest_jobs
           ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await?;

    let mut github = Vec::new();
    let mut reddit = Vec::new();
    let mut youtube = Vec::new();
    let mut sessions = Vec::new();

    for row in rows {
        let status: String = row.try_get("status")?;
        if !status_matches(&status, statuses) {
            continue;
        }

        let entry = IngestSourceExport {
            job_id: row.try_get::<uuid::Uuid, _>("id")?.to_string(),
            target: row.try_get("target")?,
            status,
            created_at: to_rfc3339_opt(row.try_get("created_at")?),
            finished_at: to_rfc3339_opt(row.try_get("finished_at")?),
            config: row.try_get("config_json")?,
            chunks_embedded: json_num_to_u64(row.try_get("chunks_embedded")?),
        };

        match row.try_get::<String, _>("source_type")?.as_str() {
            "github" => github.push(entry),
            "reddit" => reddit.push(entry),
            "youtube" => youtube.push(entry),
            "sessions" => sessions.push(entry),
            _ => {}
        }
    }

    Ok(IngestExports {
        github,
        reddit,
        youtube,
        sessions,
    })
}

async fn query_refresh_data(pool: &PgPool, statuses: &[String]) -> Result<RefreshExports> {
    let schedule_rows = sqlx::query(
        r#"SELECT id, name, seed_url, urls_json, every_seconds, enabled,
                  source_type, target, next_run_at, last_run_at
           FROM axon_refresh_schedules
           ORDER BY name ASC"#,
    )
    .fetch_all(pool)
    .await?;

    let schedules = schedule_rows
        .into_iter()
        .map(|row| -> Result<RefreshScheduleExport, sqlx::Error> {
            Ok(RefreshScheduleExport {
                id: row.try_get::<uuid::Uuid, _>("id")?.to_string(),
                name: row.try_get("name")?,
                seed_url: row.try_get("seed_url")?,
                urls: json_array_to_strings(row.try_get("urls_json")?),
                every_seconds: row.try_get("every_seconds")?,
                enabled: row.try_get("enabled")?,
                source_type: row.try_get("source_type")?,
                target: row.try_get("target")?,
                next_run_at: to_rfc3339_opt(row.try_get("next_run_at")?),
                last_run_at: to_rfc3339_opt(row.try_get("last_run_at")?),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let refresh_rows = sqlx::query(
        r#"SELECT id, urls_json, status, created_at, finished_at,
                  result_json->'checked' AS checked,
                  result_json->'changed' AS changed
           FROM axon_refresh_jobs
           ORDER BY created_at DESC
           LIMIT 100"#,
    )
    .fetch_all(pool)
    .await?;

    let mut jobs = Vec::with_capacity(refresh_rows.len());
    for row in refresh_rows {
        let status: String = row.try_get("status")?;
        if !status_matches(&status, statuses) {
            continue;
        }

        jobs.push(RefreshJobExport {
            job_id: row.try_get::<uuid::Uuid, _>("id")?.to_string(),
            urls: json_array_to_strings(row.try_get("urls_json")?),
            status,
            created_at: to_rfc3339_opt(row.try_get("created_at")?),
            finished_at: to_rfc3339_opt(row.try_get("finished_at")?),
            checked: json_num_to_u64(row.try_get("checked")?),
            changed: json_num_to_u64(row.try_get("changed")?),
        });
    }

    Ok(RefreshExports { schedules, jobs })
}

async fn query_qdrant_summary(cfg: &Config, opts: &ExportOptions) -> Result<QdrantSummary> {
    let total_points = fetch_collection_point_count(cfg).await.unwrap_or(0);

    let source_type_counts = qdrant_facet(cfg, "source_type", 100)
        .await?
        .into_iter()
        .map(|(k, v)| (k, v as u64))
        .collect::<HashMap<_, _>>();

    let domain_counts = qdrant_facet(cfg, "domain", 10_000)
        .await?
        .into_iter()
        .map(|(k, v)| (k, v as u64))
        .collect::<HashMap<_, _>>();

    let indexed_urls = if opts.include_urls {
        qdrant_url_facets(cfg, opts.url_limit)
            .await?
            .into_iter()
            .map(|(url, _)| url)
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    Ok(QdrantSummary {
        total_points,
        source_type_counts,
        domain_counts,
        indexed_urls,
    })
}

async fn fetch_collection_point_count(cfg: &Config) -> Result<u64> {
    let url = format!("{}/collections/{}", qdrant_base(cfg), cfg.collection);
    let payload = http_client()?
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;

    Ok(payload
        .get("result")
        .and_then(|v| v.get("points_count"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0))
}

fn extract_scrape_urls_from_embed_jobs(embeds: &[EmbedExport]) -> Vec<ScrapeExport> {
    let mut by_url: HashMap<String, Option<String>> = HashMap::new();

    for embed in embeds {
        if embed.source_type.as_deref() != Some("scrape") {
            continue;
        }

        if !(embed.input.starts_with("http://") || embed.input.starts_with("https://")) {
            continue;
        }

        by_url
            .entry(embed.input.clone())
            .and_modify(|earliest| {
                if earliest.is_none() {
                    *earliest = embed.created_at.clone();
                }
            })
            .or_insert_with(|| embed.created_at.clone());
    }

    let mut scrapes = by_url
        .into_iter()
        .map(|(url, scraped_at)| ScrapeExport { url, scraped_at })
        .collect::<Vec<_>>();
    scrapes.sort_by(|a, b| a.url.cmp(&b.url));
    scrapes
}

fn status_matches(status: &str, statuses: &[String]) -> bool {
    statuses.is_empty() || statuses.iter().any(|s| s == status)
}

fn to_rfc3339_opt(ts: Option<DateTime<Utc>>) -> Option<String> {
    ts.map(|value| value.to_rfc3339())
}

fn json_num_to_u64(value: Option<serde_json::Value>) -> Option<u64> {
    value.and_then(|v| {
        if v.is_null() {
            return None;
        }
        v.as_u64()
            .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
            .or_else(|| v.as_str().and_then(|s| s.parse::<u64>().ok()))
    })
}

fn json_string_opt(value: Option<serde_json::Value>) -> Option<String> {
    value.and_then(|v| v.as_str().map(str::to_string))
}

fn json_array_to_strings(value: Option<serde_json::Value>) -> Vec<String> {
    value
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}
