use super::extract::{
    ExtractedEntity, ExtractedRelationship, extract_entities_llm, normalize_entity_name,
    resolve_type_conflict,
};
use super::persist::{
    GraphChunk, GraphRelationRecord, MergedEntity, finalize_similarity, persist_edges,
    persist_nodes,
};
use super::schema::{ensure_graph_schema, ensure_neo4j_schema};
use super::taxonomy::{CandidateSource, EntityCandidate, Taxonomy};
use crate::crates::core::config::Config;
use crate::crates::core::logging::{log_done, log_info, log_warn};
use crate::crates::core::neo4j::Neo4jClient;
use crate::crates::jobs::common::{JobTable, make_pool, mark_job_completed, mark_job_failed};
use crate::crates::jobs::worker_lane::{
    ProcessFn, WorkerConfig, run_job_worker, validate_worker_env_vars,
};
use crate::crates::vector::ops::qdrant::{payload_text_typed, qdrant_retrieve_by_url};
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use uuid::Uuid;

const TABLE: JobTable = JobTable::Graph;
const GRAPH_HEARTBEAT_INTERVAL_SECS: u64 = 30;

#[derive(Debug, Clone, Deserialize, Default)]
struct GraphJobConfig {
    #[serde(default)]
    source_type: Option<String>,
}

pub fn merge_candidates(candidates: Vec<EntityCandidate>) -> Vec<EntityCandidate> {
    let mut merged = HashMap::<String, EntityCandidate>::new();
    for candidate in candidates {
        let key = normalize_entity_name(&candidate.name);
        if key.is_empty() {
            continue;
        }
        let replace = match merged.get(&key) {
            Some(existing) => {
                candidate.confidence > existing.confidence
                    || (candidate.confidence == existing.confidence
                        && matches!(candidate.source, CandidateSource::Import))
            }
            None => true,
        };
        if replace {
            merged.insert(key, candidate);
        }
    }
    let mut out = merged.into_values().collect::<Vec<_>>();
    out.sort_by(|left, right| left.name.cmp(&right.name));
    out
}

pub fn partition_by_ambiguity(
    candidates: Vec<EntityCandidate>,
) -> (Vec<EntityCandidate>, Vec<EntityCandidate>) {
    candidates.into_iter().partition(|c| !c.ambiguous)
}

