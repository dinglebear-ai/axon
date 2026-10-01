use serde_json::{Value, json};

pub(super) fn source_specific_field_schema(field: &str) -> Value {
    match field {
        "web_status_code" | "web_depth" | "session_turn_index" | "memory_importance" => {
            json!({ "type": "integer" })
        }
        "graph_confidence" => json!({ "type": "number" }),
        "manifest" => json!({ "type": "boolean" }),
        "graph_node_ids" | "graph_edge_ids" => {
            json!({ "type": "array", "items": { "type": "string" } })
        }
        _ => json!({ "type": "string" }),
    }
}

pub(super) fn shared_chunk_field_schema(field: &str) -> Option<Value> {
    Some(match field {
        "source_item_aliases" | "item_canonical_uri_aliases" | "source_path_prefixes" => {
            let mut schema = json!({ "type": "array", "items": { "type": "string", "minLength": 1, "pattern": r"\S" }, "x-qdrant-index": "keyword" });
            if field == "source_path_prefixes" {
                schema["minItems"] = json!(1);
            }
            schema
        }
        "code_symbol_aliases" => {
            json!({ "type": "array", "items": { "type": "string", "minLength": 1, "pattern": r"\S" } })
        }
        "code_syntax_recovered" => json!({ "type": "boolean" }),
        "code_ast_status" => {
            json!({ "type": "string", "enum": ["parsed", "partial", "unsupported", "failed"] })
        }
        "code_grammar" => json!({ "type": "string", "minLength": 1, "pattern": r"\S" }),
        "code_symbol_count" => json!({ "type": "integer", "minimum": 0 }),
        "additional_source_ranges" => {
            json!({"type":"array","minItems":1,"items":{"$ref":"#/$defs/SourceRange"}})
        }
        "code_symbol_source_range" => json!({ "$ref": "#/$defs/SourceRange" }),
        _ => return None,
    })
}
