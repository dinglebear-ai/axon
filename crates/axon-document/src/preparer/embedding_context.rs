//! Optional embedding context. Citation text and ranges remain literal source.
use crate::text::{MAX_PLAIN_TEXT_CHUNK_BYTES, MAX_PLAIN_TEXT_CHUNK_CHARS};
use axon_api::source::MetadataMap;

const MAX_CONTEXT_BYTES: usize = 768;

pub(super) fn code_context(
    content: &str,
    path: Option<&str>,
    symbol: Option<&str>,
    metadata: &MetadataMap,
) -> Option<String> {
    let path = path.filter(|path| {
        !std::path::Path::new(path).is_absolute()
            && !path.contains([':', '\\'])
            && !path.split('/').any(|part| part == "..")
            && !path.chars().any(char::is_control)
    });
    let language = metadata
        .get("code_language")
        .and_then(serde_json::Value::as_str);
    let mut prefix = String::new();
    for (label, value, max_bytes) in [
        ("File", path, 320),
        ("Language", language, 48),
        ("Symbol", symbol, 256),
    ] {
        if let Some(value) =
            value.filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
        {
            let value = axon_core::redact::redact_retrievable_body_secrets(value);
            prefix.push_str(label);
            prefix.push_str(": ");
            prefix.push_str(bounded(&value, max_bytes, usize::MAX));
            prefix.push('\n');
        }
    }
    if prefix.is_empty() {
        return None;
    }
    // Reserve the source body in full. Context is optional, never source truncation.
    let bytes = MAX_PLAIN_TEXT_CHUNK_BYTES
        .saturating_sub(content.len())
        .saturating_sub(2)
        .min(MAX_CONTEXT_BYTES);
    let chars = MAX_PLAIN_TEXT_CHUNK_CHARS
        .saturating_sub(content.chars().count())
        .saturating_sub(2);
    if bytes < 16 || chars < 16 {
        return None;
    }
    let prefix = bounded(&prefix, bytes, chars);
    Some(format!("{prefix}\n\n{content}"))
}

fn bounded(value: &str, max_bytes: usize, max_chars: usize) -> &str {
    let end = value
        .char_indices()
        .take(max_chars)
        .take_while(|(offset, ch)| offset + ch.len_utf8() <= max_bytes)
        .last()
        .map_or(0, |(offset, ch)| offset + ch.len_utf8());
    &value[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn context_is_deterministic_and_source_body_is_exact() {
        let mut metadata = MetadataMap::new();
        metadata.insert("code_language".into(), json!("python"));
        let source = "def update_system():\n    return 'complete configured update'\n";
        let result = code_context(
            source,
            Some("build_files/os-update/update"),
            Some("update_system"),
            &metadata,
        )
        .unwrap();
        assert!(result.starts_with(
            "File: build_files/os-update/update\nLanguage: python\nSymbol: update_system"
        ));
        assert!(result.ends_with(source));
        assert_eq!(
            code_context(
                source,
                Some("build_files/os-update/update"),
                Some("update_system"),
                &metadata
            ),
            Some(result)
        );
    }

    #[test]
    fn unsafe_locations_are_never_sent_as_context() {
        for path in [
            "https://user:password@host/source",
            "/private/source",
            "../secret",
            "C:\\secret",
            "relative\nsecret",
        ] {
            assert_eq!(
                code_context("source", Some(path), None, &MetadataMap::new()),
                None
            );
        }
        let value = code_context(
            "source",
            Some("safe/file.py"),
            Some("Authorization: Bearer abcdefghijklmnopqrstuvwxyz"),
            &MetadataMap::new(),
        )
        .unwrap();
        assert!(!value.contains("abcdefghijklmnopqrstuvwxyz"));
    }

    #[test]
    fn utf8_context_and_total_input_are_bounded_without_source_loss() {
        let path = "🌍".repeat(500);
        let source = "x".repeat(1900);
        let value = code_context(&source, Some(&path), None, &MetadataMap::new()).unwrap();
        assert!(value.len() <= MAX_PLAIN_TEXT_CHUNK_BYTES);
        assert!(value.chars().count() <= MAX_PLAIN_TEXT_CHUNK_CHARS);
        assert!(value.ends_with(&source));
        assert_eq!(
            code_context(&"x".repeat(2000), Some("file"), None, &MetadataMap::new()),
            None
        );
    }
}
