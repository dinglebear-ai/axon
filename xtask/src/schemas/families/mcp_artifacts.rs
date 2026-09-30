//! Canonical aggregate and leaf MCP schema exports.
use super::*;

pub(super) fn generate(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Mcp);
    let inputs = source_inputs(root, spec.source_paths)?;

    // Real request-DTO + subaction-enum defs, namespaced via the shared
    // schema_defs()/rewrite_refs() helper exactly like every other family.
    let operation_defs: Vec<_> = axon_mcp::server::operation_registry()
        .iter()
        .map(|op| {
            (
                format!("Operation_{}_Input", op.name),
                Value::Object(op.input_schema.as_ref().clone()),
            )
        })
        .collect();
    let mut raw_defs: Vec<(&str, Value)> = mcp_action_registry::build::def_pairs();
    raw_defs.extend(
        operation_defs
            .iter()
            .map(|(name, schema)| (name.as_str(), schema.clone())),
    );
    raw_defs.push((
        "ResponseMode",
        schemars::schema_for!(axon_api::action::ResponseMode).into(),
    ));
    raw_defs.push((
        "AxonToolResponse",
        schemars::schema_for!(axon_mcp::schema::AxonToolResponse).into(),
    ));
    let mut defs = schema_defs(&raw_defs, None)
        .as_object()
        .cloned()
        .expect("schema_defs returns an object");

    // Subaction enum defs use computed (non-`&'static str`) names, so they're
    // inserted directly rather than through the `(&str, Value)` pair list.
    for (name, value) in mcp_action_registry::build::subaction_def_pairs() {
        defs.insert(name, value);
    }

    // `Action`, `ActionDiscriminatorRules`, and `AxonToolInput` all embed raw
    // `#/$defs/...` refs to the entries above by name — insert them *after*
    // schema_defs()'s ref-rewriting pass, never through it, or their refs
    // would be incorrectly namespaced.
    defs.insert(
        "Action".to_string(),
        mcp_action_registry::build::action_enum_def(),
    );
    defs.insert(
        "ActionDiscriminatorRules".to_string(),
        mcp_action_registry::build::discriminator_rules(),
    );

    let schema = json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": schema_id(SchemaFamily::Mcp),
        "title": spec.title,
        "description": "Generated Axon MCP tool schema contract artifact.",
        "type": "object",
        "additionalProperties": false,
        "$defs": Value::Object(defs),
        "x-axon": {
            "contract_version": "2026-06-30",
            "generated_by": "cargo xtask schemas mcp",
            "owner_crates": spec.owner_crates,
            "source_inputs": inputs,
            "clean_break": true,
            "live_action_count": mcp_action_registry::live_action_names().len(),
            "operation_count": axon_mcp::server::operation_registry().len(),
            "operations": axon_mcp::server::operation_registry().iter().map(|op| {
                let mut metadata = op.metadata();
                metadata["inputSchema"] = json!({"$ref":format!("#/$defs/Operation_{}_Input", op.name)});
                metadata
            }).collect::<Vec<_>>(),
            "projection_default": "legacy",
            "projection_values": ["legacy", "atomic", "both"],
            "deferred_actions": mcp_action_registry::build::deferred_actions_value(),
        }
    });
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(rel(spec.extra_json.unwrap().path), json_string(&schema)?),
        SchemaArtifact::new(rel(spec.markdown_path), mcp_markdown::mcp_markdown(&inputs)),
    ])
}
