//! Pure file-selection predicates for the server-side embed input validator.
//!
//! Ported off the (now-deleted) `axon_vector::ops::input::{classify,
//! select}` module — see #298. These are intentionally pure (no I/O) so
//! [`super::validate_server_embed_input_with_config`] can mirror exactly
//! which files a local directory embed will actually visit, without
//! depending on `axon-vector`. The real reader for a local directory embed
//! is `axon-adapters`' local source adapter (invoked via
//! `local_source::index_local_source_with_job`), whose own pruned-dir/binary
//! extension lists differ slightly (`crates/axon-adapters/src/local_select.rs`,
//! private to that crate) — this validator intentionally keeps the original,
//! broader `axon-vector`-era lists so existing `AXON_MCP_EMBED_ALLOWED_ROOTS`
//! deployments keep validating the same set of paths they always have.

/// Directory names pruned from recursive file ingestion: version-control
/// metadata, dependency caches, and build/output artifacts.
const PRUNED_DIRS: &[&str] = &[
    ".git",
    ".worktrees",
    ".hg",
    ".svn",
    "node_modules",
    ".pnpm-store",
    ".yarn",
    ".npm",
    ".turbo",
    ".parcel-cache",
    ".vite",
    ".svelte-kit",
    ".angular",
    ".vitest",
    "playwright-report",
    "test-results",
    "__pycache__",
    ".ruff_cache",
    ".mypy_cache",
    ".pytest_cache",
    ".pyre",
    ".pytype",
    ".tox",
    ".nox",
    ".hypothesis",
    ".ipynb_checkpoints",
    "htmlcov",
    "site-packages",
    ".eggs",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    ".nyc_output",
    "vendor",
    ".venv",
    "venv",
    "env",
    ".next",
    ".nuxt",
    ".gradle",
    ".terraform",
    ".serverless",
    ".aws-sam",
    ".cache",
];

/// File extensions (lowercase, without the dot) treated as binary/non-text.
const BINARY_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "ico", "bmp", "webp", "tiff", "woff", "woff2", "ttf", "eot",
    "otf", "pdf", "zip", "tar", "gz", "bz2", "xz", "zst", "7z", "rar", "exe", "dll", "so", "dylib",
    "a", "o", "wasm", "bin", "mp3", "mp4", "avi", "mov", "mkv", "wav", "flac", "db", "sqlite",
    "sqlite3",
];

/// Returns true if a directory with this name should be pruned (not
/// descended into) during a recursive embed walk.
pub(super) fn is_pruned_dir(name: &str) -> bool {
    PRUNED_DIRS.contains(&name) || name.ends_with(".egg-info")
}

/// Returns true if a file with this extension is binary/non-text and should
/// be skipped before reading. Matched case-insensitively.
pub(super) fn is_binary_ext(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    BINARY_EXTENSIONS.contains(&lower.as_str())
}

fn is_ts_declaration_file(filename: &str) -> bool {
    filename.ends_with(".d.ts") || filename.ends_with(".d.mts") || filename.ends_with(".d.cts")
}

fn is_minified_asset_filename(filename: &str) -> bool {
    filename.ends_with(".min.js")
        || filename.ends_with(".min.mjs")
        || filename.ends_with(".min.css")
        || filename.ends_with(".bundle.js")
        || filename.ends_with(".bundle.mjs")
}

/// Returns true for generated/compiled output files with no RAG value:
/// TypeScript declaration files and minified/bundled assets. Takes the bare
/// lowercased filename (last path component).
pub(super) fn is_generated_filename(filename: &str) -> bool {
    is_ts_declaration_file(filename) || is_minified_asset_filename(filename)
}

/// Returns true when an embed input string is shaped like a filesystem path.
///
/// Used to distinguish "this path doesn't exist" (an error the caller must
/// surface) from genuine free-text embed input.
pub(super) fn looks_path_like(input: &str) -> bool {
    let input = input.trim();
    let bytes = input.as_bytes();
    let windows_drive = input.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\');

    input.starts_with('/')
        || input.starts_with("./")
        || input.starts_with("../")
        || input.starts_with("~/")
        || input.starts_with("\\\\")
        || windows_drive
}

/// Extract the file extension from a path, lowercase-agnostic (caller
/// lowercases as needed). Returns an empty string if no extension is found.
pub(super) fn path_extension(path: &str) -> &str {
    let filename = path
        .rsplit_once('/')
        .or_else(|| path.rsplit_once('\\'))
        .map_or(path, |(_, name)| name);
    match filename.rsplit_once('.') {
        Some((base, ext)) if !base.is_empty() => ext,
        _ => "",
    }
}
