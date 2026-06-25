use super::super::super::chunk::{ChunkSource, SymbolKind};
use super::super::super::chunk_code_chunks;
use super::super::{Extractor, extract_symbols};

fn names(symbols: &[super::super::SymbolInfo]) -> Vec<String> {
    symbols.iter().filter_map(|s| s.name.clone()).collect()
}

#[test]
fn yaml_top_level_keys_are_keys() {
    let src = "name: x\nserver:\n  port: 8080\n  host: h\nlist:\n  - a\n  - b\n";
    let symbols = extract_symbols(src, Extractor::Yaml);
    let n = names(&symbols);
    for k in ["name", "server", "list"] {
        assert!(
            n.iter().any(|x| x == k),
            "top-level key {k:?} missing: {symbols:?}"
        );
    }
    assert!(
        !n.iter().any(|x| x == "port"),
        "nested key `port` must not surface as top-level: {symbols:?}"
    );
    assert!(
        symbols.iter().all(|s| s.kind == SymbolKind::Key),
        "all yaml symbols are Key: {symbols:?}"
    );
}

#[test]
fn yaml_multi_document_stream_captures_each_doc() {
    // `---`-separated documents: top-level keys in every document are captured.
    let symbols = extract_symbols("a: 1\n---\nb: 2\n", Extractor::Yaml);
    let n = names(&symbols);
    assert!(
        n.iter().any(|x| x == "a") && n.iter().any(|x| x == "b"),
        "both docs: {symbols:?}"
    );
}

#[test]
fn yaml_quoted_key_is_unquoted() {
    let symbols = extract_symbols("\"quoted key\": 1\nplain: 2\n", Extractor::Yaml);
    let n = names(&symbols);
    assert!(
        n.iter().any(|x| x == "quoted key"),
        "quotes stripped from key: {symbols:?}"
    );
    assert!(
        !n.iter().any(|x| x.starts_with('"')),
        "no key retains surrounding quotes: {symbols:?}"
    );
}

#[test]
fn yaml_top_level_sequence_falls_back_to_nonempty_prose() {
    let chunks = chunk_code_chunks("- a\n- b\n- c\n", "yaml").unwrap();
    assert!(
        !chunks.is_empty(),
        "keyless yaml sequence must not be zero chunks"
    );
    assert!(chunks.iter().all(|c| c.source == ChunkSource::Prose));
}
