// AST-aware code chunking via tree-sitter.
//
// Splits source code at structural boundaries (functions, classes, blocks)
// instead of raw character counts, preserving semantic coherence in each chunk.

use text_splitter::{ChunkConfig, CodeSplitter};
use tree_sitter_language::LanguageFn;

/// Map a file extension to its tree-sitter language grammar.
///
/// NOTE: Only grammars with crates in Cargo.toml are supported. Common languages
/// like C, C++, Java, Ruby, Kotlin, Swift, Scala, and C# are missing because their
/// tree-sitter grammar crates are not yet dependencies.
// TODO: add tree-sitter-java, tree-sitter-c, tree-sitter-cpp, tree-sitter-c-sharp,
// tree-sitter-ruby, tree-sitter-kotlin, tree-sitter-swift, tree-sitter-scala,
// tree-sitter-toml when ready
fn language_for_extension(ext: &str) -> Option<LanguageFn> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE),
        "py" => Some(tree_sitter_python::LANGUAGE),
        "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE),
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX),
        "go" => Some(tree_sitter_go::LANGUAGE),
        "sh" | "bash" => Some(tree_sitter_bash::LANGUAGE),
        _ => None,
    }
}

/// Split source code into AST-aware chunks.
///
/// Returns `None` for unsupported file extensions. Empty chunks are filtered
/// from the output. Chunk sizes target 500–2000 characters with 200-char
/// overlap between adjacent chunks, splitting at structural boundaries
/// (functions, blocks, statements) when possible. The overlap ensures that
/// a function signature split across chunk boundaries appears in both chunks,
/// matching the 200-char overlap used by `chunk_text()`.
pub fn chunk_code(content: &str, file_extension: &str) -> Option<Vec<String>> {
    let lang = language_for_extension(file_extension)?;
    // 200-char overlap matches chunk_text() — ensures context continuity at boundaries
    let config = ChunkConfig::new(500..2000)
        .with_overlap(200)
        .expect("overlap 200 < desired capacity 2000");
    let splitter = CodeSplitter::new(lang, config).expect("valid language");

    let chunks: Vec<String> = splitter
        .chunks(content)
        .map(|c| c.to_string())
        .filter(|c| !c.trim().is_empty())
        .collect();

    Some(chunks)
}

#[cfg(test)]
mod tests {
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
}
