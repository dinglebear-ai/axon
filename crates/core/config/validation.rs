use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CollectionNameError {
    reason: &'static str,
}

impl CollectionNameError {
    pub fn reason(self) -> &'static str {
        self.reason
    }
}

impl fmt::Display for CollectionNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.reason)
    }
}

impl std::error::Error for CollectionNameError {}

/// Validate collection names before they are interpolated into Qdrant URL paths.
///
/// Axon allows only ASCII letters, digits, `_`, `-`, and `.`. Dot-only names,
/// leading/trailing dots, and embedded `..` are rejected to avoid path traversal
/// and hidden path-component ambiguity across config, MCP, and Qdrant call sites.
pub fn validate_collection_name(name: &str) -> Result<(), CollectionNameError> {
    if name.is_empty() {
        return Err(CollectionNameError { reason: "empty" });
    }
    if name.len() > 255 {
        return Err(CollectionNameError {
            reason: "exceeds 255 characters",
        });
    }
    if name == "." || name == ".." || name.starts_with('.') || name.ends_with('.') {
        return Err(CollectionNameError {
            reason: "leading/trailing dot or path component",
        });
    }
    if name.contains("..") {
        return Err(CollectionNameError {
            reason: "contains '..'",
        });
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(CollectionNameError {
            reason: "contains a character outside [A-Za-z0-9_.-]",
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_name_accepts_safe_names() {
        for ok in ["cortex", "axon_v2", "my-collection", "a.b.c", "a"] {
            assert!(
                validate_collection_name(ok).is_ok(),
                "expected {ok:?} to pass"
            );
        }
    }

    #[test]
    fn collection_name_rejects_path_and_url_unsafe_names() {
        for bad in [
            "",
            ".",
            "..",
            "../etc/passwd",
            "a/b",
            ".hidden",
            "trailing.",
            "a..b",
            "a?x=1",
            "a#frag",
            "a b",
            "a%2e%2e",
        ] {
            assert!(
                validate_collection_name(bad).is_err(),
                "expected {bad:?} to fail"
            );
        }
    }

    #[test]
    fn collection_name_rejects_overlong() {
        assert!(validate_collection_name(&"a".repeat(256)).is_err());
    }
}
