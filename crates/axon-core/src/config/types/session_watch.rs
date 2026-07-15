#[derive(Debug, Clone)]
pub struct CodeSearchWatchConfig {
    pub roots: Vec<std::path::PathBuf>,
    pub debounce: std::time::Duration,
    pub settle: std::time::Duration,
    pub initial_refresh: bool,
    pub dry_run: bool,
    pub enable: bool,
    pub json: bool,
}
