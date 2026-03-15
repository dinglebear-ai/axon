use crate::crates::core::config::Config;
use crate::crates::vector::ops::input;
use std::error::Error;

/// Embed source code with AST-aware chunking, falling back to plain text chunking
/// when the file extension is unsupported or AST chunking produces no chunks.
pub async fn embed_code_with_metadata(
    cfg: &Config,
    content: &str,
    url: &str,
    source_type: &str,
    title: Option<&str>,
    file_extension: &str,
    extra: Option<&serde_json::Value>,
) -> Result<usize, Box<dyn Error>> {
    if content.trim().is_empty() {
        return Ok(0);
    }
    // chunk_code() already filters empty chunks internally
    let tree_sitter_chunks = input::code::chunk_code(content, file_extension);
    let chunking_method = if tree_sitter_chunks.is_some() {
        "tree-sitter"
    } else {
        "prose"
    };
    let chunks = tree_sitter_chunks.unwrap_or_else(|| input::chunk_text(content));
    if chunks.is_empty() {
        return Ok(0);
    }
    // Merge chunking_method into extra payload so every chunk carries it
    let merged_extra = {
        let method_val = serde_json::json!({"chunking_method": chunking_method});
        match extra {
            Some(serde_json::Value::Object(map)) => {
                let mut combined = map.clone();
                combined.insert(
                    "chunking_method".to_string(),
                    serde_json::Value::String(chunking_method.to_string()),
                );
                Some(serde_json::Value::Object(combined))
            }
            _ => Some(method_val),
        }
    };
    super::embed_chunks_impl(cfg, chunks, url, source_type, title, merged_extra.as_ref()).await
}
