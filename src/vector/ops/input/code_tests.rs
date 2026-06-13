use super::*;

// ── language_for_extension ─────────────────────────────────────────────

#[test]
fn lang_rust_resolves() {
    assert!(language_for_extension("rs").is_some());
}

#[test]
fn lang_python_resolves() {
    assert!(language_for_extension("py").is_some());
}

#[test]
fn lang_javascript_resolves() {
    assert!(language_for_extension("js").is_some());
    assert!(language_for_extension("jsx").is_some());
}

#[test]
fn lang_typescript_resolves() {
    assert!(language_for_extension("ts").is_some());
    assert!(language_for_extension("tsx").is_some());
}

#[test]
fn lang_go_resolves() {
    assert!(language_for_extension("go").is_some());
}

#[test]
fn lang_bash_resolves() {
    assert!(language_for_extension("sh").is_some());
    assert!(language_for_extension("bash").is_some());
}

#[test]
fn lang_unknown_returns_none() {
    assert!(language_for_extension("rb").is_none());
    assert!(language_for_extension("cpp").is_none());
    assert!(language_for_extension("zig").is_none());
    assert!(language_for_extension("").is_none());
}

// ── chunk_code: unsupported extension ─────────────────────────────────

#[test]
fn chunk_unsupported_returns_none() {
    assert!(chunk_code("some content", "rb").is_none());
    assert!(chunk_code("some content", "").is_none());
}

// ── chunk_code: empty content ─────────────────────────────────────────

#[test]
fn chunk_empty_content() {
    let result = chunk_code("", "rs").unwrap();
    assert!(result.is_empty(), "empty source should produce no chunks");
}

#[test]
fn chunk_whitespace_only() {
    let result = chunk_code("   \n\n  \t  ", "rs").unwrap();
    assert!(
        result.is_empty(),
        "whitespace-only source should produce no chunks after filtering"
    );
}

// ── chunk_code: small file (single chunk) ─────────────────────────────

#[test]
fn chunk_small_rust_file() {
    let src = r#"
fn hello() {
    println!("hello world");
}
"#;
    let chunks = chunk_code(src, "rs").unwrap();
    assert_eq!(chunks.len(), 1, "small file should produce a single chunk");
    assert!(chunks[0].contains("fn hello"));
}

#[test]
fn chunk_small_python_file() {
    let src = r#"
def greet(name):
    return f"Hello, {name}!"
"#;
    let chunks = chunk_code(src, "py").unwrap();
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].contains("def greet"));
}

#[test]
fn chunk_small_typescript_file() {
    let src = r#"
export function greet(name: string): string {
    return `Hello, ${name}!`;
}
"#;
    let chunks = chunk_code(src, "ts").unwrap();
    assert_eq!(chunks.len(), 1);
    assert!(chunks[0].contains("function greet"));
}

// ── chunk_code: multi-function file ───────────────────────────────────

#[test]
fn chunk_multi_function_rust() {
    // Generate enough code to exceed a single chunk (>2000 chars).
    let mut src = String::new();
    for i in 0..30 {
        src.push_str(&format!(
            "fn func_{i}(x: i32) -> i32 {{\n    let result = x * {i} + 1;\n    result\n}}\n\n"
        ));
    }
    let chunks = chunk_code(&src, "rs").unwrap();
    assert!(
        chunks.len() > 1,
        "multi-function file should produce multiple chunks, got {}",
        chunks.len()
    );
}

// ── chunk_code: large function (forces split) ─────────────────────────

#[test]
fn chunk_large_function_forces_split() {
    // A single function body >2000 chars must be split.
    let mut body = String::from("fn big() {\n");
    for i in 0..150 {
        body.push_str(&format!("    let var_{i} = {i} * 2 + 1;\n"));
    }
    body.push_str("}\n");

    assert!(
        body.len() > 2000,
        "test setup: function body should exceed 2000 chars"
    );

    let chunks = chunk_code(&body, "rs").unwrap();
    assert!(
        chunks.len() > 1,
        "large function ({}B) should produce >1 chunk, got {}",
        body.len(),
        chunks.len()
    );
    assert!(
        chunks.iter().all(|chunk| chunk.len() <= 2000),
        "large function chunks must stay within max size: {:?}",
        chunks.iter().map(String::len).collect::<Vec<_>>()
    );
}

// ── chunk_code: cross-language ────────────────────────────────────────

#[test]
fn chunk_python_multi_function() {
    let mut src = String::new();
    for i in 0..40 {
        src.push_str(&format!(
            "def func_{i}(x):\n    result = x * {i} + 1\n    return result\n\n"
        ));
    }
    let chunks = chunk_code(&src, "py").unwrap();
    assert!(
        chunks.len() > 1,
        "multi-function Python file should produce multiple chunks"
    );
}

