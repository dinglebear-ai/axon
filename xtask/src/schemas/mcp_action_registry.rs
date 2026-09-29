//! Schema-generator view of the runtime MCP registry. Names, DTOs and finite
//! selectors are derived from axon-mcp; adding an action never requires an
//! independent generator inventory entry.
use serde_json::{Value, json};
use std::sync::LazyLock;

#[derive(Debug, Clone, Copy)]
pub(super) enum SubactionKind {
    None,
    TypedEnum,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ActionSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub scope: &'static str,
    pub mutates: bool,
    pub async_job: bool,
    pub request_dto: &'static str,
    pub subaction: SubactionKind,
}

pub(super) static LIVE_ACTIONS: LazyLock<Vec<ActionSpec>> = LazyLock::new(|| {
    axon_mcp::schema_registry::action_registry()
        .iter()
        .map(|spec| ActionSpec {
            name: spec.action,
            description: spec.description,
            scope: spec.required_scope,
            mutates: spec.mutates,
            async_job: spec.async_job,
            request_dto: spec.request_dto,
            subaction: if axon_mcp::schema_registry::subaction_variants(spec.action).is_empty() {
                SubactionKind::None
            } else {
                SubactionKind::TypedEnum
            },
        })
        .collect()
});

pub(super) const KNOWN_NON_LIVE_ACTIONS: &[&str] = &[
    "config",
    "vertical_scrape",
    "purge",
    "dedupe",
    "sources",
    "domains",
    "stats",
    "debug",
    "migrate",
    "setup",
];

/// The contract's target `Action` enum
/// (`docs/pipeline-unification/schemas/mcp-tool-schema.md`, "Action Enum"),
/// hardcoded here for the deferred-action delta. That doc is read-only
/// reference material; this list is this generator's own copy.
pub(super) const CONTRACT_ACTIONS: &[&str] = &[
    "codex",
    "source",
    "resolve",
    "map",
    "search",
    "query",
    "retrieve",
    "ask",
    "chat",
    "evaluate",
    "suggest",
    "research",
    "summarize",
    "endpoints",
    "brand",
    "diff",
    "screenshot",
    "extract",
    "memory",
    "jobs",
    "watch",
    "artifacts",
    "uploads",
    "prune",
    "collections",
    "graph",
    "providers",
    "reset",
    "status",
    "doctor",
    "capabilities",
    "help",
];

pub(crate) fn live_action_names() -> Vec<&'static str> {
    LIVE_ACTIONS.iter().map(|a| a.name).collect()
}

/// Contract actions with no exact-name match in the live registry. Not
/// fabricated as schemas — reported as a `deferred_actions` array in the
/// generated schema per the WS-F task contract ("Contract rows for actions
/// that do NOT exist in the runtime ... are OUT").
pub(super) fn deferred_actions() -> Vec<Value> {
    let live: std::collections::BTreeSet<&str> = live_action_names().into_iter().collect();
    CONTRACT_ACTIONS
        .iter()
        .filter(|name| !live.contains(*name))
        .map(|name| {
            json!({
                "action": name,
                "reason": "present in docs/pipeline-unification/schemas/mcp-tool-schema.md's \
                           target Action enum, absent from the live axon-mcp dispatcher \
                           (crates/axon-mcp/src/server.rs); no request DTO exists yet",
            })
        })
        .collect()
}

#[path = "mcp_action_registry/request_schemas.rs"]
mod request_schemas;
pub(super) use request_schemas::{request_schema_for, typed_subaction_variants};

#[path = "mcp_schema_build.rs"]
pub(super) mod build;

#[cfg(test)]
#[path = "mcp_action_registry_tests.rs"]
mod tests;