async fn load_graph_job(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<(String, GraphJobConfig)>, Box<dyn Error>> {
    let row = sqlx::query_as::<_, (String, serde_json::Value)>(
        "SELECT url, config_json FROM axon_graph_jobs WHERE id=$1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    let Some((url, cfg_json)) = row else {
        return Ok(None);
    };
    let job_cfg = serde_json::from_value(cfg_json).unwrap_or_default();
    Ok(Some((url, job_cfg)))
}

fn merge_llm_entities(
    entities: &mut HashMap<String, MergedEntity>,
    llm_entities: Vec<ExtractedEntity>,
) {
    for entity in llm_entities {
        let key = normalize_entity_name(&entity.name);
        if key.is_empty() {
            continue;
        }
        match entities.get_mut(&key) {
            Some(existing) => {
                existing.entity_type =
                    resolve_type_conflict(&existing.entity_type, &entity.entity_type);
            }
            None => {
                entities.insert(
                    key,
                    MergedEntity {
                        name: entity.name,
                        entity_type: entity.entity_type,
                        confidence: 0.8,
                    },
                );
            }
        }
    }
}

fn build_relationships(
    entities: &HashMap<String, MergedEntity>,
    relationships: Vec<ExtractedRelationship>,
) -> Vec<GraphRelationRecord> {
    let mut deduped = BTreeSet::new();
    for relationship in relationships {
        let source_key = normalize_entity_name(&relationship.source);
        let target_key = normalize_entity_name(&relationship.target);
        let Some(source) = entities.get(&source_key) else {
            continue;
        };
        let Some(target) = entities.get(&target_key) else {
            continue;
        };
        if source.name == target.name {
            continue;
        }
        deduped.insert(GraphRelationRecord {
            source: source.name.clone(),
            target: target.name.clone(),
            relation: relationship.relation,
        });
    }
    deduped.into_iter().collect()
}

async fn build_entity_map(
    cfg: &Config,
    taxonomy: &Taxonomy,
    chunks: &[GraphChunk],
    source_type: &str,
) -> Result<
    (
        HashMap<String, MergedEntity>,
        Vec<GraphRelationRecord>,
        usize,
    ),
    Box<dyn Error + Send + Sync>,
> {
    let mut taxonomy_candidates = Vec::new();
    for chunk in chunks {
        taxonomy_candidates.extend(taxonomy.extract_entities(&chunk.chunk_text, source_type));
    }
    let merged_candidates = merge_candidates(taxonomy_candidates);
    let (clear_candidates, ambiguous_candidates) = partition_by_ambiguity(merged_candidates);
    let mut entities = clear_candidates
        .into_iter()
        .map(|candidate| {
            (
                normalize_entity_name(&candidate.name),
                MergedEntity {
                    name: candidate.name,
                    entity_type: candidate.entity_type,
                    confidence: candidate.confidence,
                },
            )
        })
        .collect::<HashMap<_, _>>();

    let mut relationship_count = 0usize;
    let relationships = if ambiguous_candidates.is_empty() || cfg.graph_llm_url.trim().is_empty() {
        Vec::new()
    } else {
        let prompt_text = chunks
            .iter()
            .map(|chunk| chunk.chunk_text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        let llm = extract_entities_llm(cfg, &prompt_text).await?;
        merge_llm_entities(&mut entities, llm.entities);
        let rels = build_relationships(&entities, llm.relationships);
        // Neo4j MERGEs on (source, target) only — two records with different
        // relation labels for the same pair produce one persisted edge (the
        // last write wins). Count unique (source, target) pairs to match what
        // Neo4j actually stores rather than the raw relationship vec length.
        relationship_count = rels
            .iter()
            .map(|r| (r.source.as_str(), r.target.as_str()))
            .collect::<std::collections::HashSet<_>>()
            .len();
        rels
    };

    Ok((entities, relationships, relationship_count))
}

/// Core graph extraction for a single URL. No job-table interaction — callers handle that.
/// Used by both the full Postgres worker and the lite SQLite worker.
pub(crate) async fn process_graph_url(
    cfg: &Config,
    neo4j: &Neo4jClient,
    taxonomy: &Taxonomy,
    url: &str,
    source_type: &str,
) -> Result<serde_json::Value, Box<dyn Error + Send + Sync>> {
    let points = qdrant_retrieve_by_url(cfg, url, None).await?;
    if points.is_empty() {
        return Ok(serde_json::json!({
            "url": url,
            "chunk_count": 0,
            "entity_count": 0,
            "relation_count": 0,
            "similarity_edges": 0,
        }));
    }

    let mut chunks = Vec::new();
    for point in points {
        let chunk_text = payload_text_typed(&point.payload).to_string();
        if chunk_text.trim().is_empty() {
            continue;
        }
        let point_id = match point.id {
            serde_json::Value::String(value) => value,
            other => other.to_string(),
        };
        let chunk_index = point.payload.chunk_index.unwrap_or_default();
        chunks.push(GraphChunk {
            point_id,
            chunk_index,
            chunk_text,
        });
    }

    let (entities, relationships, relationship_count) =
        build_entity_map(cfg, taxonomy, &chunks, source_type).await?;

    // Stage 1: write Document+Chunk nodes and Entity nodes atomically via try_join!.
    // If either write fails the other does not commit independently, preventing
    // a partial state that would cause Stage 2 edge MATCHes to fail.
    persist_nodes(neo4j, cfg, url, source_type, &chunks, &entities).await?;

    // Stage 2: write edges that reference Stage 1 nodes in parallel.
    let mention_count = persist_edges(
        neo4j,
        taxonomy,
        source_type,
        &chunks,
        &entities,
        &relationships,
    )
    .await?;

    // Stage 3: similarity depends on Document nodes from Stage 1.
    let similarity_edges = finalize_similarity(cfg, neo4j, url).await?;

    Ok(serde_json::json!({
        "url": url,
        "chunk_count": chunks.len(),
        "entity_count": entities.len(),
        "relation_count": relationship_count + mention_count + similarity_edges.len(),
        "similarity_edges": similarity_edges.len(),
    }))
}

async fn process_graph_job(
    cfg: &Config,
    neo4j: &Neo4jClient,
    taxonomy: &Taxonomy,
    pool: &PgPool,
    id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let job_start = std::time::Instant::now();
    let Some((url, job_cfg)) = load_graph_job(pool, id).await? else {
        return Ok(());
    };
    let source_type = job_cfg.source_type.unwrap_or_else(|| "crawl".to_string());

    let result = process_graph_url(cfg, neo4j, taxonomy, &url, &source_type)
        .await
        .map_err(|e| e as Box<dyn Error>)?;
    let _ = mark_job_completed(pool, TABLE, id, Some(&result)).await?;

    log_done(&format!(
        "graph worker completed job {id} url={url} duration_ms={}",
        job_start.elapsed().as_millis()
    ));
    Ok(())
}

async fn process_claimed_graph_job(
    cfg: std::sync::Arc<Config>,
    pool: PgPool,
    id: Uuid,
    neo4j: std::sync::Arc<Neo4jClient>,
    taxonomy: std::sync::Arc<Taxonomy>,
) {
    if let Err(error) = process_graph_job(&cfg, &neo4j, &taxonomy, &pool, id).await {
        let error_text = error.to_string();
        if let Err(err) = mark_job_failed(&pool, TABLE, id, &error_text).await {
            log_warn(&format!("mark_job_failed failed job_id={id} error={err}"));
        }
        log_warn(&format!("graph worker failed job {id}: {error_text}"));
    }
}

pub async fn run_graph_worker(cfg: &Config) -> anyhow::Result<()> {
    if let Err(msg) = validate_worker_env_vars() {
        return Err(anyhow::anyhow!("{msg}"));
    }
    if cfg.neo4j_url.trim().is_empty() {
        return Err(anyhow::anyhow!(
            "graph worker requires AXON_NEO4J_URL to be set"
        ));
    }

    log_info(&format!(
        "worker_start worker=graph queue={} collection={}",
        cfg.graph_queue, cfg.collection
    ));

    let neo4j = Neo4jClient::from_config(cfg)
        .map_err(|e| anyhow::anyhow!("Neo4j client init failed: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("graph worker requires Neo4j configuration"))?;
    let neo4j = std::sync::Arc::new(neo4j);
    let taxonomy = Taxonomy::resolve(&cfg.graph_taxonomy_path)
        .map_err(|e| anyhow::anyhow!("taxonomy init failed: {e}"))?;

    let pool = make_pool(cfg).await?;
    ensure_graph_schema(&pool)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;
    ensure_neo4j_schema(&neo4j)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))?;

    let wc = WorkerConfig {
        table: TABLE,
        queue_name: cfg.graph_queue.clone(),
        job_kind: "graph",
        consumer_tag_prefix: "axon-rust-graph-worker",
        lane_count: cfg.graph_concurrency.max(1),
        heartbeat_interval_secs: GRAPH_HEARTBEAT_INTERVAL_SECS,
    };

    let process_fn: ProcessFn = {
        let neo4j = std::sync::Arc::clone(&neo4j);
        let taxonomy = std::sync::Arc::clone(&taxonomy);
        std::sync::Arc::new(move |cfg, pool, id| {
            let neo4j = std::sync::Arc::clone(&neo4j);
            let taxonomy = std::sync::Arc::clone(&taxonomy);
            Box::pin(process_claimed_graph_job(cfg, pool, id, neo4j, taxonomy))
        })
    };

    run_job_worker(cfg, pool, &wc, process_fn)
        .await
        .map_err(|err| anyhow::anyhow!("{err}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_entity_candidates_deduplicates() {
        let taxonomy_candidates = vec![
            EntityCandidate {
                name: "Tokio".to_string(),
                entity_type: "technology".to_string(),
                confidence: 0.95,
                source: CandidateSource::Taxonomy,
                ambiguous: false,
            },
            EntityCandidate {
                name: "Tokio".to_string(),
                entity_type: "technology".to_string(),
                confidence: 0.9,
                source: CandidateSource::Import,
                ambiguous: false,
            },
        ];
        let merged = merge_candidates(taxonomy_candidates);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "Tokio");
    }

    #[test]
    fn separate_ambiguous_candidates() {
        let candidates = vec![
            EntityCandidate {
                name: "Docker".to_string(),
                entity_type: "technology".to_string(),
                confidence: 0.95,
                source: CandidateSource::Taxonomy,
                ambiguous: false,
            },
            EntityCandidate {
                name: "React".to_string(),
                entity_type: "technology".to_string(),
                confidence: 0.8,
                source: CandidateSource::Taxonomy,
                ambiguous: true,
            },
        ];
        let (clear, ambiguous) = partition_by_ambiguity(candidates);
        assert_eq!(clear.len(), 1);
        assert_eq!(ambiguous.len(), 1);
        assert_eq!(ambiguous[0].name, "React");
    }
}
