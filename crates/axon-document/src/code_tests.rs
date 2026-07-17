use super::*;
use axon_api::source::{DocumentId, MetadataMap, SourceItemKey, SourceParseFacts, SourceRange};

#[test]
fn code_symbols_detects_language_from_path_extension() {
    let chunks = code_symbols("fn main() {}\n", Some("src/main.rs"), None);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].metadata.get("code_language").unwrap(), "rust");
    assert_eq!(
        chunks[0].metadata.get("symbol_extraction_status").unwrap(),
        "fallback"
    );
}

#[test]
fn code_symbols_falls_back_to_unknown_language_without_hints() {
    let chunks = code_symbols("fn main() {}\n", None, None);
    assert_eq!(chunks[0].metadata.get("code_language").unwrap(), "unknown");
}

#[test]
fn code_symbols_marks_test_paths() {
    let chunks = code_symbols("fn it_works() {}\n", Some("src/foo_tests.rs"), None);
    assert_eq!(chunks[0].metadata.get("code_is_test").unwrap(), true);
}

#[test]
fn code_symbols_splits_huge_symbol_into_line_windows() {
    let mut body = String::from("fn huge() {\n");
    for i in 0..2000 {
        body.push_str(&format!("    let x{i} = {i};\n"));
    }
    body.push_str("}\n");

    let chunks = code_symbols(&body, Some("src/lib.rs"), None);

    assert!(chunks.len() > 1, "expected the huge symbol to be split");
    for chunk in &chunks {
        assert!(chunk.content.len() <= 3100);
        assert_eq!(
            chunk.metadata.get("symbol_extraction_status").unwrap(),
            "fallback"
        );
        assert_eq!(
            chunk.metadata.get("chunking_fallback").unwrap(),
            "line_window"
        );
    }
}

#[test]
fn code_manifest_stamps_config_file_type() {
    let chunks = code_manifest("[package]\nname = \"axon\"\n", Some("Cargo.toml"));
    assert_eq!(chunks[0].metadata.get("code_file_type").unwrap(), "config");
    assert_eq!(chunks[0].metadata.get("code_language").unwrap(), "toml");
}

#[test]
fn code_symbols_splits_javascript_and_typescript_declarations() {
    let chunks = code_symbols(
        "export function createWidget() {\n  return {}\n}\n\
export interface Props {\n  name: string\n}\n\
export const useWidget = (name: string) => ({ name });\n",
        Some("src/component.tsx"),
        None,
    );

    let symbols: Vec<_> = chunks
        .iter()
        .map(|chunk| chunk.symbol.as_deref().unwrap_or(""))
        .collect();
    assert_eq!(symbols, vec!["createWidget", "Props", "useWidget"]);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.metadata.get("code_language").unwrap() == "typescript")
    );
}

#[test]
fn code_symbols_prefers_parser_fact_ranges_when_supplied() {
    let text = "let prelude = true;\nfn render() {\n    let value = 1;\n}\nfn other() {}\n";
    let fact = SourceParseFacts {
        document_id: DocumentId::from("doc-code"),
        source_item_key: SourceItemKey::from("src/lib.rs"),
        fact_kind: "code_symbol".to_string(),
        name: "render".to_string(),
        value: serde_json::json!({ "language": "rust", "symbol_kind": "function" }),
        parser_id: "code_symbols".to_string(),
        parser_version: "test".to_string(),
        parser_method: "tree_sitter".to_string(),
        range: Some(SourceRange {
            line_start: Some(2),
            line_end: Some(4),
            byte_start: None,
            byte_end: None,
            char_start: None,
            char_end: None,
            time_start_ms: None,
            time_end_ms: None,
            dom_selector: None,
            json_pointer: None,
            yaml_path: None,
            xml_xpath: None,
            csv_row: None,
            session_turn_id: None,
            turn_start: None,
            turn_end: None,
        }),
        confidence: 0.95,
        metadata: MetadataMap::new(),
    };

    let chunks = code_symbols_with_facts(text, Some("src/lib.rs"), None, &[fact]);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].symbol.as_deref(), Some("render"));
    assert!(chunks[0].content.contains("let value = 1"));
    assert_eq!(chunks[0].metadata["code_chunk_source"], "ast_symbol");
    assert_eq!(chunks[0].metadata["code_parse_status"], "parsed");
    assert_eq!(chunks[0].metadata["symbol_extraction_status"], "parsed");
}

#[test]
fn code_symbols_use_tree_sitter_byte_ranges_and_chunk_metadata() {
    let text = "const prelude = true;\nfn render() {\n    let value = 1;\n}\nfn other() {}\n";
    let start = text.find("fn render").unwrap();
    let end = text.find("\nfn other").unwrap();
    let fact = SourceParseFacts {
        document_id: DocumentId::from("doc-code"),
        source_item_key: SourceItemKey::from("src/lib.rs"),
        fact_kind: "code_symbol".to_string(),
        name: "render".to_string(),
        value: serde_json::json!({ "language": "rust", "symbol_kind": "function" }),
        parser_id: "code_symbols".to_string(),
        parser_version: "test".to_string(),
        parser_method: "tree_sitter".to_string(),
        range: Some(SourceRange {
            line_start: Some(2),
            line_end: Some(4),
            byte_start: Some(start as u64),
            byte_end: Some(end as u64),
            char_start: Some(start as u64),
            char_end: Some(end as u64),
            time_start_ms: None,
            time_end_ms: None,
            dom_selector: None,
            json_pointer: None,
            yaml_path: None,
            xml_xpath: None,
            csv_row: None,
            session_turn_id: None,
            turn_start: None,
            turn_end: None,
        }),
        confidence: 0.95,
        metadata: MetadataMap::new(),
    };

    let chunks = code_symbols_with_facts(text, Some("src/lib.rs"), None, &[fact]);

    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].content, &text[start..end]);
    assert_eq!(chunks[0].range.byte_start, Some(start as u64));
    assert_eq!(chunks[0].range.byte_end, Some(end as u64));
    assert_eq!(chunks[0].metadata["code_chunk_source"], "ast_symbol");
    assert_eq!(chunks[0].metadata["actual_chunking_method"], "tree_sitter");
    assert_eq!(chunks[0].metadata["parser_method"], "tree_sitter");
}
