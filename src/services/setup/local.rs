use super::config_store;
use crate::core::paths::{axon_home_dir, ensure_private_dir};
use model::PhaseTimer;
pub use model::{
    LocalSetupInitOptions, LocalSetupMode, LocalSetupPhase, LocalSetupReport, LocalSetupStatus,
    StackAction,
};
use std::collections::BTreeMap;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::time::Instant;

mod compose;
mod env;
mod model;
mod preflight;
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

pub async fn run_local_setup(mode: LocalSetupMode) -> io::Result<LocalSetupReport> {
    run_local_setup_with_options(mode, LocalSetupInitOptions::default()).await
}

pub async fn run_local_setup_with_options(
    mode: LocalSetupMode,
    options: LocalSetupInitOptions,
) -> io::Result<LocalSetupReport> {
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
    let env_state = run_env_and_compose_phases(
        mode,
        &env_path,
        &compose_dir,
        &mut phases,
        &options.into_env_options(),
    )?;

    let env_values = env_state.finalized.as_ref().or(env_state.checked.as_ref());
    match mode {
        LocalSetupMode::Init => {}
        LocalSetupMode::Smoke => {
            if phase_errors(&phases) || local_surface_incomplete(&phases) {
                phases.push(skipped_phase(
                    "smoke",
                    "smoke skipped because env or compose checks failed",
                ));
            } else if let Some(env_values) = env_values {
                phases.extend(run_smoke_phases(env_values).await);
            }
        }
        LocalSetupMode::Preflight => {
            let local_incomplete = local_surface_incomplete(&phases);
            phases.extend(run_preflight_check_phases(env_values).await);
            if phase_errors(&phases) || local_incomplete {
                phases.push(skipped_phase(
                    "readiness",
                    "readiness skipped because prerequisite checks failed",
                ));
            } else if let Some(env_values) = env_values {
                phases.extend(run_readiness_phases(env_values).await);
            }
        }
        LocalSetupMode::Setup => {
            phases.extend(run_preflight_check_phases(env_values).await);
            if phase_errors(&phases) {
                phases.push(skipped_phase(
                    "stack-up",
                    "stack startup skipped because prerequisite checks failed",
                ));
                phases.push(skipped_phase(
                    "readiness",
                    "readiness skipped because prerequisite checks failed",
                ));
            } else if let Some(env_values) = env_values {
                phases.extend(run_stack_up_phases(&compose_dir, &env_path, false).await);
                if phase_errors(&phases) {
                    phases.push(skipped_phase(
                        "readiness",
                        "readiness skipped because stack startup failed",
                    ));
                } else {
                    phases.extend(run_readiness_phases(env_values).await);
                }
            }
        }
    }

    Ok(build_report(
        mode.as_str(),
        started,
        axon_home,
        env_path,
        config_init.path,
        compose_dir,
        phases,
    ))
}

pub async fn run_stack_action(action: StackAction) -> io::Result<LocalSetupReport> {
    let started = Instant::now();
    let axon_home = axon_home_dir().ok_or_else(|| {
        io::Error::new(
            ErrorKind::NotFound,
            "HOME is unset or invalid; cannot resolve ~/.axon",
        )
    })?;
    let env_path = axon_home.join(".env");
    let compose_dir = axon_home.join("compose");
    let mut phases = Vec::new();
    phases.push(run_filesystem_phase(&axon_home, LocalSetupMode::Preflight)?);
    let config_init = run_config_phase(LocalSetupMode::Preflight, &mut phases)?;
    phases.push(env::check_env_file(&env_path));
    phases.push(compose::check_compose_assets(&compose_dir));

    // Also gate on Warn-level missing assets (check_env_file / check_compose_assets
    // return Warn, not Error, when files are absent on a fresh machine).
    if phase_errors(&phases) || local_surface_incomplete(&phases) {
        phases.push(skipped_phase(
            action.as_str(),
            "stack command skipped because env or compose assets are missing; run axon setup init",
        ));
    } else {
        phases.extend(match action {
            // Pass follow_logs=false so `stack up` returns once containers are
            // started rather than blocking on `docker compose logs -f` indefinitely.
            StackAction::Up => run_stack_up_phases(&compose_dir, &env_path, false).await,
            StackAction::Down => {
                vec![runtime::run_compose(&compose_dir, &env_path, ["down"]).await]
            }
            StackAction::Restart => {
                vec![runtime::run_compose(&compose_dir, &env_path, ["restart"]).await]
            }
            StackAction::Rebuild => {
                vec![
                    runtime::run_compose(&compose_dir, &env_path, ["build"]).await,
                    runtime::run_compose(&compose_dir, &env_path, ["up", "-d"]).await,
                ]
            }
        });
    }

    Ok(build_report(
        action.as_str(),
        started,
        axon_home,
        env_path,
        config_init.path,
        compose_dir,
        phases,
    ))
}

