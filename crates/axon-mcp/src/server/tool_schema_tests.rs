use super::AxonMcpServer;
use super::server_authz;

fn axon_input_schema() -> serde_json::Value {
    let tools = AxonMcpServer::tool_router().list_all();
    let axon = tools
        .into_iter()
        .find(|tool| tool.name.as_ref() == "axon")
        .expect("axon tool is registered");
    axon.schema_as_json_value()
}

fn axon_tool_description() -> String {
    let tools = AxonMcpServer::tool_router().list_all();
    tools
        .into_iter()
        .find(|tool| tool.name.as_ref() == "axon")
        .and_then(|tool| tool.description.map(|d| d.into_owned()))
        .expect("axon tool publishes a description")
}

/// Extract the comma-separated action list out of the description's
/// `"Actions: a, b, c."` sentence. Deliberately narrow (rather than a
/// whole-description substring search) because the description also contains
/// non-action prose and transport notes; action absence is asserted directly
/// against the published enum below.
fn axon_tool_actions_sentence(description: &str) -> std::collections::BTreeSet<&str> {
    let after_prefix = description
        .split_once("Actions: ")
        .map(|(_, rest)| rest)
        .expect("description should contain an `Actions: ...` sentence");
    let list = after_prefix
        .split_once('.')
        .map(|(list, _)| list)
        .unwrap_or(after_prefix);
    list.split(',').map(str::trim).collect()
}

/// The `#[tool(description = ...)]` free-text string is the primary
/// agent-facing summary from MCP tool discovery (`tools/list`), separate
/// from the machine-checked `properties.action.enum` in the input schema
/// (already covered by `axon_tool_input_schema_publishes_action_enum_from_tools_list`).
/// Its `Actions:` sentence must list exactly the real dispatchable actions —
/// no more, no less — or a caller reading only the description (not the
/// schema) will try actions that 400/403, or miss real ones entirely. Guards
/// against the drift found in the #298 alignment audit, where the
/// description advertised `domains`/`sources`/`stats` (all
/// explicitly rejected by `server.rs`'s dispatch match) while omitting real
/// actions like `jobs`, `resolve`, `capabilities`, `providers`, `prune`,
/// `watch`, and `graph`.
#[test]
fn axon_tool_description_actions_sentence_matches_real_action_set() {
    let description = axon_tool_description();
    let listed = axon_tool_actions_sentence(&description);
    let expected: std::collections::BTreeSet<&str> =
        server_authz::mcp_action_names().into_iter().collect();

    assert_eq!(
        listed, expected,
        "axon tool description's `Actions:` sentence should list exactly the real \
         MCP_ACTION_SPECS action set (server_authz::mcp_action_names())"
    );
}

#[test]
fn axon_tool_input_schema_publishes_action_enum_from_tools_list() {
    let schema = axon_input_schema();
    let action_enum = schema
        .pointer("/properties/action/enum")
        .and_then(serde_json::Value::as_array)
        .expect("tools/list inputSchema publishes properties.action.enum");

    for action in [
        "source", "extract", "retrieve", "ask", "query", "status", "jobs", "memory",
    ] {
        assert!(
            action_enum
                .iter()
                .any(|value| value.as_str() == Some(action)),
            "action enum should include {action}"
        );
    }

    // The legacy indexing actions were folded into `source` and must not appear
    // anywhere in the published MCP schema.
    for removed in [
        "crawl",
        "scrape",
        "embed",
        "ingest",
        "code_search",
        "vertical_scrape",
        "dedupe",
        "purge",
    ] {
        assert!(
            !action_enum
                .iter()
                .any(|value| value.as_str() == Some(removed)),
            "action enum must NOT include removed action {removed}"
        );
    }

    let expected = server_authz::mcp_action_names();
    let actual = action_enum
        .iter()
        .map(|value| value.as_str().expect("action enum entries are strings"))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(!actual.contains(&"debug"));
    // `watch` (issue #298 WS-B) is now a real dispatched MCP action —
    // `list`/`get`/`update`/`pause`/`resume`/`delete` over the source-request-
    // backed watch store. See `handlers_watch.rs`.
    assert!(actual.contains(&"watch"));
    assert!(actual.contains(&"reset"));
    assert!(actual.contains(&"collections"));
    assert!(actual.contains(&"uploads"));
    assert!(actual.contains(&"chat"));
    assert!(actual.contains(&"artifacts"));
}

#[test]
fn mcp_schema_exposes_only_canonical_prune_and_admin_subactions() {
    let schema = axon_input_schema();
    assert_eq!(
        schema.pointer("/x-axon-subactions/prune").unwrap(),
        &serde_json::json!(["plan", "exec"])
    );
    assert!(schema.pointer("/x-axon-subactions/reset").is_some());
    assert!(schema.pointer("/x-axon-subactions/collections").is_some());
    assert_eq!(
        schema.pointer("/x-axon-subactions/uploads").unwrap(),
        &serde_json::json!(["abort", "complete", "create", "get", "list", "put_content"])
    );
    let rendered = serde_json::to_string(&schema).unwrap();
    assert!(!rendered.contains("collection-prune convenience"));
    assert!(!rendered.contains("targeted purge"));
}

