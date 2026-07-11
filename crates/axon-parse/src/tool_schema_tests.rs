use axon_api::source::*;
use axon_graph::candidate::validate_candidate;
use uuid::Uuid;

use crate::parser::ParseInput;
use crate::tool_schema::tool_schema_parse_items;

fn input(path: &str, kind: ContentKind, text: &str) -> ParseInput {
    ParseInput {
        job_id: JobId::new(Uuid::from_u128(71)),
        stage_id: StageId::new(Uuid::from_u128(72)),
        requested_parser: None,
        document: SourceDocument {
            document_id: DocumentId::from("doc_tool_schema"),
            source_id: SourceId::from("src_repo"),
            source_item_key: SourceItemKey::from(path),
            canonical_uri: format!("file:///repo/{path}"),
            content_kind: kind,
            content: ContentRef::InlineText {
                text: text.to_string(),
            },
            metadata: MetadataMap::new(),
            title: None,
            language: None,
            path: Some(path.to_string()),
            mime_type: None,
            structured_payload: None,
            artifact_id: None,
            chunk_hints: Vec::new(),
            parser_hints: Vec::new(),
        },
    }
}

#[test]
fn extracts_mcp_tool_definitions_arguments_and_return_shape() {
    let text = r#"{
        "tools": [
            {
                "name": "delete_widget",
                "description": "Delete a widget by id",
                "inputSchema": {
                    "type": "object",
                    "properties": { "id": { "type": "string" }, "force": { "type": "boolean" } },
                    "required": ["id"]
                },
                "outputSchema": { "type": "object" }
            }
        ]
    }"#;
    let parsed = tool_schema_parse_items(&input("mcp.tools.json", ContentKind::Json, text));

    let definitions: Vec<_> = parsed
        .facts
        .iter()
        .filter(|fact| fact.fact_kind == "tool_definition")
        .collect();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].name, "delete_widget");
    assert_eq!(definitions[0].value["side_effect_class"], "delete");
    assert_eq!(definitions[0].value["output_kind"], "structured");

    let args: Vec<_> = parsed
        .facts
        .iter()
        .filter(|fact| fact.fact_kind == "tool_argument")
        .map(|fact| fact.name.as_str())
        .collect();
    assert_eq!(args.len(), 2);
    assert!(args.contains(&"delete_widget.id"));
    assert!(args.contains(&"delete_widget.force"));

    let required_fact = parsed
        .facts
        .iter()
        .find(|fact| fact.name == "delete_widget.id")
        .unwrap();
    assert_eq!(required_fact.value["required"], true);

    let return_shapes: Vec<_> = parsed
        .facts
        .iter()
        .filter(|fact| fact.fact_kind == "tool_return_shape")
        .collect();
    assert_eq!(return_shapes.len(), 1);

    assert!(
        parsed
            .graph_candidates
            .iter()
            .any(|candidate| candidate.kind == "tool_definition")
    );
    assert!(
        parsed
            .graph_candidates
            .iter()
            .any(|candidate| candidate.kind == "tool_schema_uses_tool")
    );
    assert!(
        parsed
            .graph_candidates
            .iter()
            .any(|candidate| candidate.kind == "tool_schema_reads_resource")
    );

    for candidate in &parsed.graph_candidates {
        validate_candidate(candidate).unwrap_or_else(|err| {
            panic!("candidate {:?} failed validation: {err:?}", candidate.kind)
        });
    }
}

#[test]
fn extracts_cli_help_commands_and_options() {
    let text = concat!(
        "axon - web crawl and RAG CLI\n",
        "\n",
        "Usage: axon [OPTIONS] <COMMAND>\n",
        "\n",
        "Commands:\n",
        "  scrape    Scrape a URL to markdown\n",
        "  crawl     Full site crawl\n",
        "\n",
        "Options:\n",
        "  -f, --format <FMT>  Output format\n",
        "  -h, --help          Print help\n",
    );
    let parsed = tool_schema_parse_items(&input("axon--help.txt", ContentKind::PlainText, text));

    let definitions: Vec<_> = parsed
        .facts
        .iter()
        .filter(|fact| fact.fact_kind == "tool_definition")
        .map(|fact| fact.name.as_str())
        .collect();
    assert!(definitions.contains(&"axon"));
    assert!(definitions.contains(&"axon scrape"));
    assert!(definitions.contains(&"axon crawl"));

    let arguments: Vec<_> = parsed
        .facts
        .iter()
        .filter(|fact| fact.fact_kind == "tool_argument")
        .map(|fact| fact.name.as_str())
        .collect();
    assert!(arguments.iter().any(|name| name.starts_with("axon.-f")));

    assert!(parsed.graph_candidates.iter().any(|candidate| {
        candidate.kind == "tool_definition"
            && candidate
                .merge_key
                .as_deref()
                .unwrap_or_default()
                .contains("axon scrape")
    }));
    assert!(
        parsed
            .graph_candidates
            .iter()
            .any(|candidate| candidate.kind == "tool_schema_reads_resource")
    );

    for candidate in &parsed.graph_candidates {
        validate_candidate(candidate).unwrap_or_else(|err| {
            panic!("candidate {:?} failed validation: {err:?}", candidate.kind)
        });
    }
}

#[test]
fn non_tool_content_yields_no_facts() {
    let parsed = tool_schema_parse_items(&input(
        "README.md",
        ContentKind::Markdown,
        "# Hello\nJust a regular document.\n",
    ));
    assert!(parsed.facts.is_empty());
    assert!(parsed.graph_candidates.is_empty());
}
