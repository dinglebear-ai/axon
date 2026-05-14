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
mod env_migration;
mod runtime;

const SETUP_TARGET_SECS: u64 = 120;
pub(super) const SETUP_HARD_MAX_SECS: u64 = 300;
const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8001";
const DEFAULT_QDRANT_URL: &str = "http://127.0.0.1:53333";
const DEFAULT_TEI_URL: &str = "http://127.0.0.1:52000";
const DEFAULT_CHROME_URL: &str = "http://127.0.0.1:6000";
const REQUIRED_CHILD_DIRS: &[&str] = &[
    "output",
    "logs",
    "artifacts",
    "screenshots",
    "chrome-diagnostics",
    "lab-auth",
    "tei",
    "qdrant",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSetupMode {
    FirstRun,
    Check,
    Repair,
    MigrateEnv,
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
            Self::MigrateEnv => "migrate-env",
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

impl LocalSetupPhase {
    pub fn is_hook_advisory(&self) -> bool {
        matches!(self.name, "tei-prewarm" | "crawl-smoke" | "ask-smoke")
    }
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
    let config_init = run_config_phase(mode, &mut phases)?;
    let env_state = run_env_and_compose_phases(mode, &env_path, &compose_dir, &mut phases)?;

    phases.push(check_command("docker", ["--version"]).await);
    phases.push(check_command("docker compose", ["compose", "version"]).await);
    phases.push(check_command("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"]).await);
    phases.push(check_gemini_cli().await);
    let oauth_env = env_state.finalized.as_ref().or(env_state.checked.as_ref());
    phases.push(check_oauth_config(oauth_env));

    let prereq_failed = phases
        .iter()
        .any(|phase| matches!(phase.status, LocalSetupStatus::Error));
    if let Some(env_values) = env_state.finalized.as_ref().filter(|_| !prereq_failed) {
        phases.extend(run_mutating_runtime_phases(&compose_dir, &env_path, env_values).await);
    } else {
        let (compose_detail, smoke_detail) = if prereq_failed {
            (
                "setup skipped because earlier prerequisite checks failed",
                "smoke skipped because earlier prerequisite checks failed",
            )
        } else {
            (
                "check mode does not start Docker services",
                "check mode does not run crawl/ask smoke",
            )
        };
        phases.push(skipped_phase("compose-up", compose_detail));
        phases.push(skipped_phase("smoke", smoke_detail));
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

struct EnvPhaseState {
    finalized: Option<BTreeMap<String, String>>,
    checked: Option<BTreeMap<String, String>>,
}

fn run_config_phase(
    mode: LocalSetupMode,
    phases: &mut Vec<LocalSetupPhase>,
) -> io::Result<config_store::ConfigInit> {
    let timer = PhaseTimer::start("config");
    if mode.mutates() {
        let init = config_store::ensure_user_config()?;
        phases.push(timer.finish(
            LocalSetupStatus::Ok,
            if init.created {
                format!("created {}", init.path.display())
            } else {
                format!("preserved {}", init.path.display())
            },
        ));
        return Ok(init);
    }

    let path = crate::core::paths::axon_config_path().ok_or_else(|| {
        io::Error::new(
            ErrorKind::NotFound,
            "HOME is unset or invalid; cannot resolve ~/.axon/config.toml",
        )
    })?;
    let (status, detail) = if path.exists() {
        (LocalSetupStatus::Ok, format!("found {}", path.display()))
    } else {
        (
            LocalSetupStatus::Warn,
            format!(
                "missing {}; run axon setup or axon setup repair",
                path.display()
            ),
        )
    };
    phases.push(timer.finish(status, detail));
    Ok(config_store::ConfigInit {
        path,
        created: false,
    })
}

fn run_env_and_compose_phases(
    mode: LocalSetupMode,
    env_path: &Path,
    compose_dir: &Path,
    phases: &mut Vec<LocalSetupPhase>,
) -> io::Result<EnvPhaseState> {
    if mode.mutates() {
        let env_result = if matches!(mode, LocalSetupMode::MigrateEnv) {
            if !env_path.exists() {
                phases.push(env::ensure_env_file(env_path)?.phase);
            }
            env_migration::migrate_env_file(env_path)?
        } else {
            let result = env::ensure_env_file(env_path)?;
            env_migration::EnvMigrationResult {
                phase: result.phase,
                values: result.values,
            }
        };
        let finalized = Some(env_result.values);
        phases.push(env_result.phase);
        phases.push(compose::write_compose_assets(compose_dir)?);
        Ok(EnvPhaseState {
            finalized,
            checked: None,
        })
    } else {
        phases.push(env::check_env_file(env_path));
        phases.push(compose::check_compose_assets(compose_dir));
        Ok(EnvPhaseState {
            finalized: None,
            checked: Some(env::read_env_file_values(env_path)?),
        })
    }
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
        for child in REQUIRED_CHILD_DIRS {
            ensure_private_dir(&axon_home.join(child))?;
        }
        Ok(timer.finish(
            LocalSetupStatus::Ok,
            format!("initialized {}", axon_home.display()),
        ))
    } else {
        if !axon_home.exists() {
            return Ok(timer.finish(
                LocalSetupStatus::Warn,
                format!("missing {}; run axon setup", axon_home.display()),
            ));
        }
        let missing: Vec<&str> = REQUIRED_CHILD_DIRS
            .iter()
            .copied()
            .filter(|child| !axon_home.join(child).is_dir())
            .collect();
        if missing.is_empty() {
            Ok(timer.finish(
                LocalSetupStatus::Ok,
                format!(
                    "found {} with required child directories",
                    axon_home.display()
                ),
            ))
        } else {
            Ok(timer.finish(
                LocalSetupStatus::Warn,
                format!(
                    "missing child directories under {}: {}; run axon setup repair",
                    axon_home.display(),
                    missing.join(", ")
                ),
            ))
        }
    }
}

fn skipped_phase(name: &'static str, detail: &str) -> LocalSetupPhase {
    LocalSetupPhase {
        name,
        status: LocalSetupStatus::Skipped,
        detail: detail.to_string(),
        elapsed_ms: 0,
    }
}

async fn check_command<const N: usize>(name: &'static str, args: [&str; N]) -> LocalSetupPhase {
    let timer = PhaseTimer::start(name);
    // "docker compose" is a phase label; the actual binary to invoke is `docker`.
    let binary = if name == "docker compose" {
        "docker"
    } else {
        name
    };
    let result = super::diagnostics::check_command(binary, args, Duration::from_secs(10)).await;

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

fn check_oauth_config(env_values: Option<&BTreeMap<String, String>>) -> LocalSetupPhase {
    let timer = PhaseTimer::start("oauth");
    match setup_env_value(env_values, "AXON_MCP_AUTH_MODE") {
        Some(value) if value.trim().eq_ignore_ascii_case("oauth") => {
            let missing: Vec<&str> = [
                "AXON_MCP_PUBLIC_URL",
                "AXON_MCP_GOOGLE_CLIENT_ID",
                "AXON_MCP_GOOGLE_CLIENT_SECRET",
                "AXON_MCP_AUTH_ADMIN_EMAIL",
            ]
            .into_iter()
            .filter(|key| {
                setup_env_value(env_values, key).is_none_or(|value| value.trim().is_empty())
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

fn setup_env_value(env_values: Option<&BTreeMap<String, String>>, key: &str) -> Option<String> {
    env_values
        .and_then(|values| values.get(key).cloned())
        .or_else(|| std::env::var(key).ok())
}
