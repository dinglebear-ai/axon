use axon_api::source::*;
use serde_json::json;

use crate::{DocumentPreparer, PrepareSourceDocumentRequest};

#[test]
fn local_rust_document_uses_code_symbol_chunks_with_code_metadata() {
    let prepared = prepare(local_doc(
        "src/lib.rs",
        ContentKind::Code,
        "pub fn answer() -> i32 {\n    42\n}\n\nstruct Thing;\n",
    ));

    assert_eq!(prepared.chunking_profile, "code_symbol");
    assert_eq!(prepared.chunking_method, "tree_sitter");
    assert_eq!(prepared.chunks.len(), 2);
    assert_eq!(
        prepared.chunks[0].chunk_locator.symbol.as_deref(),
        Some("answer")
    );
    assert_eq!(prepared.chunks[0].metadata["code_symbol_name"], "answer");
    assert_eq!(prepared.chunks[0].metadata["code_symbol_kind"], "function");
    assert_eq!(prepared.chunks[0].metadata["code_language"], "rust");
    assert_eq!(prepared.chunks[0].metadata["parser_method"], "tree_sitter");
    assert_eq!(
        prepared.chunks[0].metadata["actual_chunking_method"],
        "tree_sitter"
    );
    assert_eq!(prepared.chunks[0].source_range.byte_start, Some(0));
    assert_eq!(prepared.chunks[0].source_range.byte_end, Some(33));
    assert!(
        prepared
            .parse_facts
            .iter()
            .all(|fact| fact.parser_method == "tree_sitter")
    );
    assert_eq!(
        prepared.chunks[0].chunk_locator.path.as_deref(),
        Some("src/lib.rs")
    );
}

#[test]
fn local_typescript_document_uses_js_ts_symbol_kinds() {
    let prepared = prepare(local_doc(
        "src/component.tsx",
        ContentKind::Code,
        "export function createWidget() {}\n\
export interface Props { name: string }\n\
export const useWidget = (name: string) => ({ name });\n",
    ));

    let kinds: Vec<_> = prepared
        .chunks
        .iter()
        .map(|chunk| chunk.metadata["code_symbol_kind"].as_str().unwrap())
        .collect();
    let names: Vec<_> = prepared
        .chunks
        .iter()
        .map(|chunk| chunk.metadata["code_symbol_name"].as_str().unwrap())
        .collect();

    assert_eq!(names, vec!["createWidget", "Props", "useWidget"]);
    assert_eq!(kinds, vec!["function", "interface", "function"]);
    assert!(
        prepared
            .chunks
            .iter()
            .all(|chunk| chunk.metadata["code_language"] == "typescript")
    );
}

#[test]
fn local_markdown_document_uses_heading_sections_with_stable_ranges() {
    let prepared = prepare(local_doc(
        "docs/README.md",
        ContentKind::Markdown,
        "# Intro\nHello\n\n## Install\nRun it\n",
    ));

    assert_eq!(prepared.chunking_profile, "markdown_sections");
    assert_eq!(prepared.chunks.len(), 2);
    assert_eq!(
        prepared.chunks[0].chunk_locator.heading_path,
        vec!["Intro".to_string()]
    );
    assert_eq!(prepared.chunks[0].source_range.line_start, Some(1));
    assert_eq!(prepared.chunks[0].source_range.byte_start, Some(0));
    assert_eq!(prepared.chunks[1].source_range.line_start, Some(4));
    assert_eq!(
        prepared.chunks[1].previous_chunk_id,
        Some(prepared.chunks[0].chunk_id.clone())
    );
}

#[test]
fn local_manifest_document_routes_to_code_manifest_profile() {
    let prepared = prepare(local_doc(
        "Cargo.toml",
        ContentKind::Toml,
        "[package]\nname = \"demo\"\n",
    ));

    assert_eq!(prepared.chunking_profile, "code_manifest");
    assert_eq!(prepared.chunks.len(), 1);
    assert_eq!(prepared.chunks[0].metadata["manifest"], true);
    assert_eq!(
        prepared.chunks[0].chunk_locator.path.as_deref(),
        Some("Cargo.toml")
    );
}

fn prepare(document: SourceDocument) -> PreparedDocument {
    DocumentPreparer::default()
        .prepare(PrepareSourceDocumentRequest {
            document,
            generation: SourceGenerationId::new("gen_local_test"),
            profile: None,
            parse_facts: Vec::new(),
            graph_candidates: Vec::new(),
            warnings: Vec::new(),
            errors: Vec::new(),
        })
        .unwrap()
        .document
}

fn local_doc(path: &str, content_kind: ContentKind, text: &str) -> SourceDocument {
    let canonical_uri = format!("local://src-local/{path}");
    let mut metadata = MetadataMap::new();
    metadata.insert("source_family".to_string(), json!("code"));
    metadata.insert("source_kind".to_string(), json!("local"));
    metadata.insert("source_adapter".to_string(), json!("local"));
    metadata.insert("source_scope".to_string(), json!("repo"));
    metadata.insert(
        "item_canonical_uri".to_string(),
        json!(canonical_uri.clone()),
    );
    metadata.insert("visibility".to_string(), json!("internal"));
    metadata.insert("redaction_status".to_string(), json!("clean"));
    if path.ends_with(".rs") {
        metadata.insert("code_language".to_string(), json!("rust"));
        metadata.insert("code_file_type".to_string(), json!("source"));
    }
    SourceDocument {
        document_id: DocumentId::new(format!("doc_{}", path.replace(['/', '.'], "_"))),
        source_id: SourceId::new("src_local"),
        source_item_key: SourceItemKey::new(path),
        canonical_uri,
        content_kind,
        content: ContentRef::InlineText {
            text: text.to_string(),
        },
        metadata,
        title: Some(path.to_string()),
        language: path.ends_with(".rs").then(|| "rust".to_string()),
        path: Some(path.to_string()),
        mime_type: None,
        structured_payload: None,
        artifact_id: None,
        chunk_hints: Vec::new(),
        parser_hints: Vec::new(),
    }
}
