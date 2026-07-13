use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub(super) struct RuntimeMemoryMetadata {
    pub(super) project: Option<String>,
    pub(super) repo: Option<String>,
    pub(super) workspace: Option<String>,
    pub(super) git_branch: Option<String>,
    pub(super) git_commit: Option<String>,
    pub(super) git_dirty: Option<bool>,
    pub(super) cwd: Option<String>,
}

pub(super) fn detect_runtime_memory_metadata() -> RuntimeMemoryMetadata {
    let cwd_path = match std::env::current_dir() {
        Ok(path) => path,
        Err(_) => return RuntimeMemoryMetadata::default(),
    };
    let cwd = path_to_string(&cwd_path);
    let workspace_path = git_output(&cwd_path, ["rev-parse", "--show-toplevel"])
        .map(PathBuf::from)
        .or_else(|| Some(cwd_path.clone()));
    let workspace = workspace_path
        .as_ref()
        .and_then(|path| canonicalize_lossy(path).as_deref().and_then(path_to_string));
    let repo = git_output(&cwd_path, ["remote", "get-url", "origin"])
        .as_deref()
        .and_then(remote_to_repo_slug);
    let project = repo.as_deref().and_then(repo_slug_to_project).or_else(|| {
        workspace_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .map(str::to_string)
    });
    let git_branch = git_output(&cwd_path, ["rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|branch| branch != "HEAD");
    let git_commit = git_output(&cwd_path, ["rev-parse", "HEAD"]);
    let git_dirty =
        git_output(&cwd_path, ["status", "--porcelain"]).map(|status| !status.is_empty());

    RuntimeMemoryMetadata {
        project,
        repo,
        workspace,
        git_branch,
        git_commit,
        git_dirty,
        cwd,
    }
}

fn git_output<const N: usize>(cwd: &Path, args: [&str; N]) -> Option<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn remote_to_repo_slug(remote: &str) -> Option<String> {
    let normalized = remote.trim().trim_end_matches(".git").replace(':', "/");
    let mut parts = normalized
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>();
    let repo = parts.pop()?;
    let owner = parts.pop()?;
    if owner.contains('.') || repo.contains('.') {
        return None;
    }
    Some(format!("{owner}/{repo}"))
}

fn repo_slug_to_project(repo: &str) -> Option<String> {
    repo.rsplit('/')
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn canonicalize_lossy(path: &Path) -> Option<PathBuf> {
    path.canonicalize()
        .ok()
        .or_else(|| Some(path.to_path_buf()))
}

fn path_to_string(path: &Path) -> Option<String> {
    path.to_str()
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
}
