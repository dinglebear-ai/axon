use std::collections::HashSet;
use std::path::PathBuf;

pub(super) fn strip_ansi(s: &str) -> String {
    console::strip_ansi_codes(s).into_owned()
}

pub(super) fn resolve_exe() -> Result<PathBuf, String> {
    if let Ok(p) = std::env::var("AXON_BIN")
        && !p.is_empty()
    {
        let path = PathBuf::from(&p);
        if path.exists() {
            return Ok(path);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();

    if let Ok(current) = std::env::current_exe() {
        candidates.push(current.clone());
        if let Some(bin_dir) = current.parent() {
            candidates.push(bin_dir.join("axon"));
            if let Some(target_dir) = bin_dir.parent() {
                candidates.push(target_dir.join("debug").join("axon"));
                candidates.push(target_dir.join("release").join("axon"));
            }
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("target").join("debug").join("axon"));
        candidates.push(cwd.join("target").join("release").join("axon"));
        candidates.push(cwd.join("scripts").join("axon"));
    }

    let mut seen = HashSet::new();
    let mut checked: Vec<PathBuf> = Vec::new();
    for candidate in candidates {
        if seen.insert(candidate.clone()) {
            if candidate.exists() {
                return Ok(candidate);
            }
            checked.push(candidate);
        }
    }

    let msg = format!(
        "axon binary not found at any candidate path; checked: {:?}",
        checked
    );
    tracing::warn!(context = "execute", "{msg}");
    Err(msg)
}
