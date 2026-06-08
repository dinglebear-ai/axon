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
/// metadata, dependency caches, and build/output artifacts. Mirrors the
/// excluded-prefix list in `crate::ingest::github::is_indexable_source_path`.
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
/// skipped before any read attempt. Mirrors `xtask::checks::secrets::SKIP_EXTENSIONS`.
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

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
