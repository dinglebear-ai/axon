//! Canonical operation descriptors shared by routing, discovery and generation.
use super::{operation_schema, server_authz, tasks, tool_schema};
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    sync::{Arc, LazyLock},
};

#[derive(Debug, Clone)]
pub struct McpOperationDescriptor {
    pub name: String,
    pub action: &'static str,
    pub subaction: Option<String>,
    pub description: String,
    pub cost: &'static str,
    pub required_scope: Option<&'static str>,
    pub read_only: bool,
    pub destructive: bool,
    pub idempotent: Option<bool>,
    pub supports_tasks: bool,
    pub input_schema: Arc<JsonObject>,
}

pub fn operation_registry() -> &'static [McpOperationDescriptor] {
    static OPERATIONS: LazyLock<Vec<McpOperationDescriptor>> = LazyLock::new(build_operations);
    &OPERATIONS
}

pub(super) fn operation_named(name: &str) -> Option<&'static McpOperationDescriptor> {
    operation_registry()
        .iter()
        .find(|operation| operation.name == name)
}

fn build_operations() -> Vec<McpOperationDescriptor> {
    let root = tool_schema::canonical_request_schema();
    let mut names = BTreeSet::new();
    let mut operations = Vec::new();
    for spec in server_authz::MCP_ACTION_SPECS {
        let branches =
            operation_schema::subactions(&root, operation_schema::action_branch(&root, spec.name));
        let selectors: Vec<Option<String>> = if branches.is_empty() {
            vec![None]
        } else {
            branches.into_iter().map(Some).collect()
        };
        for subaction in selectors {
            let name = match subaction.as_deref() {
                None => spec.name.to_owned(),
                Some(selector) => format!("{}_{selector}", spec.name),
            };
            assert!(
                names.insert(name.clone()),
                "duplicate/colliding MCP operation {name}"
            );
            let selector = subaction.as_deref().unwrap_or("");
            let scope = server_authz::required_scope_with_mutates_if(
                spec.name,
                server_authz::required_scope_for(spec.name, selector),
            );
            assert_ne!(
                scope,
                Some("__deny__"),
                "canonical operation {name} is denied by the runtime policy"
            );
            let safety = leaf_safety(spec, subaction.as_deref());
            let description = match &subaction {
                Some(selector) => format!(
                    "{} {}. {}",
                    selector.replace('_', " "),
                    spec.name,
                    spec.description
                ),
                None => spec.description.to_owned(),
            };
            let supports_tasks = tasks::supports_operation(spec.name, selector);
            let input_schema = Arc::new(operation_schema::focused(
                &root,
                spec.name,
                subaction.as_deref(),
            ));
            operations.push(McpOperationDescriptor {
                name,
                action: spec.name,
                subaction,
                description,
                cost: if safety.read_only && spec.cost == "write" {
                    "cheap"
                } else {
                    spec.cost
                },
                required_scope: scope,
                read_only: safety.read_only,
                destructive: safety.destructive,
                idempotent: safety.idempotent,
                supports_tasks,
                input_schema,
            });
        }
    }
    operations.sort_by(|left, right| left.name.cmp(&right.name));
    operations
}

/// Effects are independent of scope. Plans can persist records; read-only admin
/// operations remain admin-gated. Future unknown leaves get conservative hints.
fn leaf_safety(
    spec: &server_authz::McpActionSpec,
    subaction: Option<&str>,
) -> server_authz::McpSafetyHints {
    let Some(selector) = subaction else {
        return spec.safety_hints();
    };
    let (read_only, destructive) = match spec.name {
        "jobs" => match selector {
            "list" | "get" | "status" | "events" | "stream" => (true, false),
            "retry" => (false, false),
            _ => (false, true),
        },
        "watch" => match selector {
            "list" | "get" | "status" | "history" => (true, false),
            "create" | "exec" => (false, false),
            _ => (false, true),
        },
        "memory" => match selector {
            "list" | "search" | "show" | "context" => (true, false),
            "remember" | "link" | "reinforce" | "export" => (false, false),
            _ => (false, true),
        },
        "prune" | "reset" => match selector {
            "get" => (true, false),
            "plan" => (false, false),
            _ => (false, true),
        },
        "uploads" => match selector {
            "list" | "get" => (true, false),
            "abort" => (false, true),
            _ => (false, false),
        },
        "codex" => match selector {
            "snapshot" | "resource" | "events" | "operations" => (true, false),
            "prepare" => (false, false),
            _ => (false, true),
        },
        "collections" | "artifacts" | "providers" | "graph" => (true, false),
        "extract" => (false, false),
        _ => (false, true),
    };
    server_authz::McpSafetyHints {
        read_only,
        destructive,
        idempotent: read_only.then_some(true),
    }
}

impl McpOperationDescriptor {
    pub fn metadata(&self) -> Value {
        json!({ "name": self.name, "action": self.action, "subaction": self.subaction,
            "description": self.description, "cost": self.cost, "required_scope": self.required_scope,
            "read_only": self.read_only, "destructive": self.destructive, "idempotent": self.idempotent,
            "task_support": if self.supports_tasks { "optional" } else { "forbidden" } })
    }
    pub(super) fn tool(&self) -> Tool {
        let annotations = ToolAnnotations::with_title(self.name.replace('_', " "))
            .read_only(self.read_only)
            .destructive(self.destructive);
        let annotations = match self.idempotent {
            Some(value) => annotations.idempotent(value),
            None => annotations,
        };
        let mut tool = Tool::new(
            self.name.clone(),
            self.description.clone(),
            Arc::clone(&self.input_schema),
        )
        .with_annotations(annotations);
        tool.meta = Some(
            serde_json::from_value(json!({ "axon": self.metadata() }))
                .expect("operation metadata object"),
        );
        tool
    }
}

#[cfg(test)]
#[path = "operations_tests.rs"]
mod tests;