#[test]
fn chunk_typescript_multi_function() {
    let mut src = String::new();
    for i in 0..30 {
        src.push_str(&format!(
                "export function func_{i}(x: number): number {{\n    const result = x * {i} + 1;\n    return result;\n}}\n\n"
            ));
    }
    let chunks = chunk_code(&src, "ts").unwrap();
    assert!(
        chunks.len() > 1,
        "multi-function TypeScript file should produce multiple chunks"
    );
}

#[test]
fn chunk_go_function() {
    let src = r#"
package main

import "fmt"

func main() {
    fmt.Println("hello")
}
"#;
    let chunks = chunk_code(src, "go").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks[0].contains("func main"));
}

#[test]
fn chunk_typed_rust_function_has_symbol_metadata() {
    let src = "fn hello() {\n    println!(\"hello\");\n}\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].symbol_name(), Some("hello"));
    assert_eq!(chunks[0].symbol_kind(), Some(SymbolKind::Function));
    assert_eq!(chunks[0].declaration_start_line, 1);
    assert_eq!(chunks[0].declaration_end_line, 3);
}

#[test]
fn chunk_typed_go_function_has_symbol_metadata() {
    let src = "package main\n\nfunc Hello() {\n}\n";
    let chunks = chunk_code_chunks(src, "go").unwrap();
    assert!(chunks.iter().any(|chunk| {
        chunk.symbol_name() == Some("Hello") && chunk.symbol_kind() == Some(SymbolKind::Function)
    }));
}

#[test]
fn symbol_extraction_status_is_observable() {
    let rust = "fn hello() {}\n";
    let rust_chunks = chunk_code_chunks(rust, "rs").unwrap();
    assert_eq!(
        code_symbol_extraction_status(rust, "rs", &rust_chunks),
        "ok"
    );

    let py = "def hello():\n    pass\n";
    let py_chunks = chunk_code_chunks(py, "py").unwrap();
    assert_eq!(code_symbol_extraction_status(py, "py", &py_chunks), "ok");

    let text_chunks = vec![CodeChunk {
        text: "hello".into(),
        byte_start: 0,
        byte_end: 5,
        start_line: 1,
        end_line: 1,
        declaration_start_line: 1,
        declaration_end_line: 1,
        symbol: None,
        source: ChunkSource::Prose,
    }];
    assert_eq!(
        code_symbol_extraction_status("hello", "txt", &text_chunks),
        "prose"
    );
}

#[test]
fn chunk_typed_python_uses_code_splitter_with_symbol_metadata() {
    let mut src = String::new();
    for i in 0..40 {
        src.push_str(&format!(
            "def func_{i}(x):\n    result = x * {i} + 1\n    return result\n\n"
        ));
    }
    let chunks = chunk_code_chunks(&src, "py").unwrap();
    assert!(chunks.len() > 1, "Python should stay code-split");
    assert!(
        chunks
            .iter()
            .any(|chunk| chunk.symbol_name() == Some("func_0")
                && chunk.symbol_kind() == Some(SymbolKind::Function))
    );
}

#[test]
fn chunk_typed_typescript_function_has_symbol_metadata() {
    let src = "export function greet(name: string): string {\n    return `hi ${name}`;\n}\n";
    let chunks = chunk_code_chunks(src, "ts").unwrap();
    assert!(chunks.iter().any(|chunk| {
        chunk.symbol_name() == Some("greet") && chunk.symbol_kind() == Some(SymbolKind::Function)
    }));
}

#[test]
fn chunk_typed_javascript_class_has_symbol_metadata() {
    let src = "class Bridge {\n  connect() {\n    return true;\n  }\n}\n";
    let chunks = chunk_code_chunks(src, "js").unwrap();
    assert!(chunks.iter().any(|chunk| {
        chunk.symbol_name() == Some("Bridge") && chunk.symbol_kind() == Some(SymbolKind::Struct)
    }));
}

#[test]
fn chunk_typed_bash_function_has_symbol_metadata() {
    let src = "start_server() {\n  echo starting\n}\n";
    let chunks = chunk_code_chunks(src, "sh").unwrap();
    assert!(chunks.iter().any(|chunk| {
        chunk.symbol_name() == Some("start_server")
            && chunk.symbol_kind() == Some(SymbolKind::Function)
    }));
}

#[test]
fn oversized_rust_function_continuations_include_header() {
    let mut src = String::from("fn big() {\n");
    for i in 0..150 {
        src.push_str(&format!("    let var_{i} = {i} * 2 + 1;\n"));
    }
    src.push_str("}\n");

    let chunks = chunk_code_chunks(&src, "rs").unwrap();
    assert!(chunks.len() > 2);
    assert!(chunks[2].text.trim_start().starts_with("fn big()"));
}

