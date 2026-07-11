use std::path::Path;

use anyhow::Result;
use serde_json::{Value, json};

use super::bundles::{registry_schema_bundle, schema_id};
use super::markdown_render::registry_markdown;
use super::{config_schema_registry, family_specs};
use crate::schemas::SchemaFamily;
use crate::schemas::artifact::SchemaArtifact;
use crate::schemas::rel;
use crate::schemas::removed;
use crate::schemas::schema_json::json_string;
use crate::schemas::source_input::source_inputs;

pub(crate) fn config_artifacts(root: &Path) -> Result<Vec<SchemaArtifact>> {
    let spec = family_specs::spec_for(SchemaFamily::Config);
    let inputs = source_inputs(root, spec.source_paths)?;
    let keys = config_key_records();
    let removed_config_keys: Vec<&str> = removed::removed_surface_registry()
        .config_keys
        .iter()
        .map(|op| op.name)
        .collect();
    let schema = registry_schema_bundle(
        schema_id(SchemaFamily::Config),
        spec.title,
        "cargo xtask schemas config",
        spec.owner_crates,
        &inputs,
        "config_keys",
        keys,
        &removed_config_keys,
    );
    let extra = spec.extra_json.unwrap();
    let env_vars = env_var_records();
    let env_schema = registry_schema_bundle(
        extra.id,
        extra.title,
        "cargo xtask schemas config",
        spec.owner_crates,
        &inputs,
        "env_vars",
        env_vars,
        &removed_config_keys,
    );
    Ok(vec![
        SchemaArtifact::new(rel(spec.json_path), json_string(&schema)?),
        SchemaArtifact::new(rel(extra.path), json_string(&env_schema)?),
        SchemaArtifact::new(
            rel(spec.markdown_path),
            registry_markdown("config", &inputs, "Config Keys"),
        ),
        SchemaArtifact::new(
            rel(spec.extra_markdown_path.unwrap()),
            registry_markdown("env", &inputs, "Env Variables"),
        ),
    ])
}

fn config_key_records() -> Vec<Value> {
    config_schema_registry::config_key_registry()
        .iter()
        .map(|key| {
            json!({
                "key": key.key,
                "section": key.section,
                "type": key.kind,
                "default": serde_json::from_str::<Value>(key.default_json)
                    .expect("config key default must be valid JSON"),
                "env_key": key.env_override,
                "owner_crate": key.owner_crate,
                "description": key.description,
                "secret": false,
                "restart_required": false,
                "removed": false,
                "replacement": Value::Null
            })
        })
        .collect()
}

fn env_var_records() -> Vec<Value> {
    config_schema_registry::env_var_registry()
        .iter()
        .map(|var| {
            json!({
                "name": var.name,
                "required": var.required,
                "secret": var.secret,
                "default": var.default,
                "owner_crate": var.owner_crate,
                "compose_usage": var.compose_usage,
                "validation": var.validation,
                "example_allowed": var.example_allowed,
                "description": var.description,
                "removed": false,
                "replacement": Value::Null
            })
        })
        .collect()
}
