use crate::core::config::SessionWatchServiceAction;
use crate::core::paths::axon_home_dir;
use crate::ingest::sessions::watch::validate::{SessionWatchRoots, validate_session_file_path};
use crate::services::setup::{LocalSetupPhase, LocalSetupStatus};
use serde::Serialize;
use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

const SERVICE_NAME: &str = "session-watch-service";
const UNIT_NAME: &str = "session-watch-service.service";

#[derive(Debug, Clone, Serialize)]
pub struct SessionWatchServiceReport {
    pub action: SessionWatchServiceAction,
    pub service_name: &'static str,
    pub unit_name: &'static str,
    pub unit_path: PathBuf,
    pub env_path: PathBuf,
    pub axon_bin: PathBuf,
    pub phases: Vec<LocalSetupPhase>,
    pub has_errors: bool,
}

#[derive(Debug, Clone)]
struct ServicePaths {
    home: PathBuf,
    state_dir: PathBuf,
    env_path: PathBuf,
    unit_path: PathBuf,
    axon_bin: PathBuf,
    sqlite_path: PathBuf,
}

pub async fn run_session_watch_service_setup(
    action: SessionWatchServiceAction,
) -> io::Result<SessionWatchServiceReport> {
    let paths = ServicePaths::resolve()?;
    let phases = match action {
        SessionWatchServiceAction::Install => install(&paths).await?,
        SessionWatchServiceAction::Check => check(&paths),
        SessionWatchServiceAction::Remove => remove(&paths)?,
        SessionWatchServiceAction::Status => status(&paths),
    };
    let has_errors = phases.iter().any(|phase| phase_is_failure(action, phase));
    Ok(SessionWatchServiceReport {
        action,
        service_name: SERVICE_NAME,
        unit_name: UNIT_NAME,
        unit_path: paths.unit_path,
        env_path: paths.env_path,
        axon_bin: paths.axon_bin,
        phases,
        has_errors,
    })
}

async fn install(paths: &ServicePaths) -> io::Result<Vec<LocalSetupPhase>> {
    let mut phases = vec![write_service_files(paths)?];
    for (name, spec) in [
        ("initial-ingest", initial_ingest_command(paths)),
        ("systemd-reload", daemon_reload_command()),
        ("enable-service", enable_now_command()),
    ] {
        phases.push(if name == "initial-ingest" {
            initial_ingest_phase(paths, spec)
        } else {
            run_command_phase(name, spec)
        });
        if phases
            .last()
            .is_some_and(|phase| matches!(phase.status, LocalSetupStatus::Error))
        {
            break;
        }
    }
    Ok(phases)
}

fn check(paths: &ServicePaths) -> Vec<LocalSetupPhase> {
    vec![
        file_check_phase("env-file", &paths.env_path),
        file_check_phase("unit-file", &paths.unit_path),
        command_status_phase("service-enabled", is_enabled_command()),
        command_status_phase("service-active", is_active_command()),
    ]
}

fn remove(paths: &ServicePaths) -> io::Result<Vec<LocalSetupPhase>> {
    Ok(vec![
        run_command_phase("disable-service", disable_now_command()),
        remove_file_phase("unit-file", &paths.unit_path),
        remove_file_phase("env-file", &paths.env_path),
        run_command_phase("systemd-reload", daemon_reload_command()),
    ])
}

fn status(_paths: &ServicePaths) -> Vec<LocalSetupPhase> {
    vec![command_status_phase("service-status", status_command())]
}

fn write_service_files(paths: &ServicePaths) -> io::Result<LocalSetupPhase> {
    let started = Instant::now();
    std::fs::create_dir_all(paths.env_path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "session watch env path has no parent",
        )
    })?)?;
    std::fs::create_dir_all(paths.unit_path.parent().ok_or_else(|| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "session watch unit path has no parent",
        )
    })?)?;
    std::fs::create_dir_all(&paths.state_dir)?;
    std::fs::write(&paths.env_path, session_watch_env_file(&paths.sqlite_path))?;
    std::fs::write(
        &paths.unit_path,
        session_watch_service_unit(
            &paths.axon_bin,
            &paths.env_path,
            &paths.state_dir,
            &paths.home,
        ),
    )?;
    Ok(phase(
        "write-files",
        LocalSetupStatus::Ok,
        format!("wrote {}", paths.unit_path.display()),
        started,
    ))
}

pub(crate) fn session_watch_env_file(sqlite_path: &Path) -> String {
    format!(
        "AXON_SQLITE_PATH={}\nRUST_LOG=warn\n",
        sqlite_path.display()
    )
}

