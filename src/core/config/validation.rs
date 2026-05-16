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
#[path = "validation_tests.rs"]
mod tests;