#[test]
fn mcp_schema_includes_source() {
    let schema = axon_input_schema();
    let action_enum = schema
        .pointer("/properties/action/enum")
        .and_then(serde_json::Value::as_array)
        .expect("tools/list inputSchema publishes properties.action.enum");
    assert!(
        action_enum
            .iter()
            .any(|value| value.as_str() == Some("source")),
        "action enum should include source"
    );
    let properties = schema
        .pointer("/properties")
        .and_then(serde_json::Value::as_object)
        .expect("tools/list inputSchema has top-level properties");
    for field in ["source", "scope"] {
        assert!(
            properties.contains_key(field),
            "top-level properties should include flattened source field `{field}`"
        );
    }
}

#[test]
fn mcp_schema_documents_source_required_fields() {
    let schema = axon_input_schema();
    let required = schema
        .pointer("/x-axon-required-fields/source")
        .and_then(serde_json::Value::as_array)
        .expect("source required field metadata is present");
    assert!(
        required
            .iter()
            .any(|value| value.as_str() == Some("source")),
        "source should document `source` as required"
    );
}

#[test]
fn mcp_schema_documents_jobs_subactions() {
    let schema = axon_input_schema();
    let subactions = schema
        .pointer("/x-axon-subactions/jobs")
        .and_then(serde_json::Value::as_array)
        .expect("jobs subaction metadata is present");
    for expected in [
        "list", "get", "events", "cancel", "retry", "recover", "cleanup", "clear",
    ] {
        assert!(
            subactions
                .iter()
                .any(|value| value.as_str() == Some(expected)),
            "jobs subactions should include {expected}"
        );
    }
}

#[test]
fn mcp_schema_omits_removed_indexing_surface() {
    // Removed names must be absent from action-bearing surfaces. Field names are
    // intentionally checked separately: `source.embed` is still a valid option,
    // even though the old top-level `embed` action is gone.
    let schema = axon_input_schema();
    let removed_actions = [
        "crawl",
        "scrape",
        "embed",
        "ingest",
        "code_search",
        "vertical_scrape",
        "dedupe",
        "purge",
    ];

    let action_enum = schema
        .pointer("/properties/action/enum")
        .and_then(serde_json::Value::as_array)
        .expect("tools/list inputSchema publishes properties.action.enum");
    for removed in removed_actions {
        assert!(
            !action_enum
                .iter()
                .any(|value| value.as_str() == Some(removed)),
            "action enum must not list removed action {removed}"
        );
    }

    let branches = schema
        .pointer("/oneOf")
        .and_then(serde_json::Value::as_array)
        .expect("schema oneOf branches are present");
    for branch in branches {
        let action = branch.pointer("/properties/action/const");
        for removed in removed_actions {
            assert_ne!(
                action.and_then(serde_json::Value::as_str),
                Some(removed),
                "oneOf branch must not advertise removed action {removed}"
            );
        }
    }

    let metadata = schema
        .pointer("/x-axon-action-metadata")
        .and_then(serde_json::Value::as_array)
        .expect("action metadata is present");
    for entry in metadata {
        let name = entry.pointer("/name").and_then(serde_json::Value::as_str);
        for removed in removed_actions {
            assert_ne!(
                name,
                Some(removed),
                "action metadata must not advertise removed action {removed}"
            );
        }
    }

    // `x-axon-subactions` must no longer list removed action families.
    let subactions = schema
        .pointer("/x-axon-subactions")
        .and_then(serde_json::Value::as_object)
        .expect("subaction metadata present");
    for removed in removed_actions {
        assert!(
            !subactions.contains_key(removed),
            "subaction metadata must not list removed family {removed}"
        );
    }
}

#[test]
fn mcp_schema_job_kind_filters_migration_only_families() {
    let schema = axon_input_schema();
    let defs = schema
        .pointer("/$defs")
        .and_then(serde_json::Value::as_object)
        .expect("schema defs are present");
    let mut found_job_kind_def = false;
    for (name, value) in defs {
        if !name.contains("JobKind") {
            continue;
        }
        found_job_kind_def = true;
        let serialized = serde_json::to_string(value).expect("serialize JobKind def");
        for removed in ["\"crawl\"", "\"embed\"", "\"ingest\""] {
            assert!(
                !serialized.contains(removed),
                "{name} must not advertise migration-only job kind {removed}"
            );
        }
    }
    assert!(
        found_job_kind_def,
        "schema should include a jobs.kind definition"
    );
}