pub(crate) fn session_watch_service_unit(
    axon_bin: &Path,
    env_path: &Path,
    state_dir: &Path,
    home: &Path,
) -> String {
    let axon_home = home.join(".axon");
    let write_paths = [
        state_dir.to_path_buf(),
        axon_home.join("jobs.db"),
        axon_home.join("output"),
        axon_home.join("logs"),
        axon_home.join("artifacts"),
        axon_home.join("screenshots"),
        axon_home.join("chrome-diagnostics"),
    ]
    .into_iter()
    .map(|path| format!("-{}", path.display()))
    .collect::<Vec<_>>()
    .join(" ");
    format!(
        r#"[Unit]
Description=axon real-time local AI session watch
After=default.target

[Service]
Type=simple
EnvironmentFile={}
ExecStart={} sessions watch --no-initial-scan --json
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=tmpfs
BindReadOnlyPaths=-{} -{} -{} -{} -{} -{}
ReadWritePaths={}
StateDirectory=axon
SyslogIdentifier=session-watch-service

[Install]
WantedBy=default.target
"#,
        env_path.display(),
        axon_bin.display(),
        axon_bin.display(),
        env_path.display(),
        home.join(".claude/projects").display(),
        home.join(".codex/sessions").display(),
        home.join(".gemini/history").display(),
        home.join(".gemini/tmp").display(),
        write_paths,
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandSpec {
    program: String,
    args: Vec<String>,
}

impl CommandSpec {
    fn new(program: impl Into<String>, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().map(Into::into).collect(),
        }
    }
}

fn initial_ingest_command(paths: &ServicePaths) -> CommandSpec {
    CommandSpec::new(
        paths.axon_bin.display().to_string(),
        ["sessions", "--wait", "true", "--json"],
    )
}

fn daemon_reload_command() -> CommandSpec {
    CommandSpec::new("systemctl", ["--user", "daemon-reload"])
}

fn enable_now_command() -> CommandSpec {
    CommandSpec::new("systemctl", ["--user", "enable", "--now", UNIT_NAME])
}

fn disable_now_command() -> CommandSpec {
    CommandSpec::new("systemctl", ["--user", "disable", "--now", UNIT_NAME])
}

fn is_enabled_command() -> CommandSpec {
    CommandSpec::new("systemctl", ["--user", "is-enabled", UNIT_NAME])
}

fn is_active_command() -> CommandSpec {
    CommandSpec::new("systemctl", ["--user", "is-active", UNIT_NAME])
}

fn status_command() -> CommandSpec {
    CommandSpec::new("systemctl", ["--user", "status", "--no-pager", UNIT_NAME])
}

fn run_command_phase(name: &'static str, spec: CommandSpec) -> LocalSetupPhase {
    run_command_capture(name, spec).phase
}

struct CommandPhaseOutput {
    phase: LocalSetupPhase,
    stdout: String,
}

fn run_command_capture(name: &'static str, spec: CommandSpec) -> CommandPhaseOutput {
    let started = Instant::now();
    match Command::new(&spec.program).args(&spec.args).output() {
        Ok(output) if output.status.success() => CommandPhaseOutput {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            phase: phase(
                name,
                LocalSetupStatus::Ok,
                command_detail(&spec, &output.stdout),
                started,
            ),
        },
        Ok(output) => CommandPhaseOutput {
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            phase: phase(
                name,
                LocalSetupStatus::Error,
                command_detail(&spec, &output.stderr),
                started,
            ),
        },
        Err(error) => CommandPhaseOutput {
            stdout: String::new(),
            phase: phase(name, LocalSetupStatus::Error, error.to_string(), started),
        },
    }
}

fn initial_ingest_phase(paths: &ServicePaths, spec: CommandSpec) -> LocalSetupPhase {
    let captured = run_command_capture("initial-ingest", spec);
    let phase = captured.phase;
    if !matches!(phase.status, LocalSetupStatus::Ok) {
        return phase;
    }
    if session_files_exist(&paths.home) && command_json_chunks(&captured.stdout) == Some(0) {
        return LocalSetupPhase {
            name: phase.name,
            status: LocalSetupStatus::Error,
            detail: format!(
                "{}; found local transcript files but initial ingest embedded 0 chunks",
                phase.detail
            ),
            elapsed_ms: phase.elapsed_ms,
        };
    }
    phase
}

fn command_status_phase(name: &'static str, spec: CommandSpec) -> LocalSetupPhase {
    let started = Instant::now();
    match Command::new(&spec.program).args(&spec.args).output() {
        Ok(output) if output.status.success() => phase(
            name,
            LocalSetupStatus::Ok,
            command_detail(&spec, &output.stdout),
            started,
        ),
        Ok(output) => phase(
            name,
            LocalSetupStatus::Warn,
            command_detail(&spec, &output.stderr),
            started,
        ),
        Err(error) => phase(name, LocalSetupStatus::Warn, error.to_string(), started),
    }
}

fn command_json_chunks(detail: &str) -> Option<u64> {
    let value = serde_json::from_str::<serde_json::Value>(detail).ok()?;
    value
        .get("chunks_embedded")
        .or_else(|| value.get("chunks"))
        .and_then(serde_json::Value::as_u64)
}

fn session_files_exist(home: &Path) -> bool {
    let roots = SessionWatchRoots::for_home(home);
    [
        &roots.claude_projects,
        &roots.codex_sessions,
        &roots.gemini_history,
        &roots.gemini_tmp,
    ]
    .into_iter()
    .any(|root| directory_contains_supported_session_file(&roots, root))
}

fn directory_contains_supported_session_file(roots: &SessionWatchRoots, root: &Path) -> bool {
    let mut stack = vec![root.to_path_buf()];
    let mut visited = 0usize;
    while let Some(path) = stack.pop() {
        visited += 1;
        if visited > 8192 {
            return true;
        }
        let Ok(metadata) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_file() {
            return validate_session_file_path(roots, &path).is_ok();
        }
        if !metadata.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&path) else {
            continue;
        };
        stack.extend(entries.filter_map(Result::ok).map(|entry| entry.path()));
    }
    false
}

