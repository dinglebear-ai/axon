use super::config_store;
use crate::core::paths::{axon_home_dir, ensure_private_dir};
use serde::Serialize;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command;

mod compose;
mod env;

const SETUP_TARGET_SECS: u64 = 120;
const SETUP_HARD_MAX_SECS: u64 = 300;
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
    pub token_path: PathBuf,
    pub phases: Vec<LocalSetupPhase>,
}

struct PhaseTimer {
    name: &'static str,
    start: Instant,
}

impl PhaseTimer {
    fn start(name: &'static str) -> Self {
        Self {
            name,
            start: Instant::now(),
        }
    }

    fn finish(self, status: LocalSetupStatus, detail: impl Into<String>) -> LocalSetupPhase {
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
    let token_path = env_path.clone();
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

    if mode.mutates() {
        phases.push(env::ensure_env_file(&env_path)?);
        phases.push(compose::write_compose_assets(&compose_dir)?);
    } else {
        phases.push(env::check_env_file(&env_path));
        phases.push(compose::check_compose_assets(&compose_dir));
    }

    phases.push(check_command("docker", ["--version"]).await);
    phases.push(check_command("docker compose", ["compose", "version"]).await);
    phases.push(check_command("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"]).await);
    phases.push(check_gemini_auth().await);
    phases.push(check_oauth_config());

    if mode.mutates() {
        phases.push(run_compose(&compose_dir, &env_path, ["pull"]).await);
        phases.push(run_compose(&compose_dir, &env_path, ["up", "-d"]).await);
        phases.push(wait_http("qdrant", "http://127.0.0.1:53333/readyz").await);
        phases.push(wait_http("tei", "http://127.0.0.1:52000/health").await);
        phases.push(wait_http("chrome", "http://127.0.0.1:6000/").await);
        phases.push(wait_http("axon", "http://127.0.0.1:8001/healthz").await);
        phases.push(prewarm_tei().await);
        phases.push(
            run_smoke(
                "crawl-smoke",
                ["crawl", "https://example.com", "--wait", "true"],
            )
            .await,
        );
        phases.push(run_smoke("ask-smoke", ["ask", "What did we crawl?"]).await);
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
        token_path,
        phases,
    })
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
    let mut command = if name == "docker compose" {
        let mut cmd = Command::new("docker");
        cmd.args(args);
        cmd
    } else {
        let mut cmd = Command::new(name);
        cmd.args(args);
        cmd
    };
    match tokio::time::timeout(Duration::from_secs(10), command.output()).await {
        Ok(Ok(output)) if output.status.success() => timer.finish(
            LocalSetupStatus::Ok,
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("available")
                .to_string(),
        ),
        Ok(Ok(output)) => timer.finish(
            LocalSetupStatus::Error,
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("command failed")
                .to_string(),
        ),
        Ok(Err(err)) if err.kind() == ErrorKind::NotFound => {
            timer.finish(LocalSetupStatus::Error, "not found on PATH")
        }
        Ok(Err(err)) => timer.finish(LocalSetupStatus::Error, err.to_string()),
        Err(_) => timer.finish(LocalSetupStatus::Error, "timed out"),
    }
}

