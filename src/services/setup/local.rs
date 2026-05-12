use super::config_store;
use crate::core::paths::{axon_home_dir, ensure_private_dir};
use serde::Serialize;
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command;

mod compose;
mod env;
mod runtime;

const SETUP_TARGET_SECS: u64 = 120;
pub(super) const SETUP_HARD_MAX_SECS: u64 = 300;
const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8001";
const DEFAULT_QDRANT_URL: &str = "http://127.0.0.1:53333";
const DEFAULT_TEI_URL: &str = "http://127.0.0.1:52000";
const DEFAULT_CHROME_URL: &str = "http://127.0.0.1:6000";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSetupMode {
    FirstRun,
    Check,
    Repair,
}

impl LocalSetupMode {
    fn mutates(self) -> bool {
        !matches!(self, Self::Check)
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::FirstRun => "first-run",
            Self::Check => "check",
            Self::Repair => "repair",
        }
    }
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

pub async fn run_local_setup(mode: LocalSetupMode) -> io::Result<LocalSetupReport> {
    let started = Instant::now();
    let axon_home = axon_home_dir().ok_or_else(|| {
        io::Error::new(
            ErrorKind::NotFound,
            "HOME is unset or invalid; cannot initialize ~/.axon",
        )
    })?;
    let env_path = axon_home.join(".env");
    let compose_dir = axon_home.join("compose");
    let mut phases = Vec::new();

    phases.push(run_filesystem_phase(&axon_home, mode)?);
    let config_init = if mode.mutates() {
        let timer = PhaseTimer::start("config");
        let init = config_store::ensure_user_config()?;
        phases.push(timer.finish(
            LocalSetupStatus::Ok,
            if init.created {
                format!("created {}", init.path.display())
            } else {
                format!("preserved {}", init.path.display())
            },
        ));
        init
    } else {
        let timer = PhaseTimer::start("config");
        let path = crate::core::paths::axon_config_path().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "HOME is unset or invalid; cannot resolve ~/.axon/config.toml",
            )
        })?;
        phases.push(timer.finish(
            if path.exists() {
                LocalSetupStatus::Ok
            } else {
                LocalSetupStatus::Warn
            },
            if path.exists() {
                format!("found {}", path.display())
            } else {
                format!(
                    "missing {}; run axon setup or axon setup repair",
                    path.display()
                )
            },
        ));
        config_store::ConfigInit {
            path,
            created: false,
        }
    };

    let finalized_env = if mode.mutates() {
        let env_result = env::ensure_env_file(&env_path)?;
        let finalized_env = Some(env_result.values);
        phases.push(env_result.phase);
        phases.push(compose::write_compose_assets(&compose_dir)?);
        finalized_env
    } else {
        phases.push(env::check_env_file(&env_path));
        phases.push(compose::check_compose_assets(&compose_dir));
        None
    };

    phases.push(check_command("docker", ["--version"]).await);
    phases.push(check_command("docker compose", ["compose", "version"]).await);
    phases.push(check_command("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"]).await);
    phases.push(check_gemini_cli().await);
    phases.push(check_oauth_config());

    if let Some(env_values) = finalized_env.as_ref() {
        phases.extend(run_mutating_runtime_phases(&compose_dir, &env_path, env_values).await);
    } else {
        phases.push(LocalSetupPhase {
            name: "compose-up",
            status: LocalSetupStatus::Skipped,
            detail: "check mode does not start Docker services".to_string(),
            elapsed_ms: 0,
        });
        phases.push(LocalSetupPhase {
            name: "smoke",
            status: LocalSetupStatus::Skipped,
            detail: "check mode does not run crawl/ask smoke".to_string(),
            elapsed_ms: 0,
        });
    }

    let elapsed_ms = started.elapsed().as_millis();
    let has_errors = phases
        .iter()
        .any(|phase| matches!(phase.status, LocalSetupStatus::Error));
    Ok(LocalSetupReport {
        mode: mode.as_str(),
        elapsed_ms,
        target_seconds: SETUP_TARGET_SECS,
        hard_max_seconds: SETUP_HARD_MAX_SECS,
        met_target: elapsed_ms <= u128::from(SETUP_TARGET_SECS) * 1000,
        exceeded_hard_max: elapsed_ms > u128::from(SETUP_HARD_MAX_SECS) * 1000,
        axon_home,
        env_path,
        config_path: config_init.path,
        compose_dir,
        web_panel_url: DEFAULT_SERVER_URL.to_string(),
        mcp_url: format!("{DEFAULT_SERVER_URL}/mcp"),
        phases,
        has_errors,
    })
}

