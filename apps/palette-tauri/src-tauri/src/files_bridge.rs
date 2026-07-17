//! Tauri commands backing the palette's "Files" action: list a directory,
//! read a text file, write a text file — all scoped to a single allowed root
//! (the user's home directory by default) with traversal/symlink-escape
//! guards.
//!
//! # Threat model
//!
//! The renderer is untrusted input for path strings (same posture as
//! `axon_bridge::validate_axon_route` / `validate_artifact_id`).
//! `resolve_within_root` is the single choke point every command routes
//! through:
//!
//! - Relative segments (`.`, `..`) are rejected outright — no traversal above
//!   the requested path is allowed, even before hitting the root boundary.
//! - The resolved path is canonicalized (symlinks followed) and the result
//!   must still be a descendant of the canonicalized root. This rejects a
//!   symlink that lives inside the root but points outside it (e.g. a
//!   symlinked directory or file within an otherwise-safe tree).
//! - The root itself is configurable (`set_files_root`, persisted like other
//!   settings) but always resolved through the same canonicalization, so a
//!   misconfigured root can't silently widen scope via a symlink either.
//!
//! Byte writes are atomic (temp file + rename), reusing the same pattern as
//! `persistence::atomic_write`.

use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::persistence::atomic_write;

/// Files larger than this are rejected for read/write — the palette's preview
/// pane is not a general-purpose file manager for large binaries/archives.
const MAX_TEXT_FILE_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    /// Unix seconds since epoch, when available from the filesystem.
    pub modified_unix: Option<u64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DirListing {
    pub path: String,
    pub root: String,
    pub entries: Vec<FileEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileContents {
    pub path: String,
    pub content: String,
    pub size: u64,
}

fn files_root(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(configured) = read_configured_root(app) {
        return canonical_root(&configured);
    }
    let home = dirs::home_dir().ok_or_else(|| "could not resolve home directory".to_string())?;
    canonical_root(&home)
}

fn canonical_root(path: &Path) -> Result<PathBuf, String> {
    fs::canonicalize(path)
        .map_err(|err| format!("configured files root {} is invalid: {err}", path.display()))
}

/// Reads a persisted root override from `<app-config>/files_root.txt`, if any.
/// Kept as a plain file (not `settings.json`) so the allowed-root value has an
/// independent, minimal read/write path with no dependency on the broader
/// `PaletteSettings` schema.
fn read_configured_root(app: &AppHandle) -> Option<PathBuf> {
    let path = files_root_config_path(app)?;
    let contents = fs::read_to_string(path).ok()?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}

fn files_root_config_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_config_dir()
        .ok()
        .map(|dir| dir.join("files_root.txt"))
}

/// Resolve a renderer-supplied path against the allowed root, rejecting any
/// path that escapes it (via `..` segments or a symlink).
///
/// `requested` may be an absolute path (must be `root` or a descendant of it)
/// or a path relative to `root`. Returns the canonicalized absolute path.
fn resolve_within_root(root: &Path, requested: &str) -> Result<PathBuf, String> {
    if requested.contains('\0') {
        return Err("path must not contain NUL bytes".to_string());
    }
    let requested_path = Path::new(requested);
    let joined = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        root.join(requested_path)
    };

    // Reject `.`/`..` segments before touching the filesystem — canonicalize
    // would silently resolve them, but we want traversal attempts to fail
    // loudly rather than "helpfully" landing back inside the root by luck.
    if joined
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("path must not contain '..' segments".to_string());
    }

    let canonical = fs::canonicalize(&joined)
        .map_err(|err| format!("failed to resolve {}: {err}", joined.display()))?;

    if !canonical.starts_with(root) {
        return Err("path escapes the allowed files root".to_string());
    }
    Ok(canonical)
}

/// Same as [`resolve_within_root`] but for a path that does not need to exist
/// yet (e.g. a new file about to be written). The parent directory must exist
/// and must resolve inside the root; symlink-escape is still checked on the
/// parent.
fn resolve_new_within_root(root: &Path, requested: &str) -> Result<PathBuf, String> {
    if requested.contains('\0') {
        return Err("path must not contain NUL bytes".to_string());
    }
    let requested_path = Path::new(requested);
    let joined = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        root.join(requested_path)
    };
    if joined
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("path must not contain '..' segments".to_string());
    }
    let Some(parent) = joined.parent() else {
        return Err("path has no parent directory".to_string());
    };
    let canonical_parent = fs::canonicalize(parent)
        .map_err(|err| format!("failed to resolve {}: {err}", parent.display()))?;
    if !canonical_parent.starts_with(root) {
        return Err("path escapes the allowed files root".to_string());
    }
    let Some(file_name) = joined.file_name() else {
        return Err("path has no file name".to_string());
    };
    // If the target already exists, re-validate its own canonical form too
    // (it may itself be a symlink pointing outside the root).
    let candidate = canonical_parent.join(file_name);
    if candidate.exists() {
        return resolve_within_root(root, requested);
    }
    Ok(candidate)
}

