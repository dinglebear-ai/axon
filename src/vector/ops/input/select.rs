// File-selection policy shared by the local-directory embed reader and the
// server-side embed validator.
//
// These predicates decide which directories to descend into and which files are
// candidates for embedding. They are intentionally pure (no I/O) so both the
// async reader in `tei/prepare.rs` and the synchronous validator in
// `services/embed.rs` can enumerate exactly the same set of files. Security
// checks (allowed roots, secret-name rejection, symlink/size limits) live in
// the validator on top of this selection — they are NOT part of selection.

use super::classify::path_extension;

/// Directory names that are pruned from a recursive embed walk: version-control
/// metadata, dependency caches, and build/output artifacts. This is a *superset*
/// of the excluded-prefix list in `crate::ingest::github::is_indexable_source_path`
/// — it additionally prunes `.git` (the github path filters by file extension via
/// its allowlist, so it never needs to prune `.git` explicitly). Keep the shared
/// entries in sync (no compile-time link).
const PRUNED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "__pycache__",
    "target",
    "dist",
    "build",
    "out",
    "coverage",
    "vendor",
    ".venv",
    "venv",
    "env",
    ".next",
    ".nuxt",
    ".gradle",
    ".terraform",
    ".mypy_cache",
    ".pytest_cache",
];

/// File extensions (lowercase, without the dot) treated as binary/non-text and
/// skipped before any read attempt. Mirrors `xtask::checks::secrets::SKIP_EXTENSIONS`
/// (no compile-time link; keep in sync manually).
const BINARY_EXTENSIONS: &[&str] = &[
    // images
    "png", "jpg", "jpeg", "gif", "ico", "bmp", "webp", "tiff", // fonts
    "woff", "woff2", "ttf", "eot", "otf", // documents / archives
    "pdf", "zip", "tar", "gz", "bz2", "xz", "zst", "7z", "rar", // binaries
    "exe", "dll", "so", "dylib", "a", "o", "wasm", "bin", // audio / video
    "mp3", "mp4", "avi", "mov", "mkv", "wav", "flac", // databases
    "db", "sqlite", "sqlite3",
];

/// Extensions for which tree-sitter AST-aware chunking is available. Must stay
/// in sync with `super::code::language_for_extension`.
const CODE_EXTENSIONS: &[&str] = &["rs", "py", "js", "jsx", "ts", "tsx", "go", "sh", "bash"];

/// Returns true if a directory with this name should be pruned (not descended
/// into) during a recursive embed walk. Comparison is case-sensitive — these
/// names are conventionally lowercase on every platform we target.
pub fn is_pruned_dir(name: &str) -> bool {
    PRUNED_DIRS.contains(&name)
}

/// Returns true if a file with this extension is binary/non-text and should be
/// skipped before reading. `ext` is matched case-insensitively.
pub fn is_binary_ext(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    BINARY_EXTENSIONS.contains(&lower.as_str())
}

/// Returns true if the file at `path` should chunk through tree-sitter
/// (`code::chunk_code`) rather than the prose/markdown splitters. Matched on the
/// path's extension, case-insensitively.
pub fn should_chunk_as_code(path: &str) -> bool {
    let ext = path_extension(path).to_ascii_lowercase();
    CODE_EXTENSIONS.contains(&ext.as_str())
}

/// Returns true when an embed input string is shaped like a filesystem path.
///
/// Used to distinguish "this path doesn't exist" (an error the caller must
/// surface — embedding the path *string* as content silently corrupts the
/// index) from genuine free-text embed input. Shared by the embed reader
/// (`vector::ops::tei::prepare`) and the server-side input validator
/// (`services::embed`).
pub fn looks_path_like(input: &str) -> bool {
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

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
