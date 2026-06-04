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