fn modified_unix(metadata: &fs::Metadata) -> Option<u64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
}

fn to_entry(root: &Path, entry: &fs::DirEntry) -> Option<FileEntry> {
    let metadata = entry.metadata().ok()?;
    // Reject symlinked children outright rather than following them — a
    // symlink inside an otherwise-safe directory could point anywhere on
    // disk, and there is no user-facing need to traverse through it here.
    if metadata.is_symlink() {
        return None;
    }
    let path = entry.path();
    let relative = path.strip_prefix(root).unwrap_or(&path);
    Some(FileEntry {
        name: entry.file_name().to_string_lossy().into_owned(),
        path: relative.to_string_lossy().into_owned(),
        is_dir: metadata.is_dir(),
        size: metadata.len(),
        modified_unix: modified_unix(&metadata),
    })
}

#[tauri::command]
pub(crate) fn files_list_dir(app: AppHandle, path: Option<String>) -> Result<DirListing, String> {
    let root = files_root(&app)?;
    let target = match path.as_deref().filter(|value| !value.trim().is_empty()) {
        Some(requested) => resolve_within_root(&root, requested)?,
        None => root.clone(),
    };
    let metadata = fs::symlink_metadata(&target).map_err(|err| err.to_string())?;
    if metadata.is_symlink() {
        return Err("path escapes the allowed files root".to_string());
    }
    if !metadata.is_dir() {
        return Err("path is not a directory".to_string());
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(&target).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        if let Some(file_entry) = to_entry(&root, &entry) {
            entries.push(file_entry);
        }
    }
    entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    let relative = target.strip_prefix(&root).unwrap_or(&target);
    Ok(DirListing {
        path: relative.to_string_lossy().into_owned(),
        root: root.to_string_lossy().into_owned(),
        entries,
    })
}

#[tauri::command]
pub(crate) fn files_read_file(app: AppHandle, path: String) -> Result<FileContents, String> {
    let root = files_root(&app)?;
    let target = resolve_within_root(&root, &path)?;
    let metadata = fs::metadata(&target).map_err(|err| err.to_string())?;
    if metadata.is_dir() {
        return Err("path is a directory, not a file".to_string());
    }
    if metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err(format!(
            "file is too large to preview ({} bytes, limit {MAX_TEXT_FILE_BYTES})",
            metadata.len()
        ));
    }
    let bytes = fs::read(&target).map_err(|err| err.to_string())?;
    let content =
        String::from_utf8(bytes).map_err(|_| "file is not valid UTF-8 text".to_string())?;
    let relative = target.strip_prefix(&root).unwrap_or(&target);
    Ok(FileContents {
        path: relative.to_string_lossy().into_owned(),
        content,
        size: metadata.len(),
    })
}

#[tauri::command]
pub(crate) fn files_write_file(
    app: AppHandle,
    path: String,
    content: String,
) -> Result<FileContents, String> {
    let root = files_root(&app)?;
    if content.len() as u64 > MAX_TEXT_FILE_BYTES {
        return Err(format!(
            "content is too large to save ({} bytes, limit {MAX_TEXT_FILE_BYTES})",
            content.len()
        ));
    }
    let target = resolve_new_within_root(&root, &path)?;
    if target.is_dir() {
        return Err("path is a directory, not a file".to_string());
    }
    atomic_write(&target, content.as_bytes()).map_err(|err| err.to_string())?;
    let metadata = fs::metadata(&target).map_err(|err| err.to_string())?;
    let relative = target.strip_prefix(&root).unwrap_or(&target);
    Ok(FileContents {
        path: relative.to_string_lossy().into_owned(),
        content,
        size: metadata.len(),
    })
}

#[tauri::command]
pub(crate) fn files_get_root(app: AppHandle) -> Result<String, String> {
    Ok(files_root(&app)?.to_string_lossy().into_owned())
}

#[cfg(test)]
#[path = "files_bridge_tests.rs"]
mod tests;
