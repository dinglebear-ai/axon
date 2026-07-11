//! Assembles the contracted `AxonToolInput` / `AxonToolResponse` JSON Schema
//! bundle from [`super::LIVE_ACTIONS`] per
//! `docs/pipeline-unification/schemas/mcp-tool-schema.md`. Kept in a
//! separate file from `mcp_action_registry.rs` to stay under the monolith
//! line cap; `use super::*` gives it the registry types.

use serde_json::{Map, Value, json};

use super::{
    ActionSpec, LIVE_ACTIONS, SubactionKind, live_action_names, request_schema_for,
    typed_subaction_variants,
};

fn subaction_def_name(action: &str) -> String {
    let mut chars = action.chars();
    let first = chars.next().map(|c| c.to_ascii_uppercase());
    let rest: String = chars.collect();
    format!("{}{}Subaction", first.into_iter().collect::<String>(), rest)
}

fn subaction_variants(spec: &ActionSpec) -> Option<Vec<String>> {
    match spec.subaction {
        SubactionKind::None => None,
        SubactionKind::TypedEnum => Some(typed_subaction_variants(spec.name)),
        SubactionKind::InformalStrings(values) => {
            Some(values.iter().map(|v| (*v).to_string()).collect())
        }
    }
}

/// Every request-DTO and subaction-enum `$defs` entry, keyed by name. Built
/// as plain `(name, schemars-or-hand-built schema)` pairs — callers route
/// these through `schema_json::schema_defs` for `$ref` namespacing exactly
/// like every other schema family (see `families::api_schema_defs`,
/// `provider_capabilities::provider_schema_defs`).
pub(crate) fn def_pairs() -> Vec<(&'static str, Value)> {
    let mut pairs: Vec<(&'static str, Value)> = Vec::new();
    for spec in LIVE_ACTIONS {
        pairs.push((spec.request_dto, request_schema_for(spec.request_dto)));
    }
    pairs
}

/// Subaction enum defs, keyed by leaked `'static` def names (grouped
/// actions only). Kept separate from `def_pairs` because the def name is
/// computed, not a literal.
pub(crate) fn subaction_def_pairs() -> Vec<(String, Value)> {
    LIVE_ACTIONS
        .iter()
        .filter_map(|spec| {
            subaction_variants(spec).map(|variants| {
                (
                    subaction_def_name(spec.name),
                    json!({ "type": "string", "enum": variants }),
                )
            })
        })
        .collect()
}

/// One `if`/`then` discriminator branch per live action, referencing the
/// action's request DTO def and (when grouped) its subaction enum def.
/// Built with raw `#/$defs/...` strings — must be merged into the final
/// `$defs` map *after* `schema_json::schema_defs`'s `$ref`-namespacing pass,
/// never passed through it itself.
pub(crate) fn discriminator_rules() -> Value {
    let branches: Vec<Value> = LIVE_ACTIONS
        .iter()
        .map(|spec| {
            let mut then = Map::new();
            then.insert(
                "properties".to_string(),
                json!({ "body": { "$ref": format!("#/$defs/{}", spec.request_dto) } }),
            );
            match spec.subaction {
                SubactionKind::None => {
                    then.insert("not".to_string(), json!({ "required": ["subaction"] }));
                }
                _ => {
                    then.insert("required".to_string(), json!(["subaction"]));
                    then.insert(
                        "properties".to_string(),
                        json!({
                            "body": { "$ref": format!("#/$defs/{}", spec.request_dto) },
                            "subaction": { "$ref": format!("#/$defs/{}", subaction_def_name(spec.name)) }
                        }),
                    );
                }
            }
            json!({
                "if": {
                    "properties": { "action": { "const": spec.name } },
                    "required": ["action"]
                },
                "then": Value::Object(then)
            })
        })
        .collect();
    json!({
        "type": "object",
        "description": "One if/then branch per live MCP action; constrains subaction \
                        requirement and the effective request DTO.",
        "oneOf": branches
    })
}

/// The root `AxonToolInput` discriminated envelope, per the contract's
/// "Root Input Schema" section.
pub(crate) fn root_input_schema() -> Value {
    json!({
        "title": "AxonToolInput",
        "type": "object",
        "required": ["action"],
        "properties": {
            "action": { "$ref": "#/$defs/Action" },
            "subaction": { "type": "string" },
            "source": { "type": "string" },
            "sources": { "type": "array", "items": { "type": "string" } },
            "query": { "type": "string" },
            "question": { "type": "string" },
            "body": { "type": "object", "additionalProperties": true },
            "wait": { "type": "boolean", "default": false },
            "response_mode": { "$ref": "#/$defs/ResponseMode", "default": "auto" }
        },
        "allOf": [ { "$ref": "#/$defs/ActionDiscriminatorRules" } ],
        "unevaluatedProperties": true
    })
}

/// Contract actions with no live-runtime request DTO
/// (`super::deferred_actions`), exposed as `x-axon.deferred_actions` instead
/// of a fabricated schema.
pub(crate) fn deferred_actions_value() -> Value {
    Value::Array(super::deferred_actions())
}

pub(crate) fn action_enum_def() -> Value {
    json!({ "type": "string", "enum": live_action_names() })
}
