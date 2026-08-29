//! Separately configured and supervised Codex control runtime.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore, watch};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlConfig {
    pub enabled: bool,
    pub codex_binary: PathBuf,
    pub control_home: PathBuf,
    pub request_timeout: Duration,
    pub read_concurrency: usize,
    pub max_restart_backoff: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlState {
    Disabled,
    Starting,
    Ready,
    Degraded,
    Incompatible,
    CircuitOpen,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlStatus {
    pub state: ControlState,
    pub detail: Option<String>,
    pub restart_count: u32,
    pub binary: Option<String>,
    pub home: Option<String>,
}

#[derive(Debug)]
pub struct ControlRuntime {
    config: ControlConfig,
    mutation_lane: Mutex<()>,
    read_permits: Arc<Semaphore>,
    status_tx: watch::Sender<ControlStatus>,
}

impl ControlRuntime {
    pub fn new(config: ControlConfig) -> Result<Self, String> {
        let status = if config.enabled {
            validate_config(&config)?;
            ControlStatus {
                state: ControlState::Stopped,
                detail: None,
                restart_count: 0,
                binary: Some(config.codex_binary.display().to_string()),
                home: Some(config.control_home.display().to_string()),
            }
        } else {
            ControlStatus {
                state: ControlState::Disabled,
                detail: None,
                restart_count: 0,
                binary: None,
                home: None,
            }
        };
        let (status_tx, _) = watch::channel(status);
        Ok(Self {
            read_permits: Arc::new(Semaphore::new(config.read_concurrency.max(1))),
            config,
            mutation_lane: Mutex::new(()),
            status_tx,
        })
    }

    pub fn status(&self) -> ControlStatus {
        self.status_tx.borrow().clone()
    }

    pub fn subscribe_status(&self) -> watch::Receiver<ControlStatus> {
        self.status_tx.subscribe()
    }

    pub fn mark_ready(&self) {
        self.update_status(ControlState::Ready, None);
    }

    pub fn mark_degraded(&self, detail: impl Into<String>) {
        self.update_status(ControlState::Degraded, Some(detail.into()));
    }

    fn update_status(&self, state: ControlState, detail: Option<String>) {
        let mut status = self.status();
        status.state = state;
        status.detail = detail;
        self.status_tx.send_replace(status);
    }

    pub async fn with_read<T, F, Fut>(&self, operation: F) -> Result<T, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        self.require_enabled()?;
        let permit = tokio::time::timeout(
            self.config.request_timeout,
            self.read_permits.clone().acquire_owned(),
        )
        .await
        .map_err(|_| "codex control read capacity timed out".to_string())?
        .map_err(|_| "codex control read capacity closed".to_string())?;
        let result = tokio::time::timeout(self.config.request_timeout, operation())
            .await
            .map_err(|_| "codex control read timed out".to_string())?;
        drop(permit);
        result
    }

    pub async fn with_mutation<T, F, Fut>(&self, operation: F) -> Result<T, String>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, String>>,
    {
        self.require_enabled()?;
        let guard = tokio::time::timeout(self.config.request_timeout, self.mutation_lane.lock())
            .await
            .map_err(|_| "codex control mutation queue timed out".to_string())?;
        let result = tokio::time::timeout(self.config.request_timeout, operation())
            .await
            .map_err(|_| "codex control mutation timed out".to_string())?;
        drop(guard);
        result
    }

    fn require_enabled(&self) -> Result<(), String> {
        if self.config.enabled {
            Ok(())
        } else {
            Err("codex control runtime is disabled".to_string())
        }
    }
}

pub fn home_identity(path: &Path) -> Result<String, String> {
    let canonical = path.canonicalize().map_err(|error| {
        format!(
            "cannot resolve Codex control home {}: {error}",
            path.display()
        )
    })?;
    let metadata = fs::metadata(&canonical).map_err(|error| {
        format!(
            "cannot inspect Codex control home {}: {error}",
            canonical.display()
        )
    })?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return Ok(format!(
            "path={};dev={};ino={};uid={};mode={:o}",
            canonical.display(),
            metadata.dev(),
            metadata.ino(),
            metadata.uid(),
            metadata.mode() & 0o7777
        ));
    }
    #[cfg(not(unix))]
    Ok(format!("path={}", canonical.display()))
}

pub fn validate_config(config: &ControlConfig) -> Result<(), String> {
    if !config.enabled {
        return Ok(());
    }
    if !config.codex_binary.is_absolute() || !config.control_home.is_absolute() {
        return Err("Codex control binary and home must be absolute paths".to_string());
    }
    reject_parent_components(&config.codex_binary)?;
    reject_parent_components(&config.control_home)?;
    validate_binary(&config.codex_binary)?;
    validate_home(&config.control_home)?;
    if config.request_timeout.is_zero() {
        return Err("Codex control request timeout must be positive".to_string());
    }
    Ok(())
}

fn reject_parent_components(path: &Path) -> Result<(), String> {
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(format!("path traversal is not allowed: {}", path.display()));
    }
    Ok(())
}

fn validate_binary(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("cannot inspect Codex binary {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("Codex control binary must be a regular non-symlink file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("Codex control binary must be executable".to_string());
        }
    }
    validate_ancestors(path.parent().unwrap_or(Path::new("/")))
}

fn validate_home(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "cannot inspect Codex control home {}: {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err("Codex control home must be a non-symlink directory".to_string());
    }
    validate_ancestors(path)
}

fn validate_ancestors(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)
            .map_err(|error| format!("cannot inspect ancestor {}: {error}", ancestor.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "symlinked ancestor is not allowed: {}",
                ancestor.display()
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mode = metadata.mode();
            if ancestor != Path::new("/") && mode & 0o002 != 0 && mode & 0o1000 == 0 {
                return Err(format!(
                    "untrusted writable ancestor: {}",
                    ancestor.display()
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "control_tests.rs"]
mod tests;