async fn check_gemini_auth() -> LocalSetupPhase {
    let timer = PhaseTimer::start("gemini");
    let home = std::env::var("HOME").unwrap_or_default();
    let gemini_home = std::env::var("AXON_HEADLESS_GEMINI_HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(home).join(".gemini"));
    let mut cmd = Command::new("gemini");
    cmd.arg("--version");
    match tokio::time::timeout(Duration::from_secs(10), cmd.output()).await {
        Ok(Ok(output)) if output.status.success() && gemini_home.exists() => timer.finish(
            LocalSetupStatus::Ok,
            format!("gemini CLI available; auth home {}", gemini_home.display()),
        ),
        Ok(Ok(output)) if output.status.success() => timer.finish(
            LocalSetupStatus::Warn,
            format!(
                "gemini CLI available, but {} is missing",
                gemini_home.display()
            ),
        ),
        Ok(Ok(output)) => timer.finish(
            LocalSetupStatus::Error,
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .next()
                .unwrap_or("gemini --version failed")
                .to_string(),
        ),
        Ok(Err(err)) if err.kind() == ErrorKind::NotFound => {
            timer.finish(LocalSetupStatus::Error, "gemini CLI not found on PATH")
        }
        Ok(Err(err)) => timer.finish(LocalSetupStatus::Error, err.to_string()),
        Err(_) => timer.finish(LocalSetupStatus::Error, "timed out"),
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

async fn run_compose<const N: usize>(
    compose_dir: &Path,
    env_path: &Path,
    args: [&str; N],
) -> LocalSetupPhase {
    let timer = PhaseTimer::start(if args.first() == Some(&"pull") {
        "compose-pull"
    } else {
        "compose-up"
    });
    let mut cmd = Command::new("docker");
    cmd.arg("compose")
        .arg("--env-file")
        .arg(env_path)
        .arg("-f")
        .arg(compose_dir.join("docker-compose.yaml"))
        .args(args)
        .current_dir(compose_dir);
    run_timed_command(timer, cmd, Duration::from_secs(SETUP_HARD_MAX_SECS)).await
}

async fn wait_http(name: &'static str, url: &'static str) -> LocalSetupPhase {
    let timer = PhaseTimer::start(name);
    let client = reqwest::Client::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match client.get(url).send().await {
            Ok(response) if response.status().is_success() => {
                return timer.finish(LocalSetupStatus::Ok, format!("{url} ready"));
            }
            _ if Instant::now() < deadline => tokio::time::sleep(Duration::from_secs(2)).await,
            Ok(response) => {
                return timer.finish(
                    LocalSetupStatus::Error,
                    format!("{url} returned {}", response.status()),
                );
            }
            Err(err) => return timer.finish(LocalSetupStatus::Error, err.to_string()),
        }
    }
}

async fn prewarm_tei() -> LocalSetupPhase {
    let timer = PhaseTimer::start("tei-prewarm");
    let client = reqwest::Client::new();
    match tokio::time::timeout(
        Duration::from_secs(120),
        client
            .post("http://127.0.0.1:52000/embed")
            .json(&serde_json::json!({ "inputs": "axon setup warmup" }))
            .send(),
    )
    .await
    {
        Ok(Ok(response)) if response.status().is_success() => {
            timer.finish(LocalSetupStatus::Ok, "Qwen3 embedding model warmed")
        }
        Ok(Ok(response)) => timer.finish(
            LocalSetupStatus::Error,
            format!("TEI warmup returned {}", response.status()),
        ),
        Ok(Err(err)) => timer.finish(LocalSetupStatus::Error, err.to_string()),
        Err(_) => timer.finish(LocalSetupStatus::Error, "timed out"),
    }
}

async fn run_smoke<const N: usize>(name: &'static str, args: [&str; N]) -> LocalSetupPhase {
    if std::env::var("AXON_SETUP_SKIP_SMOKE").ok().as_deref() == Some("1") {
        return LocalSetupPhase {
            name,
            status: LocalSetupStatus::Skipped,
            detail: "AXON_SETUP_SKIP_SMOKE=1".to_string(),
            elapsed_ms: 0,
        };
    }
    let timer = PhaseTimer::start(name);
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return timer.finish(LocalSetupStatus::Error, err.to_string()),
    };
    let mut cmd = Command::new(exe);
    cmd.args(args);
    run_timed_command(timer, cmd, Duration::from_secs(60)).await
}

async fn run_timed_command(
    timer: PhaseTimer,
    mut cmd: Command,
    timeout: Duration,
) -> LocalSetupPhase {
    match tokio::time::timeout(timeout, cmd.output()).await {
        Ok(Ok(output)) if output.status.success() => timer.finish(
            LocalSetupStatus::Ok,
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .last()
                .unwrap_or("ok")
                .to_string(),
        ),
        Ok(Ok(output)) => timer.finish(LocalSetupStatus::Error, command_failure_detail(&output)),
        Ok(Err(err)) => timer.finish(LocalSetupStatus::Error, err.to_string()),
        Err(_) => timer.finish(LocalSetupStatus::Error, "timed out"),
    }
}

fn command_failure_detail(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Some(line) = stderr.lines().last() {
        return line.to_string();
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .last()
        .unwrap_or("command failed")
        .to_string()
}
