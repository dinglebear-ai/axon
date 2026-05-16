// AST-aware code chunking via tree-sitter.
//
// Splits source code at structural boundaries (functions, classes, blocks)
// instead of raw character counts, preserving semantic coherence in each chunk.

use super::CHUNK_OVERLAP;
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
    let config = ChunkConfig::new(500..2000)
        .with_overlap(CHUNK_OVERLAP)
        .expect("CHUNK_OVERLAP < max chunk size");
    let splitter = CodeSplitter::new(lang, config).expect("valid language");

    let chunks: Vec<String> = splitter
        .chunks(content)
        .map(|c| c.to_string())
        .filter(|c| !c.trim().is_empty())
        .collect();

    Some(chunks)
}

#[cfg(test)]
#[path = "code_tests.rs"]
mod tests;
