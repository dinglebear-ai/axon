use super::helpers::{
    dedup_query_requests, dedup_sorted, json_array_to_strings, json_num_to_u64, json_string_opt,
    status_matches, to_rfc3339_opt,
};
use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use crate::crates::services::types::{
    CrawlExport, EmbedExport, ExtractionExport, IngestExports, IngestSourceExport, QdrantSummary,
    QuerySeedExport, RefreshExports, RefreshJobExport, RefreshScheduleExport, ScrapeSeedExport,
    WatchExport,
};
use crate::crates::vector::ops::qdrant::{qdrant_base, qdrant_facet};
use anyhow::Result;
use sqlx::{PgPool, Row};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct QueryHistoryExport {
    pub(super) search_queries: Vec<String>,
    pub(super) research_queries: Vec<String>,
    pub(super) search_requests: Vec<QuerySeedExport>,
    pub(super) research_requests: Vec<QuerySeedExport>,
}

#[derive(Default)]
pub(super) struct ScrapeHistoryExport {
    pub(super) requests: Vec<ScrapeSeedExport>,
}

pub(super) async fn query_crawl_jobs(
    pool: &PgPool,
    statuses: &[String],
) -> Result<Vec<CrawlExport>> {
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

pub(super) async fn query_extract_jobs(
    pool: &PgPool,
    statuses: &[String],
) -> Result<Vec<ExtractionExport>> {
    let rows = sqlx::query(
        r#"SELECT id, status, created_at, finished_at, urls_json, config_json,
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
            config: row.try_get("config_json")?,
            status,
            created_at: to_rfc3339_opt(row.try_get("created_at")?),
            finished_at: to_rfc3339_opt(row.try_get("finished_at")?),
            total_items: json_num_to_u64(row.try_get("total_items")?),
        });
    }
    Ok(out)
}

pub(super) async fn query_embed_jobs(
    pool: &PgPool,
    statuses: &[String],
) -> Result<Vec<EmbedExport>> {
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

pub(super) async fn query_ingest_jobs(pool: &PgPool, statuses: &[String]) -> Result<IngestExports> {
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

pub(super) async fn query_refresh_data(
    pool: &PgPool,
    statuses: &[String],
) -> Result<RefreshExports> {
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

pub(super) async fn query_qdrant_summary(cfg: &Config) -> Result<QdrantSummary> {
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
    Ok(QdrantSummary {
        total_points,
        source_type_counts,
        domain_counts,
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

pub(super) async fn query_watch_defs(pool: &PgPool) -> Result<Vec<WatchExport>> {
    let rows = sqlx::query(
        r#"SELECT id, name, task_type, task_payload, every_seconds, enabled, next_run_at, last_run_at, created_at, updated_at
           FROM axon_watch_defs
           ORDER BY name ASC"#,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|row| -> Result<WatchExport, sqlx::Error> {
            Ok(WatchExport {
                id: row.try_get::<uuid::Uuid, _>("id")?.to_string(),
                name: row.try_get("name")?,
                task_type: row.try_get("task_type")?,
                task_payload: row.try_get("task_payload")?,
                every_seconds: row.try_get("every_seconds")?,
                enabled: row.try_get("enabled")?,
                next_run_at: to_rfc3339_opt(row.try_get("next_run_at")?),
                last_run_at: to_rfc3339_opt(row.try_get("last_run_at")?),
                created_at: to_rfc3339_opt(row.try_get("created_at")?),
                updated_at: to_rfc3339_opt(row.try_get("updated_at")?),
            })
        })
        .collect::<Result<Vec<_>, _>>()
        .map_err(Into::into)
}

pub(super) async fn query_query_history(pool: &PgPool) -> Result<QueryHistoryExport> {
    let rows = match sqlx::query(
        r#"SELECT id, kind, query_text, options_json, created_at
           FROM axon_query_history
           ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("42P01") => {
            return Ok(QueryHistoryExport::default());
        }
        Err(err) => return Err(err.into()),
    };

    let mut search_queries = Vec::new();
    let mut research_queries = Vec::new();
    let mut search_requests = Vec::new();
    let mut research_requests = Vec::new();
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let kind: String = row.try_get("kind")?;
        let query_text: String = row.try_get("query_text")?;
        let options: serde_json::Value = row.try_get("options_json")?;
        let created_at = to_rfc3339_opt(row.try_get("created_at")?);
        match kind.as_str() {
            "search" => {
                search_queries.push(query_text.clone());
                search_requests.push(QuerySeedExport {
                    request_id: id.to_string(),
                    created_at: created_at.clone(),
                    query: query_text,
                    options,
                });
            }
            "research" => {
                research_queries.push(query_text.clone());
                research_requests.push(QuerySeedExport {
                    request_id: id.to_string(),
                    created_at: created_at.clone(),
                    query: query_text,
                    options,
                });
            }
            _ => {}
        }
    }

    Ok(QueryHistoryExport {
        search_queries: dedup_sorted(search_queries.iter().map(String::as_str)),
        research_queries: dedup_sorted(research_queries.iter().map(String::as_str)),
        search_requests: dedup_query_requests(search_requests),
        research_requests: dedup_query_requests(research_requests),
    })
}

pub(super) async fn query_scrape_history(pool: &PgPool) -> Result<ScrapeHistoryExport> {
    let rows = match sqlx::query(
        r#"SELECT id, url, options_json, created_at
           FROM axon_scrape_seeds
           ORDER BY created_at DESC"#,
    )
    .fetch_all(pool)
    .await
    {
        Ok(rows) => rows,
        Err(sqlx::Error::Database(db_err)) if db_err.code().as_deref() == Some("42P01") => {
            return Ok(ScrapeHistoryExport::default());
        }
        Err(err) => return Err(err.into()),
    };

    let mut requests = Vec::with_capacity(rows.len());
    for row in rows {
        let id: i64 = row.try_get("id")?;
        let url: String = row.try_get("url")?;
        let options: serde_json::Value = row.try_get("options_json")?;
        let created_at = to_rfc3339_opt(row.try_get("created_at")?);
        requests.push(ScrapeSeedExport {
            request_id: id.to_string(),
            created_at,
            url,
            options,
        });
    }

    Ok(ScrapeHistoryExport { requests })
}
