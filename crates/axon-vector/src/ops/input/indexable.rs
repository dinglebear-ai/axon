//! Path-allowlist predicates deciding which repo files are worth indexing.
//!
//! Moved here from `ingest::github` to break the `vector -> ingest` dependency
//! cycle (these predicates depend on `select::{is_ts_declaration_file,
//! is_minified_asset_filename}`, which live in this same `input` module). The
//! sole live consumer is `vector::ops::file_ingest`; `ingest::github` keeps a
//! re-export shim for tests and any other callers.

use crate::ops::input::select::{
    has_pruned_component, is_minified_asset_filename, is_ts_declaration_file,
};

/// Returns true if a file path should be indexed when --include-source is set.
/// Excludes lock files, generated files, binaries, and non-code files.
pub fn is_indexable_source_path(path: &str) -> bool {
    // Reject build artifact and tool cache directories through the same shared
    // path-component policy used by local file ingestion.
    if has_pruned_component(path) {
        return false;
    }

    if is_generated_bulk_path(path) {
        return false;
    }

    let path_lower = path.to_ascii_lowercase();
    let filename_lower = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    // Reject lock files by name suffix.
    if path_lower.ends_with(".lock")
        || path_lower.ends_with("-lock.json")
        || path_lower.ends_with(".lock.json")
        || path_lower.ends_with("-lock.yaml")
        || path_lower.ends_with("-lock.yml")
        || matches!(filename_lower.as_str(), "bun.lockb" | "uv.lock")
    {
        return false;
    }

    // Reject TypeScript declaration files and minified assets — compiler/bundler output.
    if is_ts_declaration_file(&filename_lower) || is_minified_asset_filename(&filename_lower) {
        return false;
    }

    // Accept known source extensions
    let accepted = [
        // Systems languages
        ".rs", ".c", ".cpp", ".h", ".hpp", ".zig", // JVM / .NET
        ".java", ".kt", ".kts", ".cs", ".gradle", // Scripting
        ".py", ".rb", ".php", ".lua", ".sh", // Web / frontend
        ".ts", ".js", ".tsx", ".jsx", // Go / Swift
        ".go", ".swift", // BEAM (Elixir / Erlang)
        ".ex", ".exs", ".erl", // Data science
        ".r", ".R", ".ipynb", // Config / schema / IaC
        ".toml", ".yaml", ".yml", ".json", ".proto", ".sql", ".tf", ".nix",
        // Documentation (also caught by is_indexable_doc_path)
        ".md", ".adoc",
    ];
    accepted.iter().any(|ext| path.ends_with(ext))
}

/// Returns true if a file path should always be indexed (markdown/docs), regardless of --include-source.
pub fn is_indexable_doc_path(path: &str) -> bool {
    if is_generated_bulk_path(path) {
        return false;
    }
    let accepted = [".md", ".mdx", ".rst", ".txt", ".adoc"];
    accepted.iter().any(|ext| path.ends_with(ext))
}

fn is_generated_bulk_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let filename = lower.rsplit('/').next().unwrap_or(lower.as_str());
    let ext = filename.rsplit_once('.').map(|(_, ext)| ext).unwrap_or("");

    if matches!(
        filename,
        "openapi.json"
            | "openapi.yaml"
            | "openapi.yml"
            | "swagger.json"
            | "swagger.yaml"
            | "swagger.yml"
    ) {
        return true;
    }

    if matches!(ext, "json" | "yaml" | "yml")
        && (lower.contains("/openapi/") || lower.contains("/swagger/"))
    {
        return true;
    }

    if lower.starts_with("docs/reference/actions/") || lower.contains("/docs/reference/actions/") {
        return true;
    }

    // TypeScript declaration files (.d.ts, .d.mts, .d.cts) and minified assets
    // are compiler/bundler output — no useful semantic content for RAG.
    if is_ts_declaration_file(filename) || is_minified_asset_filename(filename) {
        return true;
    }

    let generated_segment = lower.starts_with("generated/")
        || lower.contains("/generated/")
        || lower.starts_with("gen/")
        || lower.contains("/gen/");
    generated_segment && matches!(ext, "json" | "yaml" | "yml" | "ts" | "tsx" | "js" | "jsx")
}
