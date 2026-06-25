use super::roots::discover_code_search_watch_roots_for_dirs;
use axon_core::config::CodeSearchWatchConfig;
use axon_core::paths::axon_home_dir;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CodeSearchWatchEnableReport {
    pub unit_path: PathBuf,
    pub env_path: PathBuf,
    pub roots: Vec<PathBuf>,
}

pub(super) fn enable_code_search_watch_service(
    watch_dirs: &[PathBuf],
    options: &CodeSearchWatchConfig,
) -> io::Result<CodeSearchWatchEnableReport> {
    let roots = discover_code_search_watch_roots_for_dirs(watch_dirs)
        .map_err(|error| io::Error::other(error.to_string()))?;
    if roots.is_empty() {
        return Err(io::Error::other(
            "code-search-watch found no Git checkouts to enable",
        ));
    }
    let config_dir = axon_home_dir()
        .ok_or_else(|| io::Error::other("cannot determine AXON home directory"))?
        .join("config");
    std::fs::create_dir_all(&config_dir)?;
    let env_path = config_dir.join("code-search-watch.env");
    let unit_dir = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config/systemd/user");
    std::fs::create_dir_all(&unit_dir)?;
    let unit_path = unit_dir.join("code-search-watch.service");
    let axon_bin = std::env::current_exe()?;
    std::fs::write(&env_path, "RUST_LOG=warn\n")?;
    std::fs::write(
        &unit_path,
        code_search_watch_service_unit(&axon_bin, &env_path, watch_dirs, options),
    )?;
    run_systemctl(["--user", "daemon-reload"])?;
    run_systemctl(["--user", "enable", "--now", "code-search-watch.service"])?;
    Ok(CodeSearchWatchEnableReport {
        unit_path,
        env_path,
        roots,
    })
}

fn code_search_watch_service_unit(
    axon_bin: &Path,
    env_path: &Path,
    watch_dirs: &[PathBuf],
    options: &CodeSearchWatchConfig,
) -> String {
    let mut args = String::new();
    for dir in watch_dirs {
        args.push_str(" --cwd ");
        args.push_str(&dir.display().to_string());
    }
    args.push_str(" --debounce-ms ");
    args.push_str(&options.debounce.as_millis().to_string());
    args.push_str(" --settle-ms ");
    args.push_str(&options.settle.as_millis().to_string());
    if options.initial_refresh {
        args.push_str(" --initial-refresh");
    }
    if options.json {
        args.push_str(" --json");
    }
    format!(
        r#"[Unit]
Description=axon local code-search watch
After=default.target

[Service]
Type=simple
EnvironmentFile={}
ExecStart={} code-search-watch{}
Restart=on-failure
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true
SyslogIdentifier=code-search-watch

[Install]
WantedBy=default.target
"#,
        env_path.display(),
        axon_bin.display(),
        args,
    )
}

fn run_systemctl<const N: usize>(args: [&str; N]) -> io::Result<()> {
    let status = Command::new("systemctl").args(args).status()?;
    if status.success() {
        return Ok(());
    }
    Err(io::Error::other(format!(
        "systemctl {} failed with status {status}",
        args.join(" ")
    )))
}
