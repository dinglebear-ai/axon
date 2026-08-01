//! Docker Compose synchronization after a successful self-update.

use super::{COMMAND_TIMEOUT, SyncCommand, err};
use axon_core::paths::axon_home_dir;
use std::env;
use std::error::Error;
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::time::timeout;

const DEV_TARGET_DIR: &str = "AXON_DEV_TARGET_DIR";

pub(super) fn build_container_sync_command_with_paths(
    installed_binary: &Path,
    paths: ComposePaths,
) -> Result<SyncCommand, Box<dyn Error>> {
    let bin_dir = installed_binary
        .parent()
        .ok_or_else(|| {
            err(format!(
                "installed binary has no parent: {}",
                installed_binary.display()
            ))
        })?
        .to_path_buf();

    Ok(SyncCommand {
        program: "docker",
        args: compose_args(&paths, true),
        current_dir: paths.compose_dir,
        env_name: DEV_TARGET_DIR,
        env_value: bin_dir,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ComposePaths {
    pub(super) compose_dir: PathBuf,
    pub(super) compose_file: PathBuf,
    pub(super) env_file: Option<PathBuf>,
}

pub(super) fn compose_args(paths: &ComposePaths, include_up: bool) -> Vec<String> {
    let mut args = vec!["compose".to_string()];
    if let Some(env_file) = &paths.env_file {
        args.push("--env-file".to_string());
        args.push(env_file.display().to_string());
    }
    args.push("-f".to_string());
    args.push(paths.compose_file.display().to_string());
    if include_up {
        args.extend(
            [
                "up",
                "-d",
                "axon",
                "--no-deps",
                "--no-build",
                "--force-recreate",
            ]
            .into_iter()
            .map(String::from),
        );
    }
    args
}

pub(super) fn resolve_compose_paths() -> Result<ComposePaths, Box<dyn Error>> {
    let axon_home = axon_home_dir().ok_or_else(|| {
        err("HOME is unset or invalid; cannot resolve trusted ~/.axon compose assets")
    })?;
    resolve_compose_paths_from_home(&axon_home, env::var_os("AXON_ENV_FILE").map(PathBuf::from))
}

pub(super) fn resolve_compose_paths_from_home(
    axon_home: &Path,
    explicit_env_file: Option<PathBuf>,
) -> Result<ComposePaths, Box<dyn Error>> {
    let compose_dir = axon_home.join("compose");
    let compose_file = compose_dir.join("docker-compose.yaml");
    if !compose_file.is_file() {
        return Err(err(format!(
            "trusted compose file is missing: {}; run axon setup init",
            compose_file.display()
        )));
    }
    Ok(ComposePaths {
        compose_dir,
        compose_file,
        env_file: resolve_axon_env_file(axon_home, explicit_env_file.as_deref()),
    })
}

pub(super) fn resolve_axon_env_file(
    axon_home: &Path,
    explicit_env_file: Option<&Path>,
) -> Option<PathBuf> {
    if let Some(path) = explicit_env_file {
        if path.is_absolute() && path.is_file() {
            return Some(path.to_path_buf());
        }
        return None;
    }
    let home_env = axon_home.join(".env");
    if home_env.is_file() {
        return Some(home_env);
    }
    None
}

pub(super) async fn sync_container_from_installed_binary(
    installed_binary: &Path,
    paths: ComposePaths,
) -> Result<(), Box<dyn Error>> {
    let sync = build_container_sync_command_with_paths(installed_binary, paths)?;
    let mut command = Command::new(sync.program);
    command
        .args(&sync.args)
        .current_dir(&sync.current_dir)
        .env(sync.env_name, sync.env_value);
    let status = run_command(command, "container sync").await?;
    if !status.success() {
        return Err(err(format!("container sync failed with status {status}")));
    }

    Ok(())
}

pub(super) async fn run_command(
    mut command: Command,
    description: &str,
) -> Result<std::process::ExitStatus, Box<dyn Error>> {
    match timeout(COMMAND_TIMEOUT, command.status()).await {
        Ok(result) => Ok(result?),
        Err(_) => Err(err(format!(
            "{description} timed out after {} seconds",
            COMMAND_TIMEOUT.as_secs()
        ))),
    }
}
