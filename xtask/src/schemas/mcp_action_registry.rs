//! Hand-maintained mirror of the **live** MCP action surface (issue #298
//! WS-F, `docs/pipeline-unification/schemas/mcp-tool-schema.md`).
//!
//! `crates/axon-mcp` is read-only territory for this generator: the real
//! dispatcher table (`MCP_ACTION_SPECS` in
//! `crates/axon-mcp/src/server/authz.rs`) is `pub(super)` and cannot be
//! imported directly. This module is the generator-side registry that
//! mirrors it by hand (the same pattern already used by
//! `xtask/src/schemas/cli_registry.rs` for the CLI family). Two safeguards
//! keep it from silently rotting:
//!
//! 1. `mcp_action_registry_tests.rs` calls the already-public
//!    `axon_mcp::required_scope_for(action, subaction)` oracle for every
//!    name in [`live_action_names`] (must resolve) and every name in
//!    [`known_non_live_action_names`] (must resolve to `__deny__`/removed).
//!    If a future edit to `MCP_ACTION_SPECS` adds/removes/rescoped an
//!    action without a matching edit here, that test fails.
//! 2. Shared action request DTOs are resolved from the real,
//!    schemars-derived `axon_api::mcp_schema` types. The two system requests
//!    owned privately by `axon-mcp` (`reset` and `collections`) are mirrored
//!    explicitly here and covered by focused generator expectations.
//!
//! Contract convergence direction: `docs/pipeline-unification/schemas/
//! mcp-tool-schema.md`'s target `Action` enum has 31 names; the live
//! dispatcher currently implements the 28 below. Names present only in the
//! contract are surfaced via [`deferred_action_names`] / `deferred_actions`
//! in the generated schema instead of fabricated request schemas.

use serde_json::{Value, json};

