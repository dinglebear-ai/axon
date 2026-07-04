use axon_api::source::*;

use super::*;

fn source_doc(content_kind: ContentKind, path: &str, text: &str) -> SourceDocument {
    SourceDocument {
        document_id: DocumentId::from("doc-parse"),
        source_id: SourceId::from("src-parse"),
        source_item_key: SourceItemKey::from(path),
        canonical_uri: format!("file:///repo/{path}"),
        content_kind,
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
    }
}

#[test]
fn code_document_yields_ast_symbol_facts_and_code_route() {
    let doc = source_doc(
        ContentKind::Code,
        "src/lib.rs",
        "pub struct Widget;\npub fn render() {}\n",
    );

    let parse = parse_document(&doc);

    assert_eq!(parse.parser_id, "code_symbols");
    assert!(
        !parse.parse_facts.is_empty(),
        "code parser must produce facts"
    );
    let names: Vec<_> = parse
        .parse_facts
        .iter()
        .map(|fact| fact.name.as_str())
        .collect();
    assert!(names.contains(&"Widget"));
    assert!(names.contains(&"render"));
    assert!(
        parse
            .parse_facts
            .iter()
            .all(|f| f.fact_kind == "code_symbol")
    );
    assert!(
        !parse.graph_candidates.is_empty(),
        "code parser must emit graph candidates"
    );
    assert_eq!(parse.routed_profile(), Some(ChunkingProfile::CodeSymbol));
}

#[test]
fn manifest_document_routes_to_code_manifest() {
    let doc = source_doc(
        ContentKind::Toml,
        "Cargo.toml",
        "[package]\nname = \"demo\"\n\n[dependencies]\ntokio = \"1\"\n",
    );

    let parse = parse_document(&doc);

    assert_eq!(parse.parser_id, "manifest");
    assert!(
        !parse.parse_facts.is_empty(),
        "manifest parser must produce dependency facts"
    );
    assert_eq!(parse.routed_profile(), Some(ChunkingProfile::CodeManifest));
}

#[test]
fn prose_document_has_no_routed_profile_override() {
    let doc = source_doc(
        ContentKind::Markdown,
        "README.md",
        "# Title\n\nsome prose body\n",
    );

    let parse = parse_document(&doc);

    // Markdown parser produces heading facts but defers routing to the
    // content-kind router (returns None so the router picks MarkdownSections).
    assert_eq!(parse.routed_profile(), None);
}
