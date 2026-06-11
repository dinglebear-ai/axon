use super::process::{redact_error_detail, validate_event_path};
use super::queue::PendingFiles;
use super::validate::{SessionProvider, SessionWatchRoots, validate_event_path_missing_ok};
use super::{MAX_WATCH_DIRS, SessionWatchOptions};
use crate::core::config::Config;
use anyhow::{Result, anyhow};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) enum WatchTarget {
    Directory(PathBuf),
    File { path: PathBuf, parent: PathBuf },
}

impl WatchTarget {
    pub(crate) fn root(&self) -> &Path {
        match self {
            Self::Directory(path) => path,
            Self::File { parent, .. } => parent,
        }
    }

    pub(crate) fn allowed_file(&self) -> Option<&Path> {
        match self {
            Self::Directory(_) => None,
            Self::File { path, .. } => Some(path),
        }
    }
}

pub(crate) fn canonical_path_allowed(canonical: &Path, targets: &[WatchTarget]) -> bool {
    targets.iter().any(|target| match target {
        WatchTarget::Directory(root) => canonical.starts_with(root),
        WatchTarget::File { path, .. } => canonical == path,
    })
}

fn event_path_allowed_missing_ok(path: &Path, targets: &[WatchTarget]) -> bool {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    canonical_path_allowed(&canonical, targets)
}

pub(crate) fn handle_remove_path(
    path: &Path,
    roots: &SessionWatchRoots,
    targets: &[WatchTarget],
    pending: &mut PendingFiles,
) {
    if validate_event_path_missing_ok(roots, path).is_some()
        && event_path_allowed_missing_ok(path, targets)
    {
        pending.remove(path);
    }
}

fn default_session_roots(cfg: &Config, roots: &SessionWatchRoots) -> Vec<PathBuf> {
    let all_platforms = !cfg.sessions_claude && !cfg.sessions_codex && !cfg.sessions_gemini;
    let mut selected = Vec::new();
    if cfg.sessions_claude || all_platforms {
        selected.push(roots.claude_projects.clone());
    }
    if cfg.sessions_codex || all_platforms {
        selected.push(roots.codex_sessions.clone());
    }
    if cfg.sessions_gemini || all_platforms {
        selected.push(roots.gemini_history.clone());
        selected.push(roots.gemini_tmp.clone());
    }
    selected
}

pub(crate) fn watch_targets(
    cfg: &Config,
    roots: &SessionWatchRoots,
    options: &SessionWatchOptions,
) -> Result<Vec<WatchTarget>> {
    if let Some(path) = &options.path {
        return watch_target_for_explicit_path(cfg, roots, path).map(|target| vec![target]);
    }
    default_session_roots(cfg, roots)
        .into_iter()
        .filter(|path| path.exists())
        .map(|path| {
            let link_meta = std::fs::symlink_metadata(&path)?;
            if link_meta.file_type().is_symlink() {
                return Err(anyhow!("session watch root is a symlink"));
            }
            path.canonicalize()
                .map(WatchTarget::Directory)
                .map_err(Into::into)
        })
        .collect()
}

fn watch_target_for_explicit_path(
    cfg: &Config,
    roots: &SessionWatchRoots,
    path: &Path,
) -> Result<WatchTarget> {
    let link_meta = std::fs::symlink_metadata(path)?;
    if link_meta.file_type().is_symlink() {
        return Err(anyhow!("session watch path is a symlink"));
    }
    let canonical = path.canonicalize()?;
    if canonical.is_file() {
        let validated = super::validate::validate_session_file_path(roots, &canonical)?;
        if !provider_allowed(cfg, validated.provider) {
            return Err(anyhow!(
                "session watch path does not match selected provider filters"
            ));
        }
    } else {
        let provider = validate_event_path_missing_ok(roots, &canonical).ok_or_else(|| {
            anyhow!("session watch path must be inside a supported AI session root")
        })?;
        if !provider_allowed(cfg, provider) {
            return Err(anyhow!(
                "session watch path does not match selected provider filters"
            ));
        }
    }
    if canonical.is_file() {
        let parent = canonical
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| anyhow!("session file has no parent"))?;
        return Ok(WatchTarget::File {
            path: canonical,
            parent,
        });
    }
    Ok(WatchTarget::Directory(canonical))
}