fn file_check_phase(name: &'static str, path: &Path) -> LocalSetupPhase {
    let started = Instant::now();
    if path.exists() {
        phase(
            name,
            LocalSetupStatus::Ok,
            format!("found {}", path.display()),
            started,
        )
    } else {
        phase(
            name,
            LocalSetupStatus::Warn,
            format!("missing {}", path.display()),
            started,
        )
    }
}

fn remove_file_phase(name: &'static str, path: &Path) -> LocalSetupPhase {
    let started = Instant::now();
    match std::fs::remove_file(path) {
        Ok(()) => phase(
            name,
            LocalSetupStatus::Ok,
            format!("removed {}", path.display()),
            started,
        ),
        Err(error) if error.kind() == ErrorKind::NotFound => phase(
            name,
            LocalSetupStatus::Ok,
            format!("already absent {}", path.display()),
            started,
        ),
        Err(error) => phase(name, LocalSetupStatus::Error, error.to_string(), started),
    }
}

fn phase_is_failure(action: SessionWatchServiceAction, phase: &LocalSetupPhase) -> bool {
    match action {
        SessionWatchServiceAction::Check | SessionWatchServiceAction::Status => {
            matches!(
                phase.status,
                LocalSetupStatus::Warn | LocalSetupStatus::Error
            )
        }
        SessionWatchServiceAction::Install | SessionWatchServiceAction::Remove => {
            matches!(phase.status, LocalSetupStatus::Error)
        }
    }
}

fn command_detail(spec: &CommandSpec, bytes: &[u8]) -> String {
    let output = String::from_utf8_lossy(bytes).trim().to_string();
    if output.is_empty() {
        format!("ran {} {}", spec.program, spec.args.join(" "))
    } else {
        format!("{}: {}", spec.program, output)
    }
}

fn phase(
    name: &'static str,
    status: LocalSetupStatus,
    detail: impl Into<String>,
    started: Instant,
) -> LocalSetupPhase {
    LocalSetupPhase {
        name,
        status,
        detail: detail.into(),
        elapsed_ms: started.elapsed().as_millis(),
    }
}

impl ServicePaths {
    fn resolve() -> io::Result<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::NotFound,
                    "HOME is unset or invalid; cannot resolve service paths",
                )
            })?;
        let axon_home = axon_home_dir().ok_or_else(|| {
            io::Error::new(
                ErrorKind::NotFound,
                "HOME is unset or invalid; cannot resolve ~/.axon",
            )
        })?;
        let axon_bin = std::env::current_exe()?;
        let state_dir = home.join(".local/state/axon");
        Ok(Self {
            env_path: home.join(".config/axon/session-watch.env"),
            unit_path: home.join(".config/systemd/user").join(UNIT_NAME),
            sqlite_path: axon_home.join("jobs.db"),
            home,
            state_dir,
            axon_bin,
        })
    }
}

#[cfg(test)]
#[path = "session_watch_service_tests.rs"]
mod tests;
