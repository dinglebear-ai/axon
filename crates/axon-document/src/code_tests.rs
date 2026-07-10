use super::*;

#[test]
fn code_symbols_detects_language_from_path_extension() {
    let chunks = code_symbols("fn main() {}\n", Some("src/main.rs"), None);
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].metadata.get("code_language").unwrap(), "rust");
    assert_eq!(
        chunks[0].metadata.get("symbol_extraction_status").unwrap(),
        "parsed"
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