#[test]
fn chunk_bash_script() {
    let src = r#"
#!/bin/bash
set -euo pipefail

greet() {
    echo "Hello, $1"
}

greet "world"
"#;
    let chunks = chunk_code(src, "sh").unwrap();
    assert!(!chunks.is_empty());
}

// ── no-empty-chunks invariant ─────────────────────────────────────────

#[test]
fn no_empty_chunks_rust() {
    let src = "fn a() {}\n\n\n\n\nfn b() {}\n\n\n\n\n";
    let chunks = chunk_code(src, "rs").unwrap();
    for (i, chunk) in chunks.iter().enumerate() {
        assert!(
            !chunk.trim().is_empty(),
            "chunk {i} is empty or whitespace-only"
        );
    }
}

#[test]
fn no_empty_chunks_large_file() {
    let mut src = String::new();
    for i in 0..50 {
        src.push_str(&format!("fn f_{i}() {{}}\n\n\n"));
    }
    let chunks = chunk_code(&src, "rs").unwrap();
    for (i, chunk) in chunks.iter().enumerate() {
        assert!(
            !chunk.trim().is_empty(),
            "chunk {i} is empty or whitespace-only"
        );
    }
}

// ── jsx / tsx variants ────────────────────────────────────────────────

#[test]
fn chunk_jsx_file() {
    let src = r#"
function App() {
    return <div>Hello</div>;
}
"#;
    let chunks = chunk_code(src, "jsx").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks[0].contains("function App"));
}

#[test]
fn chunk_tsx_file() {
    let src = r#"
function App(): JSX.Element {
    return <div>Hello</div>;
}
"#;
    let chunks = chunk_code(src, "tsx").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks[0].contains("function App"));
}

// ── SymbolKind round-trip + source-symbol classification ──────────────

#[test]
fn symbol_kind_str_roundtrips_for_every_variant() {
    use SymbolKind::*;
    for kind in [
        Function, Method, Struct, Enum, Trait, Impl, Const, Static, Type, Mod, Other,
    ] {
        assert_eq!(
            SymbolKind::from_str(kind.as_str()),
            Some(kind),
            "as_str/from_str must round-trip for {kind:?}"
        );
    }
    // Strings the ranking match used to accept but no extractor ever emits.
    assert_eq!(SymbolKind::from_str("interface"), None);
    assert_eq!(SymbolKind::from_str("var"), None);
    assert_eq!(SymbolKind::from_str("nonsense"), None);
}

#[test]
fn source_symbol_classification_excludes_mod_and_other() {
    use SymbolKind::*;
    for kind in [
        Function, Method, Struct, Enum, Trait, Impl, Const, Static, Type,
    ] {
        assert!(
            kind.is_source_symbol(),
            "{kind:?} should be a source symbol"
        );
    }
    assert!(!Mod.is_source_symbol());
    assert!(!Other.is_source_symbol());
}

#[test]
fn symbol_extraction_status_none_found_for_symbolless_rust() {
    // A supported language whose parse yields no extractable symbols.
    let src = "// just a comment\nuse std::fmt;\n";
    let chunks = chunk_code_chunks(src, "rs").unwrap();
    assert!(chunks.iter().all(|chunk| chunk.symbol.is_none()));
    assert_eq!(
        code_symbol_extraction_status(src, "rs", &chunks),
        "none_found"
    );
}

#[test]
fn oversized_multibyte_chunk_splits_without_panic() {
    // A >2000-byte chunk full of multibyte characters exercises the char-boundary
    // walk-back in push_bounded_chunks; a non-boundary slice would panic.
    let body = "café ".repeat(800); // ~4 KB, multibyte
    let src = format!("fn big() {{\n    let _s = \"{body}\";\n}}\n");
    let chunks = chunk_code_chunks(&src, "rs").unwrap();
    assert!(!chunks.is_empty());
    assert!(chunks.iter().all(|chunk| !chunk.text.is_empty()));
}

#[test]
fn postprocessed_chunks_do_not_exceed_max_size() {
    let docs = "/// documentation ".repeat(90);
    let src = format!(
        "{docs}\nfn documented() {{\n{}\n}}\n",
        "    let value = 1;\n".repeat(150)
    );

    let chunks = chunk_code_chunks(&src, "rs").unwrap();

    assert!(
        chunks.iter().all(|chunk| chunk.text.len() <= 2000),
        "postprocessing must not produce oversized chunks: {:?}",
        chunks
            .iter()
            .map(|chunk| chunk.text.len())
            .collect::<Vec<_>>()
    );
}