#[test]
fn removed_crawl_fixture_is_outside_mcp_action_enum() {
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/schema/removed_crawl.invalid.json"
    ))
    .expect("fixture json");
    let action = fixture
        .get("action")
        .and_then(serde_json::Value::as_str)
        .expect("fixture has action");
    let schema = axon_input_schema();
    let action_enum = schema
        .pointer("/properties/action/enum")
        .and_then(serde_json::Value::as_array)
        .expect("tools/list inputSchema publishes properties.action.enum");
    assert!(
        !action_enum
            .iter()
            .any(|value| value.as_str() == Some(action)),
        "removed crawl action must be rejected before handler dispatch"
    );
}

#[test]
fn axon_tool_input_schema_flattens_per_action_fields_to_top_level() {
    let schema = axon_input_schema();
    let properties = schema
        .pointer("/properties")
        .and_then(serde_json::Value::as_object)
        .expect("tools/list inputSchema has top-level properties");

    // Representative per-action fields must be visible to clients that only
    // read top-level properties (Codex, mcporter signatures, codemode dts).
    for field in [
        "query",
        "url",
        "urls",
        "job_id",
        "response_mode",
        "collection",
        "limit",
        "explain",
        "source",
        "scope",
    ] {
        assert!(
            properties.contains_key(field),
            "top-level properties should include flattened field `{field}`"
        );
    }

    // Only `action` may be required at the top level — lifted fields are
    // optional supersets; per-action requirements stay in the oneOf branches.
    let required = schema
        .pointer("/required")
        .and_then(serde_json::Value::as_array)
        .expect("top-level required present");
    assert_eq!(required.len(), 1);
    assert_eq!(required[0].as_str(), Some("action"));

    // The strict oneOf validation contract must survive flattening.
    assert!(
        schema
            .pointer("/oneOf")
            .and_then(serde_json::Value::as_array)
            .is_some_and(|branches| !branches.is_empty()),
        "oneOf branches remain for per-action validation"
    );
}

#[test]
fn flattened_fields_annotate_applicable_actions() {
    let schema = axon_input_schema();

    let query_actions = schema
        .pointer("/properties/query/x-axon-actions")
        .and_then(serde_json::Value::as_array)
        .expect("lifted `query` field lists applicable actions");
    for action in ["query", "search", "memory"] {
        assert!(
            query_actions
                .iter()
                .any(|value| value.as_str() == Some(action)),
            "`query` field should apply to action {action}"
        );
    }
    assert!(
        !query_actions
            .iter()
            .any(|value| value.as_str() == Some("status")),
        "`query` field should not claim to apply to `status`"
    );
}

#[test]
fn flattened_fields_with_conflicting_shapes_become_unions() {
    let schema = axon_input_schema();
    // `limit` is i64 in job-list requests and usize in query requests —
    // the lifted property must union the distinct shapes, not pick one.
    let limit = schema
        .pointer("/properties/limit")
        .expect("lifted `limit` field present");
    let variants = limit
        .pointer("/anyOf")
        .and_then(serde_json::Value::as_array)
        .expect("conflicting `limit` shapes union under anyOf");
    assert!(
        variants.len() >= 2,
        "limit should carry at least two distinct shapes, got {variants:?}"
    );
}

#[test]
fn injected_action_and_subaction_win_over_flattened_branch_fields() {
    let schema = axon_input_schema();
    // Branch-level `action` consts and per-family `subaction` refs must not
    // clobber the injected top-level enum/description.
    assert!(
        schema.pointer("/properties/action/enum").is_some(),
        "top-level action keeps its enum"
    );
    let subaction_type = schema
        .pointer("/properties/subaction/type")
        .and_then(serde_json::Value::as_str);
    assert_eq!(subaction_type, Some("string"));
}

#[test]
fn axon_tool_input_schema_documents_subaction_families() {
    let schema = axon_input_schema();
    let subactions = schema
        .pointer("/x-axon-subactions")
        .and_then(serde_json::Value::as_object)
        .expect("tools/list inputSchema documents subaction families");

    for (family, expected) in [
        ("extract", "start"),
        ("memory", "remember"),
        ("memory", "list"),
        ("memory", "search"),
        ("memory", "show"),
        ("memory", "link"),
        ("memory", "supersede"),
        ("memory", "context"),
    ] {
        let values = subactions
            .get(family)
            .and_then(serde_json::Value::as_array)
            .unwrap_or_else(|| panic!("{family} subactions should be listed"));
        assert!(
            values.iter().any(|value| value.as_str() == Some(expected)),
            "{family} subactions should include {expected}"
        );
    }

    let extract = subactions
        .get("extract")
        .and_then(serde_json::Value::as_array)
        .expect("extract subactions should be listed");
    assert_eq!(
        extract,
        &vec![serde_json::json!("start")],
        "MCP extract must submit only; lifecycle is under action=jobs"
    );
}
