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

#[test]
fn axon_tool_input_schema_publishes_action_enum_from_tools_list() {
    let schema = axon_input_schema();
    let action_enum = schema
        .pointer("/properties/action/enum")
        .and_then(serde_json::Value::as_array)
        .expect("tools/list inputSchema publishes properties.action.enum");

    for action in [
        "crawl", "scrape", "retrieve", "ask", "query", "embed", "ingest", "status",
    ] {
        assert!(
            action_enum
                .iter()
                .any(|value| value.as_str() == Some(action)),
            "action enum should include {action}"
        );
    }

    let expected = server_authz::mcp_action_names();
    let actual = action_enum
        .iter()
        .map(|value| value.as_str().expect("action enum entries are strings"))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(!actual.contains(&"debug"));
    assert!(!actual.contains(&"watch"));
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
        "source_type",
        "target",
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
    for action in ["query", "search"] {
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
        ("crawl", "start"),
        ("extract", "status"),
        ("embed", "cancel"),
        ("ingest", "recover"),
        ("vertical_scrape", "capabilities"),
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
}
