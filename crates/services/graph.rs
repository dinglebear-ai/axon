use crate::crates::core::config::Config;
use crate::crates::core::neo4j::Neo4jClient;
use crate::crates::jobs::common::make_pool;
use crate::crates::jobs::graph::{enqueue_graph_job, ensure_graph_schema};
use crate::crates::services::context::ServiceContext;
use crate::crates::services::jobs as job_service;
use crate::crates::services::runtime::WorkerMode;
use crate::crates::services::types::{
    GraphBuildResult, GraphExploreResult, GraphStatsResult, GraphStatusResult,
};
use crate::crates::vector::ops::qdrant::{qdrant_indexed_urls, qdrant_urls_for_domain};
use futures_util::stream::{FuturesUnordered, StreamExt};
use std::collections::BTreeMap;
use std::error::Error;
use uuid::Uuid;

/// Downcast `Box<dyn Error + Send + Sync>` → `Box<dyn Error>` for `?` compatibility.
fn drop_ss(e: Box<dyn Error + Send + Sync>) -> Box<dyn Error> {
    e
}

fn require_neo4j(cfg: &Config) -> Result<Neo4jClient, Box<dyn Error>> {
    Neo4jClient::from_config(cfg)
        .map_err(drop_ss)?
        .ok_or_else(|| "graph operations require AXON_NEO4J_URL".into())
}

fn require_graph_support(cfg: &Config) -> Result<(), Box<dyn Error>> {
    if cfg.lite_mode {
        return Err("graph is not available in lite mode".into());
    }
    Ok(())
}

/// Maximum number of URLs to retrieve from Qdrant for graph build operations.
/// Prevents unbounded full-collection scrolls that cause DoS on large collections.
const GRAPH_BUILD_URL_LIMIT: usize = 50_000;

#[must_use = "graph_build returns a Result that should be handled"]
pub async fn graph_build(
    cfg: &Config,
    url: Option<&str>,
    domain: Option<&str>,
    all: bool,
) -> Result<GraphBuildResult, Box<dyn Error>> {
    require_graph_support(cfg)?;
    let _neo4j = require_neo4j(cfg)?;
    let pool = make_pool(cfg).await?;
    ensure_graph_schema(&pool).await?;

    let mut urls = if let Some(url) = url {
        vec![url.to_string()]
    } else if let Some(domain) = domain {
        // Use server-side domain filter instead of fetching 500k URLs and filtering in Rust.
        qdrant_urls_for_domain(cfg, domain)
            .await?
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        if !all {
            return Err("graph_build requires a URL, --all, or domain filter".into());
        }
        qdrant_indexed_urls(cfg, Some(GRAPH_BUILD_URL_LIMIT)).await?
    };

    urls.sort();
    urls.dedup();
    if url.is_none() && urls.len() > GRAPH_BUILD_URL_LIMIT {
        urls.truncate(GRAPH_BUILD_URL_LIMIT);
    }

    // Enqueue concurrently (32 at a time) — each URL holds an independent pg advisory lock
    // so parallel enqueues are safe. Serial enqueue of 50k URLs would take 8–40 minutes.
    const ENQUEUE_CONCURRENCY: usize = 32;
    let mut iter = urls.iter();
    let mut inflight: FuturesUnordered<_> = FuturesUnordered::new();
    for url in iter.by_ref().take(ENQUEUE_CONCURRENCY) {
        inflight.push(enqueue_graph_job(&pool, cfg, url, "crawl"));
    }
    let mut job_ids = Vec::with_capacity(urls.len());
    while let Some(result) = inflight.next().await {
        job_ids.push(result?.to_string());
        if let Some(url) = iter.next() {
            inflight.push(enqueue_graph_job(&pool, cfg, url, "crawl"));
        }
    }

    Ok(GraphBuildResult {
        payload: serde_json::json!({
            "queued": urls.len(),
            "job_ids": job_ids,
            "urls": urls,
            "mode": if url.is_some() { "single" } else if domain.is_some() { "domain" } else { "all" },
        }),
    })
}

