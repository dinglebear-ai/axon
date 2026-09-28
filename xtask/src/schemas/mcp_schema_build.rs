//! Export actual canonical requests, never a fabricated body envelope.
use super::{
    LIVE_ACTIONS, SubactionKind, live_action_names, request_schema_for, typed_subaction_variants,
};
use serde_json::{Value, json};

fn subaction_def_name(action: &str) -> String {
    let mut chars = action.chars();
    let first = chars.next().map(|c| c.to_ascii_uppercase());
    format!(
        "{}{}Subaction",
        first.into_iter().collect::<String>(),
        chars.collect::<String>()
    )
}

pub(crate) fn def_pairs() -> Vec<(&'static str, Value)> {
    let mut pairs: Vec<_> = LIVE_ACTIONS
        .iter()
        .map(|spec| (spec.request_dto, request_schema_for(spec.request_dto)))
        .collect();
    pairs.push((
        "AxonToolInput",
        axon_mcp::schema_registry::canonical_request_schema(),
    ));
    pairs
}

pub(crate) fn subaction_def_pairs() -> Vec<(String, Value)> {
    LIVE_ACTIONS
        .iter()
        .filter_map(|spec| match spec.subaction {
            SubactionKind::None => None,
            SubactionKind::TypedEnum => Some((
                subaction_def_name(spec.name),
                json!({"type":"string", "enum":typed_subaction_variants(spec.name)}),
            )),
        })
        .collect()
}

/// Retain the old named definition as an alias to the real flat request.
pub(crate) fn discriminator_rules() -> Value {
    json!({"$ref":"#/$defs/AxonToolInput", "description":"Compatibility alias for canonical flat action/subaction request validation"})
}

pub(crate) fn deferred_actions_value() -> Value {
    Value::Array(super::deferred_actions())
}
pub(crate) fn action_enum_def() -> Value {
    json!({"type":"string", "enum":live_action_names()})
}
