//! Lexical local path normalization for route identity.

use std::path::{Component, Path, PathBuf};

pub fn normalize_local_path(raw: &str) -> String {
    normalize_path_components(&expand_home(raw.trim()))
}

fn expand_home(raw: &str) -> PathBuf {
    if raw == "~" {
        return home_dir();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        return home_dir().join(rest);
    }
    PathBuf::from(raw)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("~"))
}

fn normalize_path_components(path: &Path) -> String {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.as_os_str().is_empty() || contains_only_parent_dirs(&normalized) {
                    normalized.push("..");
                } else {
                    normalized.pop();
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    let rendered = normalized.to_string_lossy().to_string();
    if rendered == "/" {
        rendered
    } else {
        rendered.trim_end_matches('/').to_string()
    }
}

fn contains_only_parent_dirs(path: &Path) -> bool {
    path.components()
        .all(|component| matches!(component, Component::ParentDir))
}
