use super::similarity::SimilarityEdge;
use super::taxonomy::Taxonomy;
use crate::crates::core::config::Config;
use crate::crates::core::neo4j::Neo4jClient;
use serde_json::Value;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq)]
pub struct GraphContext {
    pub context_text: String,
    pub entities: Vec<GraphEntity>,
    pub neighbor_chunk_ids: Vec<String>,
    pub similar_docs: Vec<SimilarityEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEntity {
    pub name: String,
    pub entity_type: String,
    pub description: String,
    pub relations: Vec<GraphRelation>,
    pub doc_count: u32,
    pub chunk_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphRelation {
    pub relation: String,
    pub target_name: String,
    pub target_type: String,
}

pub fn format_entity(entity: &GraphEntity) -> String {
    let mut lines = vec![format!(
        "Entity: {} ({}) | docs={} chunks={}",
        entity.name, entity.entity_type, entity.doc_count, entity.chunk_count
    )];
    if !entity.description.is_empty() {
        lines.push(format!("Description: {}", entity.description));
    }
    for relation in &entity.relations {
        lines.push(format!(
            "- {} -> {} ({})",
            relation.relation, relation.target_name, relation.target_type
        ));
    }
    lines.join("\n")
}

pub fn format_context_text(entities: &[GraphEntity], max_chars: usize) -> String {
    if entities.is_empty() || max_chars == 0 {
        return String::new();
    }

    let header = "Graph Context:";
    if max_chars < header.len() {
        return String::new();
    }

    let mut out = format!("{header}\n");
    for entity in entities {
        let block = format_entity(entity);
        let candidate = if out.ends_with('\n') {
            format!("{out}\n{block}\n")
        } else {
            format!("{out}\n\n{block}\n")
        };
        if candidate.len() > max_chars {
            break;
        }
        out = candidate;
    }
    let trimmed = out.trim().to_string();
    if trimmed.len() > max_chars {
        String::new()
    } else {
        trimmed
    }
}

pub fn sort_entities_by_priority(entities: &mut [GraphEntity]) {
    entities.sort_by(|left, right| {
        right
            .relations
            .len()
            .cmp(&left.relations.len())
            .then_with(|| right.doc_count.cmp(&left.doc_count))
            .then_with(|| right.chunk_count.cmp(&left.chunk_count))
            .then_with(|| left.name.cmp(&right.name))
    });
}

pub async fn build_graph_context(
    cfg: &Config,
    neo4j: &Neo4jClient,
    chunk_texts: &[String],
) -> Result<GraphContext, Box<dyn std::error::Error>> {
    if chunk_texts.is_empty() {
        return Ok(GraphContext {
            context_text: String::new(),
            entities: vec![],
            neighbor_chunk_ids: vec![],
            similar_docs: vec![],
        });
    }

    let taxonomy = Taxonomy::resolve(&cfg.graph_taxonomy_path)?;

    let mut entity_names = BTreeSet::new();
    for text in chunk_texts {
        for candidate in taxonomy.extract_entities(text, "crawl") {
            entity_names.insert(candidate.name);
        }
    }

    if entity_names.is_empty() {
        return Ok(GraphContext {
            context_text: String::new(),
            entities: vec![],
            neighbor_chunk_ids: vec![],
            similar_docs: vec![],
        });
    }

    let rows = neo4j
        .query(
            "MATCH (e:Entity) WHERE e.name IN $entity_names \
             WITH e \
             OPTIONAL MATCH (e)-[r]-(neighbor:Entity) \
             WITH e, collect({name: neighbor.name, type: neighbor.entity_type, relation: coalesce(r.relation, type(r))}) AS neighbors \
             OPTIONAL MATCH (e)-[:MENTIONED_IN]->(c:Chunk)-[:BELONGS_TO]->(d:Document) \
             WITH e, neighbors, count(DISTINCT d) AS doc_count, count(c) AS chunk_count \
             RETURN e.name AS name, e.entity_type AS type, coalesce(e.description, '') AS description, \
                    neighbors, doc_count, chunk_count \
             ORDER BY size(neighbors) DESC",
            serde_json::json!({ "entity_names": entity_names.into_iter().collect::<Vec<_>>() }),
        )
        .await
        .map_err(|e| e as Box<dyn std::error::Error>)?;

    let mut entities = rows
        .into_iter()
        .filter_map(parse_graph_entity_row)
        .collect::<Vec<_>>();
    sort_entities_by_priority(&mut entities);
    let context_text = format_context_text(&entities, cfg.graph_context_max_chars);

    Ok(GraphContext {
        context_text,
        entities,
        neighbor_chunk_ids: vec![],
        similar_docs: vec![],
    })
}

fn parse_graph_entity_row(row: Value) -> Option<GraphEntity> {
    let data = row.get("row").cloned().unwrap_or(row);
    let cells = data.as_array()?;
    let name = cells.first()?.as_str()?.to_string();
    let entity_type = cells
        .get(1)
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let description = cells
        .get(2)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let relations = cells
        .get(3)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(GraphRelation {
                        relation: item.get("relation")?.as_str()?.to_string(),
                        target_name: item.get("name")?.as_str()?.to_string(),
                        target_type: item
                            .get("type")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let doc_count = cells.get(4).and_then(Value::as_u64).unwrap_or(0) as u32;
    let chunk_count = cells.get(5).and_then(Value::as_u64).unwrap_or(0) as u32;

    Some(GraphEntity {
        name,
        entity_type,
        description,
        relations,
        doc_count,
        chunk_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_graph_context_empty() {
        let ctx = GraphContext {
            context_text: String::new(),
            entities: vec![],
            neighbor_chunk_ids: vec![],
            similar_docs: vec![],
        };
        assert!(ctx.context_text.is_empty());
    }

    #[test]
    fn format_context_respects_budget_smaller_than_header() {
        let entities = vec![GraphEntity {
            name: "Tokio".to_string(),
            entity_type: "technology".to_string(),
            description: String::new(),
            relations: vec![],
            doc_count: 1,
            chunk_count: 1,
        }];
        assert_eq!(format_context_text(&entities, 5), "");
    }

    #[test]
    fn format_entity_block() {
        let entity = GraphEntity {
            name: "Tokio".to_string(),
            entity_type: "technology".to_string(),
            description: "async runtime for Rust".to_string(),
            relations: vec![GraphRelation {
                relation: "USED_BY".to_string(),
                target_name: "axum".to_string(),
                target_type: "technology".to_string(),
            }],
            doc_count: 12,
            chunk_count: 47,
        };
        let text = format_entity(&entity);
        assert!(text.contains("Tokio"));
        assert!(text.contains("technology"));
        assert!(text.contains("USED_BY"));
        assert!(text.contains("axum"));
    }

    #[test]
    fn format_context_respects_char_budget() {
        let entities: Vec<GraphEntity> = (0..50)
            .map(|i| GraphEntity {
                name: format!("Entity{i}"),
                entity_type: "technology".to_string(),
                description: "A technology that does stuff and has a long description".to_string(),
                relations: vec![],
                doc_count: 1,
                chunk_count: 1,
            })
            .collect();
        let text = format_context_text(&entities, 500);
        assert!(
            text.len() <= 500,
            "Should respect budget, got {}",
            text.len()
        );
        assert!(!text.ends_with("Ent"));
    }

    #[test]
    fn entities_prioritized_by_relation_count() {
        let mut entities = vec![
            GraphEntity {
                name: "A".to_string(),
                entity_type: "technology".to_string(),
                description: String::new(),
                relations: vec![],
                doc_count: 100,
                chunk_count: 100,
            },
            GraphEntity {
                name: "B".to_string(),
                entity_type: "technology".to_string(),
                description: String::new(),
                relations: vec![
                    GraphRelation {
                        relation: "USES".to_string(),
                        target_name: "C".to_string(),
                        target_type: "technology".to_string(),
                    },
                    GraphRelation {
                        relation: "USES".to_string(),
                        target_name: "D".to_string(),
                        target_type: "technology".to_string(),
                    },
                ],
                doc_count: 1,
                chunk_count: 1,
            },
        ];
        sort_entities_by_priority(&mut entities);
        assert_eq!(entities[0].name, "B");
    }
}