/// Subaction shape for a grouped action.
#[derive(Debug, Clone, Copy)]
pub(super) enum SubactionKind {
    /// Action takes no `subaction` (ungrouped).
    None,
    /// `subaction` is validated against a real schemars enum type. `variants`
    /// is produced from that enum's schema at generation time, never
    /// hand-typed (see `subaction_variants_for`).
    TypedEnum,
    /// `subaction` is accepted as a free string with an informal, documented
    /// value set (the DTO does not model it as a Rust enum).
    InformalStrings(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ActionSpec {
    /// Wire action name (matches `MCP_ACTION_SPECS[i].name` in
    /// `crates/axon-mcp/src/server/authz.rs`, read there for reference only).
    pub name: &'static str,
    pub description: &'static str,
    /// `"read" | "write" | "admin" | "info"` — mirrors `ActionScope::as_label`.
    pub scope: &'static str,
    pub mutates: bool,
    pub async_job: bool,
    /// The real `axon_api::mcp_schema` request DTO type name for this
    /// action, resolved to a schema via `request_schema_for`.
    pub request_dto: &'static str,
    pub subaction: SubactionKind,
}

/// The live action registry, mirroring `MCP_ACTION_SPECS` as read from
/// `crates/axon-mcp/src/server/authz.rs` (read-only reference; do not copy
/// scope changes here without re-reading that file, and do not edit that
/// file from this generator).
pub(super) const LIVE_ACTIONS: &[ActionSpec] = &[
    ActionSpec {
        name: "help",
        description: "List actions, subactions, defaults, and schema resource links",
        scope: "info",
        mutates: false,
        async_job: false,
        request_dto: "HelpRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "status",
        description: "Show unified jobs, watches, cleanup, totals, and service status",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "StatusRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "jobs",
        description: "List, inspect, page events, cancel, retry, recover, cleanup, or clear unified durable jobs",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "JobsRequest",
        subaction: SubactionKind::TypedEnum,
    },
    ActionSpec {
        name: "doctor",
        description: "Diagnose Axon service connectivity",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "DoctorRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "source",
        description: "Acquire and index one source (local path, git/web/feed/youtube/reddit/session/registry target) through the unified pipeline",
        scope: "write",
        mutates: true,
        async_job: true,
        request_dto: "SourceRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "query",
        description: "Run semantic vector search over indexed content",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "QueryRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "retrieve",
        description: "Fetch stored document chunks by URL",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "RetrieveRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "resolve",
        description: "Resolve source identity and adapter route without acquiring content",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "ResolveRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "capabilities",
        description: "Machine-readable server capability document: actions, scopes, providers",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "CapabilitiesRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "providers",
        description: "List or inspect provider capability/health",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "ProvidersRequest",
        subaction: SubactionKind::InformalStrings(&["list", "get"]),
    },
    ActionSpec {
        name: "search",
        description: "Run SearXNG/Tavily web search and optionally queue Source jobs for results",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "SearchRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "map",
        description: "Discover URLs for a site without scraping page content",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "MapRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "prune",
        description: "Plan or execute source, generation, or collection cleanup behind axon-prune",
        scope: "admin",
        mutates: true,
        async_job: false,
        request_dto: "PruneMcpRequest",
        subaction: SubactionKind::InformalStrings(&["plan", "exec"]),
    },
    ActionSpec {
        name: "collections",
        description: "List or inspect configured vector collections",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "CollectionsMcpRequest",
        subaction: SubactionKind::InformalStrings(&["list", "get"]),
    },
    ActionSpec {
        name: "reset",
        description: "Plan or execute an explicit clean-slate store reset",
        scope: "admin",
        mutates: true,
        async_job: false,
        request_dto: "ResetMcpRequest",
        subaction: SubactionKind::InformalStrings(&["plan", "exec"]),
    },
    ActionSpec {
        name: "uploads",
        description: "Stage, inspect, complete, list, or abort durable uploads",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "UploadsMcpRequest",
        subaction: SubactionKind::InformalStrings(&[
            "list",
            "create",
            "get",
            "put_content",
            "complete",
            "abort",
        ]),
    },
    ActionSpec {
        name: "ask",
        description: "Answer a question with RAG over indexed content",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "AskRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "evaluate",
        description: "Evaluate RAG quality against a baseline and judge diagnostics",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "EvaluateRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "suggest",
        description: "Suggest new documentation URLs to index",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "SuggestRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "research",
        description: "Run SearXNG/Tavily research with synthesis and auto-indexing",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "ResearchRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "screenshot",
        description: "Capture a full-page screenshot through headless Chrome",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "ScreenshotRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "brand",
        description: "Extract brand identity metadata from a URL",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "BrandRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "diff",
        description: "Compare two URLs for content, metadata, and link changes",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "DiffRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "extract",
        description: "Start async structured extraction jobs; use action=jobs for lifecycle",
        scope: "write",
        mutates: true,
        async_job: true,
        request_dto: "ExtractRequest",
        subaction: SubactionKind::TypedEnum,
    },
    ActionSpec {
        name: "memory",
        description: "Remember, search, and show persistent agent memory",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "MemoryRequest",
        subaction: SubactionKind::TypedEnum,
    },
    ActionSpec {
        name: "summarize",
        description: "Fetch URL context and summarize it with the configured LLM",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "SummarizeRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "endpoints",
        description: "Discover and optionally verify static site endpoints",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "EndpointsRequest",
        subaction: SubactionKind::None,
    },
    ActionSpec {
        name: "watch",
        description: "Create, list, inspect, execute, page history, update, pause, resume, or delete source-request-backed watches",
        scope: "write",
        mutates: true,
        async_job: false,
        request_dto: "WatchRequest",
        subaction: SubactionKind::TypedEnum,
    },
    ActionSpec {
        name: "graph",
        description: "Query the read-only SourceGraph: kinds, resolve, query, node, edge, source subgraph",
        scope: "read",
        mutates: false,
        async_job: false,
        request_dto: "GraphRequest",
        subaction: SubactionKind::TypedEnum,
    },
];

/// Action names that exist on the shared `axon_api::mcp_schema::AxonRequest`
/// enum (REST/CLI compatibility) but are rejected pre-dispatch by MCP authz
/// (`crates/axon-mcp/src/server.rs`'s removed/HTTP-only match arms) — read
/// there for reference. Used only by the drift test's negative-space check;
/// not part of the generated schema.
#[allow(dead_code)]
pub(super) const KNOWN_NON_LIVE_ACTIONS: &[&str] = &[
    "crawl",
    "embed",
    "ingest",
    "code_search",
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

/// Resolve the real, schemars-derived request schema for an action's
/// `request_dto` name. Panics on an unknown name — every `LIVE_ACTIONS`
/// entry above must resolve, and the sidecar test enforces that this stays
/// exhaustive.
pub(super) fn request_schema_for(request_dto: &str) -> Value {
    use axon_api::mcp_schema as m;
    match request_dto {
        "HelpRequest" => schemars::schema_for!(m::HelpRequest).into(),
        "StatusRequest" => schemars::schema_for!(m::StatusRequest).into(),
        "JobsRequest" => schemars::schema_for!(m::JobsRequest).into(),
        "DoctorRequest" => schemars::schema_for!(m::DoctorRequest).into(),
        "SourceRequest" => schemars::schema_for!(m::SourceRequest).into(),
        "QueryRequest" => schemars::schema_for!(m::QueryRequest).into(),
        "RetrieveRequest" => schemars::schema_for!(m::RetrieveRequest).into(),
        "ResolveRequest" => schemars::schema_for!(m::ResolveRequest).into(),
        "CapabilitiesRequest" => schemars::schema_for!(m::CapabilitiesRequest).into(),
        "ProvidersRequest" => schemars::schema_for!(m::ProvidersRequest).into(),
        "SearchRequest" => schemars::schema_for!(m::SearchRequest).into(),
        "MapRequest" => schemars::schema_for!(m::MapRequest).into(),
        "PruneMcpRequest" => schemars::schema_for!(m::PruneMcpRequest).into(),
        "CollectionsMcpRequest" => json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "subaction": { "type": "string", "enum": ["list", "get"], "default": "list" },
                "collection": { "type": ["string", "null"] },
                "prefix": { "type": ["string", "null"] },
                "limit": { "type": ["integer", "null"], "minimum": 0 },
                "cursor": { "type": ["string", "null"] },
                "response_mode": { "type": ["string", "null"], "enum": ["path", "inline", "both", "auto_inline", null] }
            }
        }),
        "ResetMcpRequest" => json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "subaction": { "type": "string", "enum": ["plan", "exec"], "default": "plan" },
                "stores": { "type": ["array", "null"], "items": { "type": "string" } },
                "collection": { "type": ["string", "null"] },
                "include_artifacts": { "type": ["boolean", "null"] },
                "include_config": { "type": ["boolean", "null"] },
                "reason": { "type": ["string", "null"] },
                "plan_id": { "type": ["string", "null"] },
                "confirm": { "type": ["boolean", "null"] },
                "response_mode": { "type": ["string", "null"], "enum": ["path", "inline", "both", "auto_inline", null] }
            }
        }),
        "UploadsMcpRequest" => json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "subaction": { "type": "string", "enum": ["list", "create", "get", "put_content", "complete", "abort"], "default": "list" },
                "upload_id": { "type": ["string", "null"] },
                "filename": { "type": ["string", "null"] },
                "content_type": { "type": ["string", "null"] },
                "size_bytes": { "type": ["integer", "null"], "minimum": 0 },
                "purpose": { "type": ["string", "null"], "enum": ["source_artifact", "import", "evaluation", null] },
                "sha256": { "type": ["string", "null"] },
                "source_hint": { "type": ["string", "null"] },
                "content": { "type": ["string", "null"] },
                "content_ref": { "type": ["object", "null"] },
                "source_options": { "type": ["object", "null"] },
                "reason": { "type": ["string", "null"] },
                "status": { "type": ["string", "null"], "enum": ["pending", "received", "completed", "aborted", "expired", null] },
                "limit": { "type": ["integer", "null"], "minimum": 0 },
                "cursor": { "type": ["string", "null"] },
                "response_mode": { "type": ["string", "null"], "enum": ["path", "inline", "both", "auto_inline", null] }
            }
        }),
        "AskRequest" => schemars::schema_for!(m::AskRequest).into(),
        "EvaluateRequest" => schemars::schema_for!(m::EvaluateRequest).into(),
        "SuggestRequest" => schemars::schema_for!(m::SuggestRequest).into(),
        "ResearchRequest" => schemars::schema_for!(m::ResearchRequest).into(),
        "ScreenshotRequest" => schemars::schema_for!(m::ScreenshotRequest).into(),
        "BrandRequest" => schemars::schema_for!(m::BrandRequest).into(),
        "DiffRequest" => schemars::schema_for!(m::DiffRequest).into(),
        "ExtractRequest" => schemars::schema_for!(m::ExtractRequest).into(),
        "MemoryRequest" => schemars::schema_for!(m::MemoryRequest).into(),
        "SummarizeRequest" => schemars::schema_for!(m::SummarizeRequest).into(),
        "EndpointsRequest" => schemars::schema_for!(m::EndpointsRequest).into(),
        "WatchRequest" => schemars::schema_for!(m::WatchRequest).into(),
        "GraphRequest" => schemars::schema_for!(m::GraphRequest).into(),
        other => panic!("mcp_action_registry: no request schema mapped for {other}"),
    }
}

