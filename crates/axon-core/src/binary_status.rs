use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const SOURCE_INPUTS: &[&str] = &[
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    ".cargo/config.toml",
    "src",
    "config",
    "migrations",
    "assets",
    "apps/web/out",
];

pub fn stale_binary_warning() -> Option<String> {
    if std::env::var_os("AXON_SUPPRESS_STALE_BINARY_WARNING").is_some() {
        return None;
    }

    let exe = std::env::current_exe().ok()?;
    let binary_mtime = fs::metadata(&exe).ok()?.modified().ok()?;
    stale_binary_warning_at(binary_mtime, Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn stale_binary_warning_at(binary_mtime: SystemTime, source_root: &Path) -> Option<String> {
    if !source_root.is_dir() {
        return None;
    }

    let newest = SOURCE_INPUTS
        .iter()
        .filter_map(|relative| {
            newest_mtime_for_path(source_root.join(relative)).map(|mtime| (*relative, mtime))
        })
        .max_by_key(|(_, mtime)| *mtime)?;

    stale_binary_warning_for(binary_mtime, [newest])
}

pub fn stale_binary_warning_for<'a>(
    binary_mtime: SystemTime,
    inputs: impl IntoIterator<Item = (&'a str, SystemTime)>,
) -> Option<String> {
    inputs
        .into_iter()
        .filter(|(_, input_mtime)| *input_mtime > binary_mtime)
        .max_by_key(|(_, input_mtime)| *input_mtime)
        .map(|(path, _)| warning_message(path))
}

pub fn warning_message(newer_input: &str) -> String {
    format!(
        "outdated axon binary: {newer_input} is newer than the running executable. Rebuild with `cargo build --bin axon` or `cargo build --release --bin axon`; the Cargo wrapper should copy the fresh binary to ~/.local/bin/axon."
    )
}

fn newest_mtime_for_path(path: PathBuf) -> Option<SystemTime> {
    let metadata = fs::metadata(&path).ok()?;
    if metadata.is_file() {
        return metadata.modified().ok();
    }
    if !metadata.is_dir() {
        return None;
    }

    let mut newest = metadata.modified().ok();
    let mut stack = vec![path];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if should_skip_path(&path) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file()
                && let Ok(mtime) = metadata.modified()
                && newest.is_none_or(|current| mtime > current)
            {
                newest = Some(mtime);
            }
        }
    }
    newest
}

fn should_skip_path(path: &Path) -> bool {
    path.file_name()
        .and_then(OsStr::to_str)
        .is_some_and(|name| matches!(name, ".git" | "target" | "node_modules"))
}

#[cfg(test)]
#[path = "binary_status_tests.rs"]
mod tests;