#[must_use = "graph_status returns a Result that should be handled"]
pub async fn graph_status(cfg: &Config) -> Result<GraphStatusResult, Box<dyn Error>> {
    require_graph_support(cfg)?;
    let pool = make_pool(cfg).await?;
    ensure_graph_schema(&pool).await?;
    let rows = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT status, COUNT(*)::BIGINT
        FROM axon_graph_jobs
        GROUP BY status
        ORDER BY status
        "#,
    )
    .fetch_all(&pool)
    .await?;

    let counts: BTreeMap<String, i64> = rows.into_iter().collect();

    let recent = sqlx::query_as::<_, (Uuid, String, String, i32, i32, i32, Option<String>)>(
        r#"
        SELECT id, url, status, chunk_count, entity_count, relation_count, error_text
        FROM axon_graph_jobs
        ORDER BY created_at DESC
        LIMIT 20
        "#,
    )
    .fetch_all(&pool)
    .await?;

    Ok(GraphStatusResult {
        payload: serde_json::json!({
            "counts": counts,
            "recent": recent.into_iter().map(|(id, url, status, chunk_count, entity_count, relation_count, error_text)| {
                serde_json::json!({
                    "id": id,
                    "url": url,
                    "status": status,
                    "chunk_count": chunk_count,
                    "entity_count": entity_count,
                    "relation_count": relation_count,
                    "error": error_text,
                })
            }).collect::<Vec<_>>(),
        }),
    })
}

#[must_use = "graph_explore returns a Result that should be handled"]
pub async fn graph_explore(
    cfg: &Config,
    entity: &str,
) -> Result<GraphExploreResult, Box<dyn Error>> {
    require_graph_support(cfg)?;
    let entity = entity.trim();
    if entity.is_empty() {
        return Err("graph_explore requires a non-empty entity name".into());
    }
    if entity.len() > 1000 {
        return Err("graph_explore entity name exceeds 1000 character limit".into());
    }
    let neo4j = require_neo4j(cfg)?;
    let rows = neo4j
        .query(
            "MATCH (e:Entity {name: $name}) \
             OPTIONAL MATCH (e)-[r]-(neighbor:Entity) \
             WITH e, collect({name: neighbor.name, type: neighbor.entity_type, relation: coalesce(r.relation, type(r))}) AS neighbors \
             OPTIONAL MATCH (e)-[:MENTIONED_IN]->(c:Chunk)-[:BELONGS_TO]->(d:Document) \
             RETURN e.name, e.entity_type, coalesce(e.description, ''), neighbors, count(DISTINCT d), count(c)",
            serde_json::json!({ "name": entity }),
        )
        .await
        .map_err(drop_ss)?;

    Ok(GraphExploreResult {
        payload: serde_json::json!({
            "entity": entity,
            "rows": rows,
        }),
    })
}

#[must_use = "graph_stats returns a Result that should be handled"]
pub async fn graph_stats(cfg: &Config) -> Result<GraphStatsResult, Box<dyn Error>> {
    require_graph_support(cfg)?;
    let neo4j = require_neo4j(cfg)?;
    let rows = neo4j
        .query(
            "MATCH (e:Entity) WITH count(e) AS entities \
             MATCH ()-[r]->() WITH entities, count(r) AS relationships \
             MATCH (d:Document) WITH entities, relationships, count(d) AS documents \
             MATCH (c:Chunk) RETURN entities, relationships, documents, count(c) AS chunks",
            serde_json::json!({}),
        )
        .await
        .map_err(drop_ss)?;

    Ok(GraphStatsResult {
        payload: serde_json::json!({
            "rows": rows,
        }),
    })
}

pub async fn graph_worker(service_context: &ServiceContext) -> Result<(), Box<dyn Error>> {
    require_graph_support(service_context.cfg.as_ref())?;
    match job_service::run_worker(
        service_context,
        crate::crates::jobs::backend::JobKind::Graph,
    )
    .await?
    {
        WorkerMode::Started | WorkerMode::InProcess => Ok(()),
        WorkerMode::Unsupported(message) => Err(message.into()),
    }
}
