use super::{CodeSearchWatchDryRunPlan, CodeSearchWatchDryRunRoot};
use anyhow::Result;
use axon_code_index::manifest::collect_git_files;
use axon_core::config::CodeSearchWatchConfig;
use axon_vector::ops::file_ingest::{SelectionPolicy, collect_files};
use axon_vector::ops::input::select;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) fn code_search_watch_dirs(options: &CodeSearchWatchConfig) -> Result<Vec<PathBuf>> {
    let raw = if options.roots.is_empty() {
        vec![std::env::current_dir()?]
    } else {
        options.roots.clone()
    };
    raw.into_iter()
        .map(|path| std::fs::canonicalize(path).map_err(Into::into))
        .collect()
}

pub(super) fn discover_code_search_watch_roots_for_dirs(
    watch_dirs: &[PathBuf],
) -> Result<Vec<PathBuf>> {
    let mut roots = Vec::new();
    for dir in watch_dirs {
        roots.extend(discover_code_search_watch_roots(dir)?);
    }
    roots.sort();
    roots.dedup();
    Ok(roots)
}

pub(super) async fn build_code_search_watch_dry_run_plan(
    watch_dirs: &[PathBuf],
) -> Result<CodeSearchWatchDryRunPlan> {
    let roots = discover_code_search_watch_roots_for_dirs(watch_dirs)?;
    let mut planned_roots = Vec::new();
    let mut total_files = 0usize;
    for root in roots {
        let files = collect_code_search_watch_files(&root).await?;
        let files = files
            .into_iter()
            .filter_map(|path| {
                path.strip_prefix(&root)
                    .ok()
                    .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            })
            .collect::<Vec<_>>();
        total_files += files.len();
        planned_roots.push(CodeSearchWatchDryRunRoot { root, files });
    }
    Ok(CodeSearchWatchDryRunPlan {
        roots: planned_roots,
        total_files,
    })
}

async fn collect_code_search_watch_files(root: &Path) -> Result<Vec<PathBuf>> {
    match collect_git_files(root, SelectionPolicy::CodeSearch).await {
        Ok(files) => Ok(files),
        Err(_) => collect_files(root, SelectionPolicy::CodeSearch).await,
    }
}

pub(super) fn code_search_watch_dirty_roots(
    roots: &[PathBuf],
    event: notify::Result<notify::Event>,
    overflow_rescan: &AtomicBool,
) -> Vec<PathBuf> {
    let Ok(event) = event else {
        overflow_rescan.store(true, Ordering::Relaxed);
        return Vec::new();
    };
    if event.need_rescan() {
        overflow_rescan.store(true, Ordering::Relaxed);
        return Vec::new();
    }
    if !(event.kind.is_create() || event.kind.is_modify() || event.kind.is_remove()) {
        return Vec::new();
    }
    let mut dirty = BTreeSet::new();
    for path in event.paths {
        if let Some(root) = roots.iter().find(|root| path.starts_with(root.as_path()))
            && code_search_watch_path_is_relevant(root, &path)
        {
            dirty.insert(root.clone());
        }
    }
    dirty.into_iter().collect()
}

fn code_search_watch_path_is_relevant(root: &Path, path: &Path) -> bool {
    !code_search_watch_path_is_pruned(root, path)
}

fn discover_code_search_watch_roots(workspace: &Path) -> Result<Vec<PathBuf>> {
    if is_git_checkout_root(workspace) {
        return Ok(vec![workspace.to_path_buf()]);
    }
    let mut roots = Vec::new();
    for entry in std::fs::read_dir(workspace)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || code_search_watch_path_is_pruned(workspace, &path) {
            continue;
        }
        if is_git_checkout_root(&path) {
            roots.push(path);
        }
    }
    roots.sort();
    Ok(roots)
}

fn is_git_checkout_root(path: &Path) -> bool {
    path.join(".git").exists()
}

fn code_search_watch_path_is_pruned(root: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components().any(|component| match component {
        Component::Normal(name) => name.to_str().is_some_and(select::is_pruned_dir),
        _ => false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    #[test]
    fn code_search_watch_ignores_noisy_pruned_paths() {
        let root = Path::new("/repo");
        assert!(!code_search_watch_path_is_relevant(
            root,
            Path::new("/repo/.git/index")
        ));
        assert!(!code_search_watch_path_is_relevant(
            root,
            Path::new("/repo/target/debug/axon")
        ));
        assert!(code_search_watch_path_is_relevant(
            root,
            Path::new("/repo/src/lib.rs")
        ));
        assert!(code_search_watch_path_is_relevant(
            root,
            Path::new("/repo/docs/reference/actions/code-search.md")
        ));
    }

    #[test]
    fn discover_code_search_watch_roots_uses_workspace_children() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path();
        std::fs::create_dir(workspace.join("axon")).expect("repo dir");
        std::fs::write(workspace.join("axon/.git"), "gitdir: /tmp/axon.git\n").expect("git file");
        std::fs::create_dir(workspace.join("lab")).expect("repo dir");
        std::fs::create_dir(workspace.join("lab/.git")).expect("git dir");
        std::fs::create_dir(workspace.join("notes")).expect("non repo dir");

        let roots = discover_code_search_watch_roots(workspace).expect("discover roots");

        assert_eq!(roots, vec![workspace.join("axon"), workspace.join("lab")]);
    }

    #[tokio::test]
    async fn dry_run_plan_lists_eligible_files_by_repo() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path();
        let axon = workspace.join("axon");
        std::fs::create_dir(&axon).expect("repo dir");
        Command::new("git")
            .arg("-C")
            .arg(&axon)
            .arg("init")
            .output()
            .expect("git init");
        std::fs::create_dir_all(workspace.join("axon/src")).expect("src dir");
        std::fs::create_dir_all(workspace.join("axon/target")).expect("target dir");
        std::fs::write(workspace.join("axon/src/lib.rs"), "fn main() {}\n").expect("source");
        std::fs::write(
            workspace.join("axon/target/generated.rs"),
            "fn generated() {}\n",
        )
        .expect("generated");
        std::fs::write(workspace.join("axon/Cargo.lock"), "# lock\n").expect("lock");
        std::fs::write(workspace.join("axon/.gitignore"), "ignored.log\n").expect("gitignore");
        std::fs::write(workspace.join("axon/ignored.log"), "ignore me\n").expect("ignored");
        let lab = workspace.join("lab");
        std::fs::create_dir(&lab).expect("repo dir");
        Command::new("git")
            .arg("-C")
            .arg(&lab)
            .arg("init")
            .output()
            .expect("git init");
        std::fs::write(workspace.join("lab/README.md"), "# lab\n").expect("doc");

        let plan = build_code_search_watch_dry_run_plan(&[workspace.to_path_buf()])
            .await
            .expect("dry run plan");

        assert_eq!(plan.roots.len(), 2);
        assert!(plan.roots[0].files.contains(&"src/lib.rs".to_string()));
        assert!(!plan.roots[0].files.contains(&"ignored.log".to_string()));
        assert!(
            !plan.roots[0]
                .files
                .contains(&"target/generated.rs".to_string())
        );
        assert!(!plan.roots[0].files.contains(&"Cargo.lock".to_string()));
        assert!(plan.roots[1].files.contains(&"README.md".to_string()));
    }
}
