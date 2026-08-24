use serde_json::{Value, json};

pub(crate) fn request_schema_for(request_dto: &str) -> Value {
    use axon_api::mcp_schema as m;
    match request_dto {
        "ScrapeRequest" => schemars::schema_for!(axon_api::ScrapeRequest).into(),
        "CrawlRequest" => schemars::schema_for!(axon_api::CrawlRequest).into(),
        "EmbedRequest" => schemars::schema_for!(axon_api::EmbedRequest).into(),
        "IngestRequest" => schemars::schema_for!(axon_api::IngestRequest).into(),
        "CodeSearchRequest" => schemars::schema_for!(axon_api::CodeSearchRequest).into(),
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
            "type": "object", "additionalProperties": false,
            "properties": {
                "subaction": { "type": "string", "enum": ["list", "get"], "default": "list" },
                "collection": { "type": ["string", "null"] }, "prefix": { "type": ["string", "null"] },
                "limit": { "type": ["integer", "null"], "minimum": 0 }, "cursor": { "type": ["string", "null"] },
                "response_mode": { "type": ["string", "null"], "enum": ["path", "inline", "both", "auto_inline", null] }
            }
        }),
        "ResetMcpRequest" => json!({
            "type": "object", "additionalProperties": false,
            "properties": {
                "subaction": { "type": "string", "enum": ["plan", "exec"], "default": "plan" },
                "stores": { "type": ["array", "null"], "items": { "type": "string" } }, "collection": { "type": ["string", "null"] },
                "include_artifacts": { "type": ["boolean", "null"] }, "include_config": { "type": ["boolean", "null"] },
                "reason": { "type": ["string", "null"] }, "plan_id": { "type": ["string", "null"] }, "confirm": { "type": ["boolean", "null"] },
                "response_mode": { "type": ["string", "null"], "enum": ["path", "inline", "both", "auto_inline", null] }
            }
        }),
        "UploadsMcpRequest" => json!({
            "type": "object", "additionalProperties": false,
            "properties": {
                "subaction": { "type": "string", "enum": ["list", "create", "get", "put_content", "complete", "abort"], "default": "list" },
                "upload_id": { "type": ["string", "null"] }, "filename": { "type": ["string", "null"] }, "content_type": { "type": ["string", "null"] },
                "size_bytes": { "type": ["integer", "null"], "minimum": 0 }, "purpose": { "type": ["string", "null"], "enum": ["source_artifact", "import", "evaluation", null] },
                "sha256": { "type": ["string", "null"] }, "source_hint": { "type": ["string", "null"] }, "content": { "type": ["string", "null"] },
                "content_ref": { "type": ["object", "null"] }, "source_options": { "type": ["object", "null"] }, "reason": { "type": ["string", "null"] },
                "status": { "type": ["string", "null"], "enum": ["pending", "received", "completed", "aborted", "expired", null] },
                "limit": { "type": ["integer", "null"], "minimum": 0 }, "cursor": { "type": ["string", "null"] },
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

pub(crate) fn typed_subaction_variants(action: &str) -> Vec<String> {
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

fn enum_string_values(schema: &Value) -> Vec<String> {
    let mut values = Vec::new();
    if let Some(array) = schema.get("enum").and_then(Value::as_array) {
        values.extend(
            array
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string)),
        );
    }
    if let Some(branches) = schema.get("oneOf").and_then(Value::as_array) {
        for branch in branches {
            if let Some(array) = branch.get("enum").and_then(Value::as_array) {
                values.extend(
                    array
                        .iter()
                        .filter_map(|value| value.as_str().map(str::to_string)),
                );
            }
            if let Some(value) = branch.get("const").and_then(Value::as_str) {
                values.push(value.to_string());
            }
        }
    }
    values
}
