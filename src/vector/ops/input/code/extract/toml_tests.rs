use super::super::super::chunk::SymbolKind;
use super::super::super::chunk_code_chunks;
use super::super::{Extractor, extract_symbols};

fn names(symbols: &[super::super::SymbolInfo]) -> Vec<String> {
    symbols.iter().filter_map(|s| s.name.clone()).collect()
}

#[test]
fn toml_tables_and_top_level_pairs_are_keys() {
    let src = "title = \"x\"\n\n[server]\nport = 8080\n\n[server.db]\nhost = \"h\"\n\n[[items]]\nid = 1\n";
    let symbols = extract_symbols(src, Extractor::Toml);
    let n = names(&symbols);
    for k in ["title", "server", "server.db", "items"] {
        assert!(n.iter().any(|x| x == k), "expected key {k:?}: {symbols:?}");
    }
    // Keys nested inside a table must not surface as their own top-level chunk.
    assert!(
        !n.iter().any(|x| x == "port" || x == "host"),
        "table-internal keys must not surface: {symbols:?}"
    );
    assert!(
        symbols.iter().all(|s| s.kind == SymbolKind::Key),
        "all toml symbols are Key: {symbols:?}"
    );
}

#[test]
fn toml_top_level_dotted_pair_and_inline_table() {
    // A top-level dotted-key pair (`a.b.c = 1`) is captured; an inline table's
    // inner keys must NOT leak as top-level keys.
    let src = "owner.name = \"x\"\nopts = { width = 80, height = 24 }\n";
    let symbols = extract_symbols(src, Extractor::Toml);
    let n = names(&symbols);
    assert!(
        n.iter().any(|x| x == "owner.name"),
        "dotted top-level pair: {symbols:?}"
    );
    assert!(
        n.iter().any(|x| x == "opts"),
        "inline-table pair key: {symbols:?}"
    );
    assert!(
        !n.iter().any(|x| x == "width" || x == "height"),
        "inline-table inner keys must not leak: {symbols:?}"
    );
}

#[test]
fn toml_array_of_tables_is_a_key_chunk_end_to_end() {
    let chunks = chunk_code_chunks("[[items]]\nid = 1\nname = \"a\"\n", "toml").unwrap();
    assert!(
        chunks
            .iter()
            .any(|c| c.symbol_name() == Some("items") && c.symbol_kind() == Some(SymbolKind::Key)),
        "array-of-tables `items` must be a Key chunk: {:?}",
        chunks
            .iter()
            .map(|c| (c.symbol_name(), c.symbol_kind()))
            .collect::<Vec<_>>(),
    );
}
