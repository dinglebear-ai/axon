use crate::crates::core::config::Config;
use crate::crates::core::http::http_client;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExtractedEntity {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExtractedRelationship {
    pub source: String,
    pub target: String,
    pub relation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExtractionResult {
    #[serde(default)]
    pub entities: Vec<ExtractedEntity>,
    #[serde(default)]
    pub relationships: Vec<ExtractedRelationship>,
}

pub fn build_extraction_request(model: &str, text: &str) -> Value {
    serde_json::json!({
        "model": model,
        "prompt": format!(
            "Extract software entities and relationships from the text below. Return only JSON matching the schema.\n\n{text}"
        ),
        "think": false,
        "stream": false,
        "format": {
            "type": "object",
            "properties": {
                "entities": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "name": {"type": "string"},
                            "type": {
                                "type": "string",
                                "enum": ["technology", "service", "project", "concept", "person", "organization", "language", "framework", "library", "tool"]
                            }
                        },
                        "required": ["name", "type"]
                    }
                },
                "relationships": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "source": {"type": "string"},
                            "target": {"type": "string"},
                            "relation": {
                                "type": "string",
                                "enum": ["USES", "DEPENDS_ON", "IMPLEMENTS", "WORKS_WITH", "SIMILAR_TO", "PART_OF", "RUNS_ON", "CONNECTS_TO"]
                            }
                        },
                        "required": ["source", "target", "relation"]
                    }
                }
            },
            "required": ["entities", "relationships"]
        }
    })
}

pub fn parse_extraction_response(
    json_str: &str,
) -> Result<ExtractionResult, Box<dyn std::error::Error>> {
    Ok(serde_json::from_str(json_str)?)
}

pub fn normalize_entity_name(name: &str) -> String {
    let mut normalized = name.trim().to_ascii_lowercase();
    if let Some((prefix, _)) = normalized.split_once("::") {
        normalized = prefix.to_string();
    }
    if let Some((prefix, _)) = normalized.split_once('(') {
        normalized = prefix.to_string();
    }
    if let Some((prefix, ext)) = normalized.rsplit_once('.')
        && matches!(
            ext,
            "rs" | "py" | "ts" | "tsx" | "js" | "jsx" | "json" | "toml" | "yaml" | "yml" | "md"
        )
    {
        normalized = prefix.to_string();
    }
    normalized.trim_end_matches("()").to_string()
}

pub fn resolve_type_conflict(type_a: &str, type_b: &str) -> String {
    fn rank(value: &str) -> usize {
        match value {
            "service" => 5,
            "framework" => 4,
            "library" => 4,
            "technology" => 3,
            "tool" => 3,
            "language" => 3,
            "project" => 2,
            "organization" => 2,
            "person" => 2,
            "concept" => 1,
            _ => 0,
        }
    }

    if rank(type_a) >= rank(type_b) {
        type_a.to_string()
    } else {
        type_b.to_string()
    }
}

pub async fn extract_entities_llm(
    cfg: &Config,
    text: &str,
) -> Result<ExtractionResult, Box<dyn std::error::Error>> {
    let client = http_client()?;
    let endpoint = format!("{}/api/generate", cfg.graph_llm_url.trim_end_matches('/'));
    let response = client
        .post(endpoint)
        .json(&build_extraction_request(&cfg.graph_llm_model, text))
        .send()
        .await?
        .error_for_status()?
        .json::<Value>()
        .await?;

    let json_str = response
        .get("response")
        .and_then(Value::as_str)
        .ok_or("missing Ollama response field")?;
    parse_extraction_response(json_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_extraction_request_has_required_fields() {
        let req = build_extraction_request("qwen3.5:4b", "Some document text about Rust");
        assert_eq!(req["model"], "qwen3.5:4b");
        assert_eq!(req["think"], false);
        assert_eq!(req["stream"], false);
        assert!(req["format"]["properties"]["entities"].is_object());
        assert!(req["format"]["properties"]["relationships"].is_object());
    }

    #[test]
    fn parse_extraction_response_valid() {
        let response = serde_json::json!({
            "entities": [
                {"name": "Tokio", "type": "technology"},
                {"name": "Axon", "type": "project"}
            ],
            "relationships": [
                {"source": "Axon", "target": "Tokio", "relation": "USES"}
            ]
        });
        let result = parse_extraction_response(&response.to_string()).unwrap();
        assert_eq!(result.entities.len(), 2);
        assert_eq!(result.relationships.len(), 1);
    }

    #[test]
    fn parse_extraction_response_empty_entities() {
        let response = r#"{"entities": [], "relationships": []}"#;
        let result = parse_extraction_response(response).unwrap();
        assert!(result.entities.is_empty());
    }

    #[test]
    fn normalize_entity_name_strips_suffixes() {
        assert_eq!(normalize_entity_name("tokio::new()"), "tokio");
        assert_eq!(normalize_entity_name("config.rs"), "config");
        assert_eq!(normalize_entity_name("PostgreSQL"), "postgresql");
    }

    #[test]
    fn resolve_type_conflict_most_specific_wins() {
        assert_eq!(resolve_type_conflict("technology", "concept"), "technology");
        assert_eq!(resolve_type_conflict("concept", "service"), "service");
    }
}
