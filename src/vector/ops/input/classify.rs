// File classification for embedding metadata.
//
// Pure functions that categorize file paths by type (test, config, doc, source)
// and map file extensions to human-readable language names.
/// Extract the file extension from a path, lowercase.
/// Returns an empty string if no extension is found.
pub fn path_extension(path: &str) -> &str {
    // Find the last component (after last `/` or `\`).
    let filename = path
        .rsplit_once('/')
        .or_else(|| path.rsplit_once('\\'))
        .map_or(path, |(_, name)| name);

    // Find extension after the last dot (but not if the filename starts with dot only).
    match filename.rsplit_once('.') {
        Some((base, ext)) if !base.is_empty() => ext,
        _ => "",
    }
}

/// Returns `true` if the path looks like a test file or lives in a test directory.
pub fn is_test_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();

    // Directory-based: test/, tests/, __tests__/
    if lower.contains("test/")
        || lower.contains("tests/")
        || lower.contains("__tests__/")
        // Windows paths
        || lower.contains("test\\")
        || lower.contains("tests\\")
        || lower.contains("__tests__\\")
    {
        return true;
    }

    // Filename-based: extract the last path component.
    let filename = lower
        .rsplit_once('/')
        .or_else(|| lower.rsplit_once('\\'))
        .map_or(lower.as_str(), |(_, name)| name);

    filename.starts_with("test_")
        || filename.contains("_test.")
        || filename.contains(".test.")
        || filename.contains(".spec.")
}

/// Classify a file path into one of four categories.
///
/// Returns `"test"`, `"config"`, `"doc"`, or `"source"`.
pub fn classify_file_type(path: &str) -> &'static str {
    if is_test_path(path) {
        return "test";
    }

    let ext = path_extension(path).to_ascii_lowercase();
    match ext.as_str() {
        "toml" | "yaml" | "yml" | "json" => "config",
        "md" | "mdx" | "rst" | "txt" => "doc",
        _ => "source",
    }
}

/// Map a file extension to a human-readable language name.
///
/// Returns the extension itself for unrecognized extensions.
pub fn language_name(ext: &str) -> &str {
    match ext {
        "rs" => "rust",
        "py" => "python",
        "js" | "jsx" => "javascript",
        "ts" | "tsx" => "typescript",
        "go" => "go",
        "sh" | "bash" => "shell",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        "json" => "json",
        "md" | "mdx" => "markdown",
        other => other,
    }
}

#[cfg(test)]
#[path = "classify_tests.rs"]
mod tests;
