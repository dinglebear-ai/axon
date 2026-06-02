use serde::Serialize;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSetupMode {
    Setup,
    Init,
    Preflight,
    Smoke,
}

impl LocalSetupMode {
    pub(super) fn mutates(self) -> bool {
        matches!(self, Self::Setup | Self::Init)
    }

    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Setup => "setup",
            Self::Init => "init",
            Self::Preflight => "preflight",
            Self::Smoke => "smoke",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeAction {
    Up,
    Down,
    Restart,
    Rebuild,
}

impl ComposeAction {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Up => "compose-up",
            Self::Down => "compose-down",
            Self::Restart => "compose-restart",
            Self::Rebuild => "compose-rebuild",
        }
    }
}

#[derive(Debug, Default)]
pub struct LocalSetupInitOptions {
    pub mcp_host: Option<String>,
    pub mcp_port: Option<String>,
    pub auth_mode: Option<String>,
    pub mcp_token: Option<String>,
    pub oauth_public_url: Option<String>,
    pub google_client_id: Option<String>,
    pub google_client_secret: Option<String>,
    pub auth_admin_email: Option<String>,
    pub tavily_api_key: Option<String>,
    pub github_token: Option<String>,
    pub reddit_client_id: Option<String>,
    pub reddit_client_secret: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalSetupStatus {
    Ok,
    Warn,
    Error,
    Skipped,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalSetupPhase {
    pub name: &'static str,
    pub status: LocalSetupStatus,
    pub detail: String,
    pub elapsed_ms: u128,
}

#[derive(Debug, Clone, Serialize)]
pub struct LocalSetupReport {
    pub mode: &'static str,
    pub elapsed_ms: u128,
    pub target_seconds: u64,
    pub hard_max_seconds: u64,
    pub met_target: bool,
    pub exceeded_hard_max: bool,
    pub axon_home: PathBuf,
    pub env_path: PathBuf,
    pub config_path: PathBuf,
    pub compose_dir: PathBuf,
    pub web_panel_url: String,
    pub mcp_url: String,
    pub phases: Vec<LocalSetupPhase>,
    pub has_errors: bool,
}

pub(super) struct PhaseTimer {
    name: &'static str,
    start: Instant,
}

impl PhaseTimer {
    pub(super) fn start(name: &'static str) -> Self {
        Self {
            name,
            start: Instant::now(),
        }
    }

    pub(super) fn finish(
        self,
        status: LocalSetupStatus,
        detail: impl Into<String>,
    ) -> LocalSetupPhase {
        LocalSetupPhase {
            name: self.name,
            status,
            detail: detail.into(),
            elapsed_ms: self.start.elapsed().as_millis(),
        }
    }
}