pub(crate) fn provider_allowed(cfg: &Config, provider: SessionProvider) -> bool {
    let all_platforms = !cfg.sessions_claude && !cfg.sessions_codex && !cfg.sessions_gemini;
    all_platforms
        || matches!(provider, SessionProvider::Claude) && cfg.sessions_claude
        || matches!(provider, SessionProvider::Codex) && cfg.sessions_codex
        || matches!(provider, SessionProvider::Gemini) && cfg.sessions_gemini
}

pub(crate) fn collect_watch_dirs(root: &Path) -> Result<Vec<PathBuf>> {
    let mut dirs = Vec::new();
    if root.is_file() {
        if let Some(parent) = root.parent() {
            collect_watch_dirs_inner(parent, &mut dirs, true)?;
        }
    } else {
        collect_watch_dirs_inner(root, &mut dirs, true)?;
    }
    Ok(dirs)
}

fn collect_watch_dirs_inner(path: &Path, dirs: &mut Vec<PathBuf>, is_root: bool) -> Result<()> {
    if dirs.len() >= MAX_WATCH_DIRS {
        return Err(anyhow!(
            "session watcher directory budget exceeded ({MAX_WATCH_DIRS})"
        ));
    }
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) => {
            if is_root {
                return Err(anyhow!("failed to inspect session watch path: {error}"));
            }
            tracing::warn!(
                detail = %redact_error_detail(&error.to_string()),
                "skipping unreadable session watch path"
            );
            return Ok(());
        }
    };
    if metadata.file_type().is_symlink() || metadata.is_file() || !metadata.is_dir() {
        return Ok(());
    }
    let canonical = path.canonicalize()?;
    dirs.push(canonical);
    let read_dir = match std::fs::read_dir(path) {
        Ok(read_dir) => read_dir,
        Err(error) => {
            if is_root {
                return Err(anyhow!("failed to read session watch directory: {error}"));
            }
            tracing::warn!(
                detail = %redact_error_detail(&error.to_string()),
                "skipping unreadable session watch directory"
            );
            return Ok(());
        }
    };
    for entry in sorted_child_paths(read_dir) {
        collect_watch_dirs_inner(&entry, dirs, false)?;
    }
    Ok(())
}

pub(crate) fn collect_validated_files(
    roots: &SessionWatchRoots,
    target: &WatchTarget,
) -> Vec<super::validate::ValidatedSessionPath> {
    let mut files = Vec::new();
    if let Some(path) = target.allowed_file() {
        if let Some(validated) = validate_event_path(roots, path) {
            files.push(validated);
        }
        return files;
    }
    collect_validated_files_inner(roots, target.root(), &mut files);
    files.sort_by(|left, right| left.canonical.cmp(&right.canonical));
    files
}

pub(crate) fn collect_validated_files_under(
    roots: &SessionWatchRoots,
    root: &Path,
) -> Vec<super::validate::ValidatedSessionPath> {
    let mut files = Vec::new();
    collect_validated_files_inner(roots, root, &mut files);
    files.sort_by(|left, right| left.canonical.cmp(&right.canonical));
    files
}

fn collect_validated_files_inner(
    roots: &SessionWatchRoots,
    root: &Path,
    files: &mut Vec<super::validate::ValidatedSessionPath>,
) {
    let Ok(metadata) = std::fs::symlink_metadata(root) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        return;
    }
    if metadata.is_file() {
        if let Some(validated) = validate_event_path(roots, root) {
            files.push(validated);
        }
        return;
    }
    if !metadata.is_dir() {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(root) else {
        return;
    };
    for entry in sorted_child_paths(read_dir) {
        collect_validated_files_inner(roots, &entry, files);
    }
}

fn sorted_child_paths(read_dir: std::fs::ReadDir) -> Vec<PathBuf> {
    let mut entries = read_dir
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    entries.sort();
    entries
}
