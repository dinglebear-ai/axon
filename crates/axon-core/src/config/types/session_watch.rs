use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct SessionWatchConfig {
    pub path: Option<PathBuf>,
    pub debounce: Duration,
    pub settle: Duration,
    pub max_retries: u8,
    pub max_batch_docs: usize,
    pub max_processing_concurrency: usize,
    pub rescan_cooldown: Duration,
    pub initial_scan: bool,
    pub upload_to_server: bool,
    pub upload_server_url: Option<String>,
    pub upload_token: Option<String>,
    pub verbose_paths: bool,
    pub json: bool,
}

#[derive(Debug, Clone)]
pub struct CodeSearchWatchConfig {
    pub roots: Vec<PathBuf>,
    pub debounce: Duration,
    pub settle: Duration,
    pub initial_refresh: bool,
    pub dry_run: bool,
    pub enable: bool,
    pub json: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionsRuntimeAction {
    WatchStatus { limit: usize },
    SmokeWatch { timeout_secs: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionWatchServiceAction {
    Install,
    Check,
    Remove,
    Status,
}

impl SessionWatchServiceAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Check => "check",
            Self::Remove => "remove",
            Self::Status => "status",
        }
    }
}
