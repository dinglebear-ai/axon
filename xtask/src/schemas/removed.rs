use std::collections::BTreeSet;

use anyhow::{Result, bail};

use super::artifact::SchemaArtifact;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedRestRoute {
    pub method: &'static str,
    pub path: &'static str,
    pub replacement: &'static str,
    pub operation_id: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedDtoField {
    pub dto: &'static str,
    pub field: &'static str,
    pub replacement: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedOperation {
    pub name: &'static str,
    pub replacement: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemovedSurfaceRegistry {
    pub cli_commands: &'static [RemovedOperation],
    pub mcp_actions: &'static [RemovedOperation],
    pub rest_routes: &'static [RemovedRestRoute],
    pub config_keys: &'static [RemovedOperation],
    pub dto_fields: &'static [RemovedDtoField],
    pub generated_clients: &'static [&'static str],
    pub generated_client_operations: &'static [RemovedOperation],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedSurfaceFinding {
    pub artifact_path: String,
    pub category: &'static str,
    pub surface: String,
    pub replacement: &'static str,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RemovedSurfaceReport {
    pub findings: Vec<RemovedSurfaceFinding>,
}

impl RemovedSurfaceReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }

    pub fn warning_messages(&self) -> Vec<String> {
        self.findings
            .iter()
            .map(|finding| {
                format!(
                    "{} exposes removed {} {}; replacement: {}",
                    finding.artifact_path, finding.category, finding.surface, finding.replacement
                )
            })
            .collect()
    }
}

pub fn removed_surface_registry() -> RemovedSurfaceRegistry {
    super::removed_registry::removed_surface_registry()
}

pub fn removed_surface_absence_report(artifacts: &[SchemaArtifact]) -> RemovedSurfaceReport {
    let registry = removed_surface_registry();
    let mut report = RemovedSurfaceReport::default();

    for artifact in artifacts {
        if artifact.path.to_string_lossy().ends_with(".json") {
            match serde_json::from_str::<serde_json::Value>(&artifact.content) {
                Ok(json) => check_json_artifact(&registry, artifact, &json, &mut report),
                Err(_) => continue,
            }
        }
    }

    report
}

pub fn assert_removed_surface_absent(report: &RemovedSurfaceReport) -> Result<()> {
    if report.is_clean() {
        return Ok(());
    }
    bail!("{}", report.warning_messages().join("\n"))
}

fn check_json_artifact(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    check_cli_commands(registry, artifact, json, report);
    check_mcp_actions(registry, artifact, json, report);
    check_rest_routes(registry, artifact, json, report);
    check_config_keys(registry, artifact, json, report);
    check_api_dto_fields(registry, artifact, json, report);
    check_generated_client_operations(registry, artifact, json, report);
}

fn check_cli_commands(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(commands) = collect_string_field(json.get("commands"), "name") else {
        return;
    };
    for removed in registry.cli_commands {
        if commands.contains(removed.name) {
            push_finding(
                report,
                artifact,
                "CLI command",
                removed.name,
                removed.replacement,
            );
        }
    }
}

fn check_mcp_actions(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(actions) = collect_string_field(json.get("actions"), "action") else {
        return;
    };
    for removed in registry.mcp_actions {
        if actions.contains(removed.name) {
            push_finding(
                report,
                artifact,
                "MCP action",
                removed.name,
                removed.replacement,
            );
        }
    }
}

fn check_rest_routes(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(routes) = json.get("routes").and_then(|routes| routes.as_array()) else {
        return;
    };
    let operation_ids =
        collect_string_field(json.get("routes"), "operation_id").unwrap_or_else(|| BTreeSet::new());
    for removed in registry.rest_routes {
        let route_present = routes.iter().any(|route| {
            route.get("method").and_then(|value| value.as_str()) == Some(removed.method)
                && route.get("path").and_then(|value| value.as_str()) == Some(removed.path)
        });
        if route_present {
            push_finding(
                report,
                artifact,
                "REST route",
                &format!("{} {}", removed.method, removed.path),
                removed.replacement,
            );
        }
        if operation_ids.contains(removed.operation_id) {
            push_finding(
                report,
                artifact,
                "REST operation",
                removed.operation_id,
                removed.replacement,
            );
        }
    }
}