async fn run_mutating_runtime_phases(
    compose_dir: &Path,
    env_path: &Path,
    env_values: &BTreeMap<String, String>,
) -> Vec<LocalSetupPhase> {
    let qdrant_url = env_value(env_values, "QDRANT_URL", DEFAULT_QDRANT_URL);
    let tei_url = env_value(env_values, "TEI_URL", DEFAULT_TEI_URL);
    let chrome_url = env_value(env_values, "AXON_CHROME_REMOTE_URL", DEFAULT_CHROME_URL);

    vec![
        runtime::run_compose(compose_dir, env_path, ["pull"]).await,
        runtime::run_compose(compose_dir, env_path, ["up", "-d"]).await,
        runtime::wait_http(
            "qdrant",
            format!("{}/readyz", qdrant_url.trim_end_matches('/')),
        )
        .await,
        runtime::wait_http("tei", format!("{}/health", tei_url.trim_end_matches('/'))).await,
        runtime::wait_http("chrome", chrome_url).await,
        runtime::wait_http("axon", "http://127.0.0.1:8001/readyz").await,
        runtime::prewarm_tei(&tei_url).await,
        runtime::run_smoke(
            "crawl-smoke",
            ["crawl", "https://example.com", "--wait", "true"],
        )
        .await,
        runtime::run_smoke("ask-smoke", ["ask", "What did we crawl?"]).await,
    ]
}

fn env_value(env_values: &BTreeMap<String, String>, key: &str, default: &str) -> String {
    env_values
        .get(key)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or(default)
        .to_string()
}

fn run_filesystem_phase(axon_home: &Path, mode: LocalSetupMode) -> io::Result<LocalSetupPhase> {
    let timer = PhaseTimer::start("filesystem");
    if mode.mutates() {
        ensure_private_dir(axon_home)?;
        for child in [
            "output",
            "logs",
            "artifacts",
            "screenshots",
            "chrome-diagnostics",
            "lab-auth",
            "tei",
            "qdrant",
        ] {
            ensure_private_dir(&axon_home.join(child))?;
        }
        Ok(timer.finish(
            LocalSetupStatus::Ok,
            format!("initialized {}", axon_home.display()),
        ))
    } else {
        Ok(timer.finish(
            if axon_home.exists() {
                LocalSetupStatus::Ok
            } else {
                LocalSetupStatus::Warn
            },
            if axon_home.exists() {
                format!("found {}", axon_home.display())
            } else {
                format!("missing {}; run axon setup", axon_home.display())
            },
        ))
    }
}

async fn check_command<const N: usize>(name: &'static str, args: [&str; N]) -> LocalSetupPhase {
    let timer = PhaseTimer::start(name);
    let result = if name == "docker compose" {
        super::diagnostics::check_command("docker", args, Duration::from_secs(10)).await
    } else {
        super::diagnostics::check_command(name, args, Duration::from_secs(10)).await
    };

    let status = match result.status {
        super::diagnostics::CommandStatus::Ok => LocalSetupStatus::Ok,
        super::diagnostics::CommandStatus::Failed
        | super::diagnostics::CommandStatus::NotFound
        | super::diagnostics::CommandStatus::TimedOut => LocalSetupStatus::Error,
    };
    timer.finish(status, result.detail)
}

async fn check_gemini_cli() -> LocalSetupPhase {
    let timer = PhaseTimer::start("gemini");
    let mut cmd = Command::new("gemini");
    cmd.arg("--version");
    match tokio::time::timeout(Duration::from_secs(10), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => timer.finish(
            LocalSetupStatus::Ok,
            format!(
                "gemini CLI present: {}; ask-smoke verifies auth/completion",
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .next()
                    .unwrap_or("version unavailable")
            ),
        ),
        Ok(Ok(output)) => timer.finish(
            LocalSetupStatus::Warn,
            format!(
                "gemini CLI version check failed: {}; ask-smoke verifies auth/completion",
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .next()
                    .unwrap_or("gemini --version failed")
            ),
        ),
        Ok(Err(err)) if err.kind() == ErrorKind::NotFound => timer.finish(
            LocalSetupStatus::Warn,
            "gemini CLI not found on PATH; ask-smoke is the auth/completion proof",
        ),
        Ok(Err(err)) => timer.finish(
            LocalSetupStatus::Warn,
            format!("gemini CLI check failed: {err}; ask-smoke verifies auth/completion"),
        ),
        Err(_) => timer.finish(
            LocalSetupStatus::Warn,
            "gemini CLI version check timed out; ask-smoke verifies auth/completion",
        ),
    }
}

fn check_oauth_config() -> LocalSetupPhase {
    let timer = PhaseTimer::start("oauth");
    match std::env::var("AXON_MCP_AUTH_MODE") {
        Ok(value) if value.trim().eq_ignore_ascii_case("oauth") => {
            let missing: Vec<&str> = [
                "AXON_MCP_PUBLIC_URL",
                "AXON_MCP_GOOGLE_CLIENT_ID",
                "AXON_MCP_GOOGLE_CLIENT_SECRET",
                "AXON_MCP_AUTH_ADMIN_EMAIL",
            ]
            .into_iter()
            .filter(|key| {
                std::env::var(key)
                    .ok()
                    .is_none_or(|value| value.trim().is_empty())
            })
            .collect();
            if missing.is_empty() {
                timer.finish(LocalSetupStatus::Ok, "oauth mode configured")
            } else {
                timer.finish(
                    LocalSetupStatus::Error,
                    format!("missing {}", missing.join(", ")),
                )
            }
        }
        _ => timer.finish(
            LocalSetupStatus::Ok,
            "static bearer token mode; OAuth not requested",
        ),
    }
}
