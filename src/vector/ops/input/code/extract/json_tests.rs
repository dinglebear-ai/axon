use super::super::super::chunk::{ChunkSource, SymbolKind};
use super::super::super::chunk_code_chunks;
use super::super::{Extractor, extract_symbols};

fn names(symbols: &[super::super::SymbolInfo]) -> Vec<String> {
    symbols.iter().filter_map(|s| s.name.clone()).collect()
}

#[test]
fn json_top_level_keys_are_keys() {
    let src = "{\n  \"name\": \"x\",\n  \"server\": { \"port\": 8080 },\n  \"list\": [1, 2]\n}\n";
    let symbols = extract_symbols(src, Extractor::Json);
    let n = names(&symbols);
    for k in ["name", "server", "list"] {
        assert!(
            n.iter().any(|x| x == k),
            "top-level key {k:?} missing: {symbols:?}"
        );
    }
    assert!(
        !n.iter().any(|x| x == "port"),
        "nested key `port` must not surface as a top-level key: {symbols:?}"
    );
    assert!(
        symbols.iter().all(|s| s.kind == SymbolKind::Key),
        "all json symbols are Key: {symbols:?}"
    );
}

#[test]
fn json_top_level_array_has_no_keys() {
    let symbols = extract_symbols("[1, 2, 3]\n", Extractor::Json);
    assert!(
        symbols.is_empty(),
        "top-level array yields no keys: {symbols:?}"
    );
}

#[test]
fn json_top_level_key_is_a_key_chunk_end_to_end() {
    // The public chunk_code_chunks path (not just extract_symbols) must emit a
    // Key-kinded chunk named after the top-level key.
    let chunks = chunk_code_chunks("{\n  \"server\": { \"port\": 8080 }\n}\n", "json").unwrap();
    assert!(
        chunks
            .iter()
            .any(|c| c.symbol_name() == Some("server") && c.symbol_kind() == Some(SymbolKind::Key)),
        "top-level key `server` must be a Key chunk: {:?}",
        chunks
            .iter()
            .map(|c| (c.symbol_name(), c.symbol_kind()))
            .collect::<Vec<_>>(),
    );
}

#[test]
fn json_keyless_falls_back_to_nonempty_prose() {
    // A top-level array or scalar yields no keys → must degrade to non-empty
    // prose, never zero chunks (the chunk_code_chunks fallback contract).
    for src in [
        "[1, 2, 3, 4, 5]\n",
        "\"just a top-level string value here\"\n",
    ] {
        let chunks = chunk_code_chunks(src, "json").unwrap();
        assert!(
            !chunks.is_empty(),
            "keyless json must not be zero chunks: {src:?}"
        );
        assert!(
            chunks.iter().all(|c| c.source == ChunkSource::Prose),
            "keyless json must be prose: {src:?}"
        );
    }
}