/// Real subaction enum variants for `SubactionKind::TypedEnum` actions,
/// extracted from the schemars schema of the actual enum type (never
/// hand-typed), so a future variant add/remove is picked up automatically.
pub(super) fn typed_subaction_variants(action: &str) -> Vec<String> {
    use axon_api::mcp_schema as m;
    let schema: Value = match action {
        "jobs" => schemars::schema_for!(m::JobsSubaction).into(),
        "extract" => schemars::schema_for!(m::ExtractSubaction).into(),
        "memory" => schemars::schema_for!(m::MemorySubaction).into(),
        "watch" => schemars::schema_for!(m::WatchSubaction).into(),
        "graph" => schemars::schema_for!(m::GraphSubaction).into(),
        other => panic!("mcp_action_registry: no typed subaction enum mapped for {other}"),
    };
    enum_string_values(&schema)
}

/// Collects a Rust enum's string variants from its schemars-generated
/// schema. schemars 1.x renders an enum with per-variant doc comments as a
/// `oneOf` mixing a flat `enum` array (undocumented variants) with
/// individual `const` branches (documented variants) instead of a single
/// top-level `enum` — so both shapes must be checked.
fn enum_string_values(schema: &Value) -> Vec<String> {
    let mut values = Vec::new();
    if let Some(array) = schema.get("enum").and_then(|v| v.as_array()) {
        values.extend(array.iter().filter_map(|v| v.as_str().map(str::to_string)));
    }
    if let Some(branches) = schema.get("oneOf").and_then(|v| v.as_array()) {
        for branch in branches {
            if let Some(array) = branch.get("enum").and_then(|v| v.as_array()) {
                values.extend(array.iter().filter_map(|v| v.as_str().map(str::to_string)));
            }
            if let Some(v) = branch.get("const").and_then(|v| v.as_str()) {
                values.push(v.to_string());
            }
        }
    }
    values
}

#[path = "mcp_schema_build.rs"]
pub(super) mod build;

#[cfg(test)]
#[path = "mcp_action_registry_tests.rs"]
mod tests;
