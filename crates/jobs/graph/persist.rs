use super::taxonomy::Taxonomy;
use crate::crates::core::config::Config;
use crate::crates::core::neo4j::Neo4jClient;
use std::collections::HashMap;
use std::error::Error;

// ─── shared data types used by worker + persist ───────────────────────────────

#[derive(Debug, Clone)]
pub struct GraphChunk {
    pub point_id: String,
    pub chunk_index: i64,
    pub chunk_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MergedEntity {
    pub name: String,
    pub entity_type: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphRelationRecord {
    pub source: String,
    pub target: String,
    pub relation: String,
}

// ─── neo4j write helpers ──────────────────────────────────────────────────────

pub fn candidate_names_for_chunk(
    taxonomy: &Taxonomy,
    chunk_text: &str,
    source_type: &str,
    entities: &HashMap<String, MergedEntity>,
) -> Vec<String> {
    use std::collections::BTreeSet;

    use super::extract::normalize_entity_name;
    let mut names = BTreeSet::new();
    for candidate in taxonomy.extract_entities(chunk_text, source_type) {
        let key = normalize_entity_name(&candidate.name);
        if let Some(entity) = entities.get(&key) {
            names.insert(entity.name.clone());
        }
    }
    names.into_iter().collect()
}

pub async fn write_document_and_chunks(
    neo4j: &Neo4jClient,
    cfg: &Config,
    url: &str,
    source_type: &str,
    chunks: &[GraphChunk],
) -> Result<(), Box<dyn Error>> {
    let items: Vec<serde_json::Value> = chunks
        .iter()
        .map(|chunk| {
            serde_json::json!({
                "point_id": chunk.point_id,
                "chunk_index": chunk.chunk_index,
            })
        })
        .collect();

    neo4j
        .execute(
            "MERGE (d:Document {url: $url}) \
             SET d.source_type = $source_type, \
                 d.collection = $collection, \
                 d.updated_at = datetime() \
             WITH d \
             UNWIND $items AS item \
             MERGE (c:Chunk {point_id: item.point_id}) \
             SET c.url = $url, \
                 c.collection = $collection, \
                 c.chunk_index = item.chunk_index, \
                 c.updated_at = datetime() \
             MERGE (c)-[:BELONGS_TO]->(d)",
            serde_json::json!({
                "url": url,
                "source_type": source_type,
                "collection": cfg.collection,
                "items": items,
            }),
        )
        .await?;
    Ok(())
}

pub async fn write_entities(
    neo4j: &Neo4jClient,
    entities: &HashMap<String, MergedEntity>,
) -> Result<(), Box<dyn Error>> {
    if entities.is_empty() {
        return Ok(());
    }
    let items: Vec<serde_json::Value> = entities
        .values()
        .map(|entity| {
            serde_json::json!({
                "name": entity.name,
                "entity_type": entity.entity_type,
                "confidence": entity.confidence,
            })
        })
        .collect();

    neo4j
        .execute(
            "UNWIND $items AS item \
             MERGE (e:Entity {name: item.name}) \
             SET e.entity_type = item.entity_type, \
                 e.confidence = item.confidence, \
                 e.updated_at = datetime()",
            serde_json::json!({ "items": items }),
        )
        .await?;
    Ok(())
}

pub async fn write_chunk_mentions(
    neo4j: &Neo4jClient,
    taxonomy: &Taxonomy,
    source_type: &str,
    chunks: &[GraphChunk],
    entities: &HashMap<String, MergedEntity>,
) -> Result<usize, Box<dyn Error>> {
    let mut items: Vec<serde_json::Value> = Vec::new();
    for chunk in chunks {
        let names = candidate_names_for_chunk(taxonomy, &chunk.chunk_text, source_type, entities);
        for name in names {
            items.push(serde_json::json!({
                "name": name,
                "point_id": chunk.point_id,
            }));
        }
    }
    let mention_count = items.len();
    if !items.is_empty() {
        neo4j
            .execute(
                "UNWIND $items AS item \
                 MATCH (e:Entity {name: item.name}) \
                 MATCH (c:Chunk {point_id: item.point_id}) \
                 MERGE (e)-[:MENTIONED_IN]->(c)",
                serde_json::json!({ "items": items }),
            )
            .await?;
    }
    Ok(mention_count)
}

pub async fn write_entity_relationships(
    neo4j: &Neo4jClient,
    relationships: &[GraphRelationRecord],
) -> Result<(), Box<dyn Error>> {
    if relationships.is_empty() {
        return Ok(());
    }
    let items: Vec<serde_json::Value> = relationships
        .iter()
        .map(|r| {
            serde_json::json!({
                "source": r.source,
                "target": r.target,
                "relation": r.relation,
            })
        })
        .collect();

    neo4j
        .execute(
            "UNWIND $items AS item \
             MATCH (s:Entity {name: item.source}) \
             MATCH (t:Entity {name: item.target}) \
             MERGE (s)-[r:RELATES_TO]->(t) \
             SET r.relation = item.relation, \
                 r.updated_at = datetime()",
            serde_json::json!({ "items": items }),
        )
        .await?;
    Ok(())
}
