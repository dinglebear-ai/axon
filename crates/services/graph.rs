use crate::crates::core::config::Config;
use crate::crates::core::neo4j::Neo4jClient;
use crate::crates::jobs::common::make_pool;
use crate::crates::jobs::graph::{enqueue_graph_job, ensure_graph_schema, run_graph_worker};
use crate::crates::services::types::{
    GraphBuildResult, GraphExploreResult, GraphStatsResult, GraphStatusResult,
};
use crate::crates::vector::ops::qdrant::qdrant_indexed_urls;
use spider::url::Url;
use std::collections::BTreeMap;
use std::error::Error;
use uuid::Uuid;

fn require_neo4j(cfg: &Config) -> Result<Neo4jClient, Box<dyn Error>> {
    Neo4jClient::from_config(cfg)?.ok_or_else(|| "graph operations require AXON_NEO4J_URL".into())
}

/// Maximum number of URLs to retrieve from Qdrant for graph build operations.
/// Prevents unbounded full-collection scrolls that cause DoS on large collections.
const GRAPH_BUILD_URL_LIMIT: usize = 50_000;

/// Pre-fetch limit for domain-scoped graph builds.
/// Larger than GRAPH_BUILD_URL_LIMIT to improve domain coverage,
/// but still capped to prevent unbounded full-collection scrolls.
const GRAPH_BUILD_DOMAIN_FETCH_LIMIT: usize = 500_000;

/// Check whether `url` belongs to `domain` (exact match or subdomain).
///
/// `domain_suffix` must be `".{domain}"` — pre-computed by the caller to avoid
/// a per-URL allocation when filtering large URL lists.
fn url_matches_domain(url: &str, domain: &str, domain_suffix: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| host == domain || host.ends_with(domain_suffix))
        })
        .unwrap_or(false)
}

pub async fn graph_build(
    cfg: &Config,
    url: Option<&str>,
    domain: Option<&str>,
    all: bool,
) -> Result<GraphBuildResult, Box<dyn Error>> {
    let _neo4j = require_neo4j(cfg)?;
    let pool = make_pool(cfg).await?;
    ensure_graph_schema(&pool).await?;

    let mut urls = if let Some(url) = url {
        vec![url.to_string()]
    } else if domain.is_some() {
        // Domain-scoped builds fetch more URLs to improve domain coverage,
        // then filter down to only matching domain URLs.
        qdrant_indexed_urls(cfg, Some(GRAPH_BUILD_DOMAIN_FETCH_LIMIT)).await?
    } else {
        qdrant_indexed_urls(cfg, Some(GRAPH_BUILD_URL_LIMIT)).await?
    };

    if let Some(domain) = domain {
        let suffix = format!(".{domain}");
        urls.retain(|candidate| url_matches_domain(candidate, domain, &suffix));
    } else if !all && url.is_none() {
        return Err("graph_build requires a URL, --all, or domain filter".into());
    }

    urls.sort();
    urls.dedup();
    if url.is_none() && urls.len() > GRAPH_BUILD_URL_LIMIT {
        urls.truncate(GRAPH_BUILD_URL_LIMIT);
    }

    let mut job_ids = Vec::new();
    for item in &urls {
        let job_id = enqueue_graph_job(&pool, cfg, item, "crawl").await?;
        job_ids.push(job_id.to_string());
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

pub async fn graph_status(cfg: &Config) -> Result<GraphStatusResult, Box<dyn Error>> {
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

pub async fn graph_explore(
    cfg: &Config,
    entity: &str,
) -> Result<GraphExploreResult, Box<dyn Error>> {
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
        .await?;

    Ok(GraphExploreResult {
        payload: serde_json::json!({
            "entity": entity,
            "rows": rows,
        }),
    })
}

pub async fn graph_stats(cfg: &Config) -> Result<GraphStatsResult, Box<dyn Error>> {
    let neo4j = require_neo4j(cfg)?;
    let rows = neo4j
        .query(
            "MATCH (e:Entity) WITH count(e) AS entities \
             MATCH ()-[r]->() WITH entities, count(r) AS relationships \
             MATCH (d:Document) WITH entities, relationships, count(d) AS documents \
             MATCH (c:Chunk) RETURN entities, relationships, documents, count(c) AS chunks",
            serde_json::json!({}),
        )
        .await?;

    Ok(GraphStatsResult {
        payload: serde_json::json!({
            "rows": rows,
        }),
    })
}

pub async fn graph_worker(cfg: &Config) -> Result<(), Box<dyn Error>> {
    run_graph_worker(cfg)
        .await
        .map_err(|err| -> Box<dyn Error> { err.into() })
}
