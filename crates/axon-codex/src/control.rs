//! Separately configured and supervised Codex control runtime.

use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::{File, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};
use utoipa::ToSchema;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlConfig {
    pub enabled: bool,
    pub codex_binary: PathBuf,
    pub control_home: PathBuf,
    pub request_timeout: Duration,
    pub read_concurrency: usize,
    pub max_restart_backoff: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
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
    status: StdMutex<ControlStatus>,
    restart: StdMutex<RestartState>,
}

#[derive(Debug, Default)]
struct RestartState {
    consecutive_failures: u32,
    retry_not_before: Option<tokio::time::Instant>,
}

const RESTART_BACKOFF_BASE: Duration = Duration::from_millis(100);
const RESTART_FAILURE_LIMIT: u32 = 5;

impl ControlRuntime {
    pub fn new(config: ControlConfig) -> Result<Self, String> {
        let status = if config.enabled {
            validate_config(&config)?;
            ControlStatus {
                state: ControlState::Stopped,
                detail: None,
                restart_count: 0,
                binary: config
                    .codex_binary
                    .file_name()
                    .map(|value| value.to_string_lossy().into_owned()),
                home: Some("isolated-control-home".to_string()),
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
        Ok(Self {
            read_permits: Arc::new(Semaphore::new(config.read_concurrency.max(1))),
            config,
            mutation_lane: Mutex::new(()),
            status: StdMutex::new(status),
            restart: StdMutex::new(RestartState::default()),
        })
    }

    pub fn status(&self) -> ControlStatus {
        self.status
            .lock()
            .unwrap_or_else(|value| value.into_inner())
            .clone()
    }

    pub fn mark_ready(&self) {
        let mut restart = self
            .restart
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        restart.consecutive_failures = 0;
        restart.retry_not_before = None;
        drop(restart);
        self.update_status(ControlState::Ready, None);
    }

    /// Record an observed process/start failure and schedule the next bounded retry.
    pub fn record_restart_failure(&self, detail: impl Into<String>) {
        let detail = detail.into();
        let mut restart = self
            .restart
            .lock()
            .unwrap_or_else(|value| value.into_inner());
        restart.consecutive_failures = restart.consecutive_failures.saturating_add(1);
        let exponent = restart.consecutive_failures.saturating_sub(1).min(31);
        let delay = RESTART_BACKOFF_BASE
            .saturating_mul(1_u32 << exponent)
            .min(self.config.max_restart_backoff);
        restart.retry_not_before = Some(tokio::time::Instant::now() + delay);
        let state = if restart.consecutive_failures >= RESTART_FAILURE_LIMIT {
            ControlState::CircuitOpen
        } else {
            ControlState::Degraded
        };
        drop(restart);
        self.update_status(state, Some(detail));
    }

    /// Wait until the supervised retry window and count the actual restart attempt.
    pub async fn begin_restart(&self) -> Result<(), String> {
        self.require_enabled()?;
        let retry_not_before = {
            let restart = self
                .restart
                .lock()
                .unwrap_or_else(|value| value.into_inner());
            if restart.consecutive_failures >= RESTART_FAILURE_LIMIT {
                return Err("Codex control restart circuit is open".to_string());
            }
            restart.retry_not_before
        };
        if let Some(deadline) = retry_not_before {
            tokio::time::sleep_until(deadline).await;
        }
        let mut status = self.status();
        status.restart_count = status.restart_count.saturating_add(1);
        status.state = ControlState::Starting;
        status.detail = None;
        *self
            .status
            .lock()
            .unwrap_or_else(|value| value.into_inner()) = status;
        Ok(())
    }

    fn update_status(&self, state: ControlState, detail: Option<String>) {
        let mut status = self.status();
        status.state = state;
        status.detail = detail;
        *self
            .status
            .lock()
            .unwrap_or_else(|value| value.into_inner()) = status;
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
        let _process_guard = acquire_process_mutation_lock(
            self.config.control_home.join(".axon-control-mutation.lock"),
            self.config.request_timeout,
        )
        .await?;
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

struct ProcessMutationGuard(File);

impl Drop for ProcessMutationGuard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

async fn acquire_process_mutation_lock(
    path: PathBuf,
    timeout: Duration,
) -> Result<ProcessMutationGuard, String> {
    tokio::task::spawn_blocking(move || {
        let deadline = std::time::Instant::now() + timeout;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|error| {
                format!(
                    "cannot open Codex mutation lock {}: {error}",
                    path.display()
                )
            })?;
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(ProcessMutationGuard(file)),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return Err(
                            "codex control cross-process mutation lock timed out".to_string()
                        );
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(error) => {
                    return Err(format!(
                        "cannot lock Codex control home {}: {error}",
                        path.display()
                    ));
                }
            }
        }
    })
    .await
    .map_err(|error| format!("Codex mutation lock task failed: {error}"))?
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
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("Codex control binary must be executable".to_string());
        }
        if metadata.permissions().mode() & 0o022 != 0 {
            return Err("Codex control binary must not be group/world writable".to_string());
        }
        if metadata.uid() != nix::unistd::Uid::effective().as_raw() {
            return Err("Codex control binary must be owned by the Axon service user".to_string());
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
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.uid() != nix::unistd::Uid::effective().as_raw() || metadata.mode() & 0o022 != 0
        {
            return Err(
                "Codex control home must be service-owned and not group/world writable".to_string(),
            );
        }
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
            if ancestor != Path::new("/") && mode & 0o022 != 0 {
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
