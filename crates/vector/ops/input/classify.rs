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
mod tests {
    use super::*;

    // ── path_extension ────────────────────────────────────────────────────

    #[test]
    fn ext_simple_rust() {
        assert_eq!(path_extension("src/main.rs"), "rs");
    }

    #[test]
    fn ext_nested_path() {
        assert_eq!(path_extension("crates/vector/ops/input.rs"), "rs");
    }

    #[test]
    fn ext_no_extension() {
        assert_eq!(path_extension("Makefile"), "");
    }

    #[test]
    fn ext_hidden_file_no_ext() {
        assert_eq!(path_extension(".gitignore"), "");
    }

    #[test]
    fn ext_hidden_file_with_ext() {
        assert_eq!(path_extension(".env.example"), "example");
    }

    #[test]
    fn ext_double_extension() {
        assert_eq!(path_extension("archive.tar.gz"), "gz");
    }

    #[test]
    fn ext_empty_string() {
        assert_eq!(path_extension(""), "");
    }

    #[test]
    fn ext_trailing_dot() {
        assert_eq!(path_extension("file."), "");
    }

    #[test]
    fn ext_windows_path() {
        assert_eq!(path_extension("src\\main.rs"), "rs");
    }

    // ── is_test_path ──────────────────────────────────────────────────────

    #[test]
    fn test_dir_tests() {
        assert!(is_test_path("src/tests/foo.rs"));
    }

    #[test]
    fn test_dir_test() {
        assert!(is_test_path("test/integration.py"));
    }

    #[test]
    fn test_dir_dunder_tests() {
        assert!(is_test_path("src/__tests__/component.test.tsx"));
    }

    #[test]
    fn test_filename_prefix() {
        assert!(is_test_path("src/test_utils.py"));
    }

    #[test]
    fn test_filename_underscore_test_dot() {
        assert!(is_test_path("src/engine_test.rs"));
    }

    #[test]
    fn test_filename_dot_test() {
        assert!(is_test_path("src/engine.test.ts"));
    }

    #[test]
    fn test_filename_dot_spec() {
        assert!(is_test_path("components/button.spec.tsx"));
    }

    #[test]
    fn test_not_a_test_path() {
        assert!(!is_test_path("src/main.rs"));
    }

    #[test]
    fn test_not_a_test_substring() {
        // "testing" in the filename doesn't match — only "test_", "_test.", ".test.", ".spec."
        assert!(!is_test_path("src/testing_utils.rs"));
    }

    #[test]
    fn test_case_insensitive() {
        assert!(is_test_path("src/Tests/Foo.rs"));
        assert!(is_test_path("src/__TESTS__/bar.js"));
    }

    // ── classify_file_type ────────────────────────────────────────────────

    #[test]
    fn classify_test_file() {
        assert_eq!(classify_file_type("src/tests/engine.rs"), "test");
    }

    #[test]
    fn classify_test_by_filename() {
        assert_eq!(classify_file_type("src/engine.spec.ts"), "test");
    }

    #[test]
    fn classify_config_toml() {
        assert_eq!(classify_file_type("Cargo.toml"), "config");
    }

    #[test]
    fn classify_config_yaml() {
        assert_eq!(classify_file_type("docker-compose.yaml"), "config");
    }

    #[test]
    fn classify_config_yml() {
        assert_eq!(classify_file_type("ci.yml"), "config");
    }

    #[test]
    fn classify_config_json() {
        assert_eq!(classify_file_type("package.json"), "config");
    }

    #[test]
    fn classify_doc_md() {
        assert_eq!(classify_file_type("README.md"), "doc");
    }

    #[test]
    fn classify_doc_mdx() {
        assert_eq!(classify_file_type("guide.mdx"), "doc");
    }

    #[test]
    fn classify_doc_rst() {
        assert_eq!(classify_file_type("api.rst"), "doc");
    }

    #[test]
    fn classify_doc_txt() {
        assert_eq!(classify_file_type("notes.txt"), "doc");
    }

    #[test]
    fn classify_source_rust() {
        assert_eq!(classify_file_type("src/main.rs"), "source");
    }

    #[test]
    fn classify_source_python() {
        assert_eq!(classify_file_type("app.py"), "source");
    }

    #[test]
    fn classify_source_unknown() {
        assert_eq!(classify_file_type("binary.wasm"), "source");
    }

    #[test]
    fn classify_source_no_ext() {
        assert_eq!(classify_file_type("Makefile"), "source");
    }

    #[test]
    fn classify_test_trumps_config() {
        // A JSON file in a tests/ directory should be "test", not "config".
        assert_eq!(classify_file_type("tests/fixtures/data.json"), "test");
    }

    #[test]
    fn classify_test_trumps_doc() {
        assert_eq!(classify_file_type("tests/README.md"), "test");
    }

    #[test]
    fn classify_case_insensitive_ext() {
        assert_eq!(classify_file_type("config.TOML"), "config");
        assert_eq!(classify_file_type("notes.MD"), "doc");
    }

    // ── language_name ─────────────────────────────────────────────────────

    #[test]
    fn lang_rust() {
        assert_eq!(language_name("rs"), "rust");
    }

    #[test]
    fn lang_python() {
        assert_eq!(language_name("py"), "python");
    }

    #[test]
    fn lang_javascript() {
        assert_eq!(language_name("js"), "javascript");
        assert_eq!(language_name("jsx"), "javascript");
    }

    #[test]
    fn lang_typescript() {
        assert_eq!(language_name("ts"), "typescript");
        assert_eq!(language_name("tsx"), "typescript");
    }

    #[test]
    fn lang_go() {
        assert_eq!(language_name("go"), "go");
    }

    #[test]
    fn lang_shell() {
        assert_eq!(language_name("sh"), "shell");
        assert_eq!(language_name("bash"), "shell");
    }

    #[test]
    fn lang_config_formats() {
        assert_eq!(language_name("toml"), "toml");
        assert_eq!(language_name("yaml"), "yaml");
        assert_eq!(language_name("yml"), "yaml");
        assert_eq!(language_name("json"), "json");
    }

    #[test]
    fn lang_markdown() {
        assert_eq!(language_name("md"), "markdown");
        assert_eq!(language_name("mdx"), "markdown");
    }

    #[test]
    fn lang_unknown_passthrough() {
        assert_eq!(language_name("cpp"), "cpp");
        assert_eq!(language_name("rb"), "rb");
        assert_eq!(language_name("zig"), "zig");
    }

    #[test]
    fn lang_empty_passthrough() {
        assert_eq!(language_name(""), "");
    }
}
