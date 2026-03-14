use super::extract::{
    ExtractedEntity, ExtractedRelationship, extract_entities_llm, normalize_entity_name,
    resolve_type_conflict,
};
use super::schema::{ensure_graph_schema, ensure_neo4j_schema};
use super::similarity::compute_similarity;
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

#[derive(Debug, Clone)]
struct GraphChunk {
    point_id: String,
    chunk_index: i64,
    chunk_text: String,
}

#[derive(Debug, Clone, PartialEq)]
struct MergedEntity {
    name: String,
    entity_type: String,
    confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct GraphRelationRecord {
    source: String,
    target: String,
    relation: String,
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
    let mut clear = Vec::new();
    let mut ambiguous = Vec::new();
    for candidate in candidates {
        if candidate.ambiguous {
            ambiguous.push(candidate);
        } else {
            clear.push(candidate);
        }
    }
    (clear, ambiguous)
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

fn candidate_names_for_chunk(
    taxonomy: &Taxonomy,
    chunk_text: &str,
    source_type: &str,
    entities: &HashMap<String, MergedEntity>,
) -> Vec<String> {
    let mut names = BTreeSet::new();
    for candidate in taxonomy.extract_entities(chunk_text, source_type) {
        let key = normalize_entity_name(&candidate.name);
        if let Some(entity) = entities.get(&key) {
            names.insert(entity.name.clone());
        }
    }
    names.into_iter().collect()
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

async fn write_document_and_chunks(
    neo4j: &Neo4jClient,
    cfg: &Config,
    url: &str,
    source_type: &str,
    chunks: &[GraphChunk],
) -> Result<(), Box<dyn Error>> {
    for chunk in chunks {
        neo4j
            .execute(
                "MERGE (d:Document {url: $url}) \
                 SET d.source_type = $source_type, \
                     d.collection = $collection, \
                     d.updated_at = datetime() \
                 MERGE (c:Chunk {point_id: $point_id}) \
                 SET c.url = $url, \
                     c.collection = $collection, \
                     c.chunk_index = $chunk_index, \
                     c.updated_at = datetime() \
                 MERGE (c)-[:BELONGS_TO]->(d)",
                serde_json::json!({
                    "url": url,
                    "source_type": source_type,
                    "collection": cfg.collection,
                    "point_id": chunk.point_id,
                    "chunk_index": chunk.chunk_index,
                }),
            )
            .await?;
    }
    Ok(())
}

async fn write_entities(
    neo4j: &Neo4jClient,
    entities: &HashMap<String, MergedEntity>,
) -> Result<(), Box<dyn Error>> {
    for entity in entities.values() {
        neo4j
            .execute(
                "MERGE (e:Entity {name: $name}) \
                 SET e.entity_type = $entity_type, \
                     e.confidence = $confidence, \
                     e.updated_at = datetime()",
                serde_json::json!({
                    "name": entity.name,
                    "entity_type": entity.entity_type,
                    "confidence": entity.confidence,
                }),
            )
            .await?;
    }
    Ok(())
}

async fn write_chunk_mentions(
    neo4j: &Neo4jClient,
    taxonomy: &Taxonomy,
    source_type: &str,
    chunks: &[GraphChunk],
    entities: &HashMap<String, MergedEntity>,
) -> Result<usize, Box<dyn Error>> {
    let mut mention_count = 0usize;
    for chunk in chunks {
        let names = candidate_names_for_chunk(taxonomy, &chunk.chunk_text, source_type, entities);
        for name in names {
            neo4j
                .execute(
                    "MATCH (e:Entity {name: $name}) \
                     MATCH (c:Chunk {point_id: $point_id}) \
                     MERGE (e)-[:MENTIONED_IN]->(c)",
                    serde_json::json!({
                        "name": name,
                        "point_id": chunk.point_id,
                    }),
                )
                .await?;
            mention_count += 1;
        }
    }
    Ok(mention_count)
}

async fn write_entity_relationships(
    neo4j: &Neo4jClient,
    relationships: &[GraphRelationRecord],
) -> Result<(), Box<dyn Error>> {
    for relationship in relationships {
        neo4j
            .execute(
                "MATCH (s:Entity {name: $source}) \
                 MATCH (t:Entity {name: $target}) \
                 MERGE (s)-[r:RELATES_TO]->(t) \
                 SET r.relation = $relation, \
                     r.updated_at = datetime()",
                serde_json::json!({
                    "source": relationship.source,
                    "target": relationship.target,
                    "relation": relationship.relation,
                }),
            )
            .await?;
    }
    Ok(())
}

async fn process_graph_job(
    cfg: &Config,
    neo4j: &Neo4jClient,
    pool: &PgPool,
    id: Uuid,
) -> Result<(), Box<dyn Error>> {
    let job_start = std::time::Instant::now();
    let Some((url, job_cfg)) = load_graph_job(pool, id).await? else {
        return Ok(());
    };
    let source_type = job_cfg.source_type.unwrap_or_else(|| "crawl".to_string());
    let taxonomy = if cfg.graph_taxonomy_path.trim().is_empty() {
        Taxonomy::builtin()
    } else {
        Taxonomy::from_path(&cfg.graph_taxonomy_path)?
    };

    let points = qdrant_retrieve_by_url(cfg, &url, None).await?;
    if points.is_empty() {
        let result = serde_json::json!({
            "url": url,
            "chunk_count": 0,
            "entity_count": 0,
            "relation_count": 0,
            "similarity_edges": 0,
        });
        let _ = mark_job_completed(pool, TABLE, id, Some(&result)).await?;
        return Ok(());
    }

    let mut chunks = Vec::new();
    let mut taxonomy_candidates = Vec::new();
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
            chunk_text: chunk_text.clone(),
        });
        taxonomy_candidates.extend(taxonomy.extract_entities(&chunk_text, &source_type));
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

    let llm_result = if ambiguous_candidates.is_empty() || cfg.graph_llm_url.trim().is_empty() {
        None
    } else {
        let prompt_text = chunks
            .iter()
            .map(|chunk| chunk.chunk_text.as_str())
            .collect::<Vec<_>>()
            .join("\n\n");
        Some(extract_entities_llm(cfg, &prompt_text).await?)
    };

    if let Some(result) = llm_result {
        merge_llm_entities(&mut entities, result.entities);
        let relationships = build_relationships(&entities, result.relationships);
        write_document_and_chunks(neo4j, cfg, &url, &source_type, &chunks).await?;
        write_entities(neo4j, &entities).await?;
        let mention_count =
            write_chunk_mentions(neo4j, &taxonomy, &source_type, &chunks, &entities).await?;
        write_entity_relationships(neo4j, &relationships).await?;
        let similarity_edges = compute_similarity(cfg, neo4j, &url).await?;
        let result = serde_json::json!({
            "url": url,
            "chunk_count": chunks.len(),
            "entity_count": entities.len(),
            "relation_count": relationships.len() + mention_count + similarity_edges.len(),
            "similarity_edges": similarity_edges.len(),
            "llm_entities": entities.len(),
        });
        let _ = mark_job_completed(pool, TABLE, id, Some(&result)).await?;
    } else {
        write_document_and_chunks(neo4j, cfg, &url, &source_type, &chunks).await?;
        write_entities(neo4j, &entities).await?;
        let mention_count =
            write_chunk_mentions(neo4j, &taxonomy, &source_type, &chunks, &entities).await?;
        let similarity_edges = compute_similarity(cfg, neo4j, &url).await?;
        let result = serde_json::json!({
            "url": url,
            "chunk_count": chunks.len(),
            "entity_count": entities.len(),
            "relation_count": mention_count + similarity_edges.len(),
            "similarity_edges": similarity_edges.len(),
        });
        let _ = mark_job_completed(pool, TABLE, id, Some(&result)).await?;
    }

    log_done(&format!(
        "graph worker completed job {id} url={url} duration_ms={}",
        job_start.elapsed().as_millis()
    ));
    Ok(())
}

async fn process_claimed_graph_job(cfg: Config, pool: PgPool, id: Uuid) {
    let neo4j = match Neo4jClient::from_config(&cfg) {
        Ok(Some(client)) => client,
        Ok(None) => {
            let error_text = "AXON_NEO4J_URL is required for graph worker".to_string();
            if let Err(err) = mark_job_failed(&pool, TABLE, id, &error_text).await {
                log_warn(&format!("mark_job_failed failed job_id={id} error={err}"));
            }
            return;
        }
        Err(e) => {
            let error_text = format!("Failed to initialize Neo4j client: {}", e);
            if let Err(err) = mark_job_failed(&pool, TABLE, id, &error_text).await {
                log_warn(&format!("mark_job_failed failed job_id={id} error={err}"));
            }
            return;
        }
    };

    if let Err(error) = process_graph_job(&cfg, &neo4j, &pool, id).await {
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

    let process_fn: ProcessFn =
        std::sync::Arc::new(|cfg, pool, id| Box::pin(process_claimed_graph_job(cfg, pool, id)));

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