fn build_report(
    mode: &'static str,
    started: Instant,
    axon_home: PathBuf,
    env_path: PathBuf,
    config_path: PathBuf,
    compose_dir: PathBuf,
    phases: Vec<LocalSetupPhase>,
) -> LocalSetupReport {
    let elapsed_ms = started.elapsed().as_millis();
    let has_errors = phase_errors(&phases);
    LocalSetupReport {
        mode,
        elapsed_ms,
        target_seconds: SETUP_TARGET_SECS,
        hard_max_seconds: SETUP_HARD_MAX_SECS,
        met_target: elapsed_ms <= u128::from(SETUP_TARGET_SECS) * 1000,
        exceeded_hard_max: elapsed_ms > u128::from(SETUP_HARD_MAX_SECS) * 1000,
        axon_home,
        env_path,
        config_path,
        compose_dir,
        web_panel_url: DEFAULT_SERVER_URL.to_string(),
        mcp_url: format!("{DEFAULT_SERVER_URL}/mcp"),
        phases,
        has_errors,
    }
}

struct EnvPhaseState {
    finalized: Option<BTreeMap<String, String>>,
    checked: Option<BTreeMap<String, String>>,
}

impl LocalSetupInitOptions {
    fn into_env_options(self) -> env::EnvSetupOptions {
        env::EnvSetupOptions {
            mcp_host: self.mcp_host,
            mcp_port: self.mcp_port,
            auth_mode: self.auth_mode,
            mcp_token: self.mcp_token,
            oauth_public_url: self.oauth_public_url,
            google_client_id: self.google_client_id,
            google_client_secret: self.google_client_secret,
            auth_admin_email: self.auth_admin_email,
            tavily_api_key: self.tavily_api_key,
            github_token: self.github_token,
            reddit_client_id: self.reddit_client_id,
            reddit_client_secret: self.reddit_client_secret,
        }
    }
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
            format!("missing {}; run axon setup init", path.display()),
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
    options: &env::EnvSetupOptions,
) -> io::Result<EnvPhaseState> {
    if mode.mutates() {
        let env_result = env::ensure_env_file_with_options(env_path, options)?;
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

async fn run_preflight_check_phases(
    env_values: Option<&BTreeMap<String, String>>,
) -> Vec<LocalSetupPhase> {
    vec![
        preflight::check_command("docker", ["--version"]).await,
        preflight::check_command("docker compose", ["compose", "version"]).await,
        preflight::check_command("nvidia-smi", ["--query-gpu=name", "--format=csv,noheader"]).await,
        preflight::check_gemini_cli().await,
        preflight::check_oauth_config(env_values),
    ]
}

async fn run_stack_up_phases(
    compose_dir: &Path,
    env_path: &Path,
    follow_logs: bool,
) -> Vec<LocalSetupPhase> {
    let mut phases = vec![
        runtime::run_compose(compose_dir, env_path, ["pull"]).await,
        runtime::run_compose(compose_dir, env_path, ["up", "-d"]).await,
    ];
    if follow_logs {
        phases.push(runtime::follow_logs(compose_dir, env_path).await);
    }
    phases
}

async fn run_readiness_phases(env_values: &BTreeMap<String, String>) -> Vec<LocalSetupPhase> {
    let qdrant_url = env_value(env_values, "QDRANT_URL", DEFAULT_QDRANT_URL);
    let tei_url = env_value(env_values, "TEI_URL", DEFAULT_TEI_URL);
    let chrome_url = env_value(env_values, "AXON_CHROME_REMOTE_URL", DEFAULT_CHROME_URL);

    vec![
        runtime::wait_http(
            "qdrant",
            format!("{}/readyz", qdrant_url.trim_end_matches('/')),
        )
        .await,
        runtime::wait_http("tei", format!("{}/health", tei_url.trim_end_matches('/'))).await,
        runtime::wait_http("chrome", chrome_url).await,
        runtime::wait_http("axon", "http://127.0.0.1:8001/readyz").await,
    ]
}

async fn run_smoke_phases(env_values: &BTreeMap<String, String>) -> Vec<LocalSetupPhase> {
    let tei_url = env_value(env_values, "TEI_URL", DEFAULT_TEI_URL);
    vec![
        runtime::prewarm_tei(&tei_url).await,
        runtime::run_smoke(
            "crawl-smoke",
            ["crawl", "https://example.com", "--wait", "true"],
        )
        .await,
        runtime::run_smoke("ask-smoke", ["ask", "What did we crawl?"]).await,
    ]
}

fn phase_errors(phases: &[LocalSetupPhase]) -> bool {
    phases
        .iter()
        .any(|phase| matches!(phase.status, LocalSetupStatus::Error))
}

fn local_surface_incomplete(phases: &[LocalSetupPhase]) -> bool {
    phases.iter().any(|phase| {
        matches!(
            phase.name,
            "filesystem" | "config" | "env" | "compose-assets"
        ) && !matches!(phase.status, LocalSetupStatus::Ok)
    })
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
                    "missing child directories under {}: {}; run axon setup init",
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
