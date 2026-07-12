//! Pure file-selection predicates for the code-search watch dry-run preview.
//!
//! Ported off the legacy `axon_vector::ops::{file_ingest, input::select}`
//! walker (axon-vector is being retired — see #298) so this module has no
//! remaining dependency on that crate. Only the `SelectionPolicy::CodeSearch`
//! behavior is reproduced here — the only policy `code_search_watch` ever
//! passes. The real indexing path (`query::code_search_refresh`) walks files
//! through `axon-adapters`' local adapter via
//! `local_source::index_local_source_with_job`; this module backs only the
//! `dry-run` file-list preview, so any drift between the two is cosmetic
//! (preview counts), not a correctness issue for indexing itself.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow as anyhow_err};
use axon_core::logging::log_warn;

/// Directory names pruned from a recursive walk: version-control metadata,
/// dependency caches, and build/output artifacts. Mirrors the (now-deleted)
/// `axon_vector::ops::input::select::PRUNED_DIRS`.
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

pub(super) fn is_pruned_dir(name: &str) -> bool {
    PRUNED_DIRS.contains(&name) || name.ends_with(".egg-info")
}

fn has_pruned_component(path: &str) -> bool {
    path.split(['/', '\\']).any(is_pruned_dir)
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
    if is_ts_declaration_file(filename) || is_minified_asset_filename(filename) {
        return true;
    }
    let generated_segment = lower.starts_with("generated/")
        || lower.contains("/generated/")
        || lower.starts_with("gen/")
        || lower.contains("/gen/");
    generated_segment && matches!(ext, "json" | "yaml" | "yml" | "ts" | "tsx" | "js" | "jsx")
}

/// Mirrors `axon_vector::ops::input::indexable::is_indexable_source_path`.
fn is_indexable_source_path(path: &str) -> bool {
    if has_pruned_component(path) {
        return false;
    }
    if is_generated_bulk_path(path) {
        return false;
    }
    let path_lower = path.to_ascii_lowercase();
    let filename_lower = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if path_lower.ends_with(".lock")
        || path_lower.ends_with("-lock.json")
        || path_lower.ends_with(".lock.json")
        || path_lower.ends_with("-lock.yaml")
        || path_lower.ends_with("-lock.yml")
        || matches!(filename_lower.as_str(), "bun.lockb" | "uv.lock")
    {
        return false;
    }
    if is_ts_declaration_file(&filename_lower) || is_minified_asset_filename(&filename_lower) {
        return false;
    }
    let accepted = [
        ".rs", ".c", ".cpp", ".h", ".hpp", ".zig", ".java", ".kt", ".kts", ".cs", ".gradle", ".py",
        ".rb", ".php", ".lua", ".sh", ".ts", ".js", ".tsx", ".jsx", ".go", ".swift", ".ex", ".exs",
        ".erl", ".r", ".R", ".ipynb", ".toml", ".yaml", ".yml", ".json", ".proto", ".sql", ".tf",
        ".nix", ".md", ".adoc",
    ];
    accepted.iter().any(|ext| path.ends_with(ext))
}

/// Mirrors `axon_vector::ops::input::indexable::is_indexable_doc_path`.
fn is_indexable_doc_path(path: &str) -> bool {
    if is_generated_bulk_path(path) {
        return false;
    }
    let accepted = [".md", ".mdx", ".rst", ".txt", ".adoc"];
    accepted.iter().any(|ext| path.ends_with(ext))
}

/// `SelectionPolicy::CodeSearch` file predicate: mirrors
/// `axon_vector::ops::file_ingest::should_include_file`'s `CodeSearch` arm.
fn should_include_code_search_file(path: &Path, root: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    if rel
        .components()
        .any(|component| component.as_os_str().to_str().is_some_and(is_pruned_dir))
    {
        return false;
    }
    let rel = rel.to_string_lossy().replace('\\', "/");
    is_indexable_doc_path(&rel) || is_indexable_source_path(&rel)
}

/// Recursively collect files under `root` using the `CodeSearch` selection
/// policy. Mirrors `axon_vector::ops::file_ingest::collect_files`: pruned
/// directories are never descended into, unreadable subdirectories are
/// logged and skipped, and an unreadable root is a hard error. Returned paths
/// are sorted.
pub(super) async fn collect_code_search_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut at_root = true;
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(e) if at_root => {
                return Err(anyhow_err!(
                    "invalid code-search watch directory {}: {e}",
                    dir.display()
                ));
            }
            Err(e) => {
                log_warn(&format!(
                    "command=code_search_watch skip_unreadable_dir path={} err={e}",
                    dir.display()
                ));
                at_root = false;
                continue;
            }
        };
        at_root = false;
        loop {
            let entry = match entries.next_entry().await {
                Ok(Some(entry)) => entry,
                Ok(None) => break,
                Err(e) => {
                    log_warn(&format!(
                        "command=code_search_watch dir_iter_error path={} err={e}",
                        dir.display()
                    ));
                    continue;
                }
            };
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let ft = match entry.file_type().await {
                Ok(ft) => ft,
                Err(e) => {
                    log_warn(&format!(
                        "command=code_search_watch skip_unknown_type path={} err={e}",
                        path.display()
                    ));
                    continue;
                }
            };
            if ft.is_dir() {
                if !is_pruned_dir(name) {
                    stack.push(path);
                }
            } else if ft.is_file() && should_include_code_search_file(&path, root) {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

pub(super) fn should_include_file(path: &Path, root: &Path) -> bool {
    should_include_code_search_file(path, root)
}