fn check_config_keys(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(config_keys) = json.get("config_keys").and_then(|keys| keys.as_array()) else {
        return;
    };
    for key in config_keys {
        for removed in registry.config_keys {
            let env_present =
                key.get("env_key").and_then(|value| value.as_str()) == Some(removed.name);
            let key_present = key.get("key").and_then(|value| value.as_str()) == Some(removed.name);
            if env_present || key_present {
                push_finding(
                    report,
                    artifact,
                    "config key",
                    removed.name,
                    removed.replacement,
                );
            }
        }
    }
}

fn check_api_dto_fields(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    let Some(defs) = json.get("$defs").and_then(|defs| defs.as_object()) else {
        return;
    };
    let mut removed_dtos = BTreeSet::new();
    for removed in registry.dto_fields {
        removed_dtos.insert(removed.dto);
    }

    for dto in removed_dtos {
        if defs.contains_key(dto) {
            push_finding(
                report,
                artifact,
                "DTO schema",
                dto,
                "SourceRequest, QueryRequest, or PruneSelector replacement DTO",
            );
        }
    }

    for removed in registry.dto_fields {
        let Some(properties) = defs
            .get(removed.dto)
            .and_then(|schema| schema.get("properties"))
            .and_then(|properties| properties.as_object())
        else {
            continue;
        };
        if properties.contains_key(removed.field) {
            push_finding(
                report,
                artifact,
                "DTO field",
                &format!("{}.{}", removed.dto, removed.field),
                removed.replacement,
            );
        }
    }
}

fn check_generated_client_operations(
    registry: &RemovedSurfaceRegistry,
    artifact: &SchemaArtifact,
    json: &serde_json::Value,
    report: &mut RemovedSurfaceReport,
) {
    if !is_generated_client_artifact(artifact, json) {
        return;
    }

    let mut operations = BTreeSet::new();
    collect_operation_names(json, &mut operations);
    for removed in registry.generated_client_operations {
        if operations.contains(removed.name) {
            push_finding(
                report,
                artifact,
                "generated client operation",
                removed.name,
                removed.replacement,
            );
        }
    }
}

fn collect_operation_names(value: &serde_json::Value, names: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                collect_operation_names(value, names);
            }
        }
        serde_json::Value::Object(map) => {
            for key in ["operation_id", "operationId", "operation", "method", "name"] {
                if let Some(name) = map.get(key).and_then(|value| value.as_str()) {
                    names.insert(name.to_owned());
                }
            }
            for key in ["operations", "methods", "endpoints"] {
                if let Some(value) = map.get(key) {
                    collect_operation_names(value, names);
                }
            }
        }
        _ => {}
    }
}

fn collect_string_field<'a>(
    value: Option<&'a serde_json::Value>,
    field: &str,
) -> Option<BTreeSet<&'a str>> {
    let values = value?.as_array()?;
    Some(
        values
            .iter()
            .filter_map(|item| item.get(field).and_then(|value| value.as_str()))
            .collect(),
    )
}

fn is_generated_client_artifact(artifact: &SchemaArtifact, json: &serde_json::Value) -> bool {
    let path = artifact.path.to_string_lossy();
    path.contains("generated")
        || path.contains("client")
        || path.contains("apps/web")
        || json.get("client").is_some()
        || json.get("operations").is_some()
}

fn push_finding(
    report: &mut RemovedSurfaceReport,
    artifact: &SchemaArtifact,
    category: &'static str,
    surface: &str,
    replacement: &'static str,
) {
    report.findings.push(RemovedSurfaceFinding {
        artifact_path: artifact.path.display().to_string(),
        category,
        surface: surface.to_owned(),
        replacement,
    });
}
