use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub(super) const DOCKER_SERVICES_FILE: &str = "docker-compose.services.yaml";
pub(super) const SERVE_CHILD_ROLE_ENV: &str = "AXON_SERVE_CHILD_ROLE";
pub(super) const SERVE_CHILD_ROLE_BRIDGE: &str = "bridge";
pub(super) const RESTART_BACKOFF_INITIAL_SECS: u64 = 1;
pub(super) const RESTART_BACKOFF_MAX_SECS: u64 = 30;
pub(super) const RESTART_STABLE_WINDOW_SECS: u64 = 30;
pub(super) const SHUTDOWN_GRACE_SECS: u64 = 5;
pub(super) const MAX_UNSTABLE_RESTARTS: usize = 3;
const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD: &str = "\x1b[1m";
const ANSI_DIM: &str = "\x1b[2m";
pub(super) const ANSI_RED: &str = "\x1b[31m";
pub(super) const ANSI_GREEN: &str = "\x1b[32m";
pub(super) const ANSI_YELLOW: &str = "\x1b[33m";
pub(super) const ANSI_BLUE: &str = "\x1b[34m";
pub(super) const ANSI_MAGENTA: &str = "\x1b[35m";
pub(super) const ANSI_CYAN: &str = "\x1b[36m";

pub(super) fn ansi_reset() -> &'static str {
    ANSI_RESET
}

pub(super) fn ansi_bold() -> &'static str {
    ANSI_BOLD
}

pub(super) fn ansi_dim() -> &'static str {
    ANSI_DIM
}

pub(super) fn child_color(name: &str) -> &'static str {
    match name {
        "serve-runtime" => ANSI_BLUE,
        "mcp-http" => ANSI_MAGENTA,
        "shell-server" => ANSI_CYAN,
        "nextjs" => ANSI_GREEN,
        "crawl-worker" => ANSI_YELLOW,
        "embed-worker" => ANSI_CYAN,
        "extract-worker" => ANSI_BLUE,
        "ingest-worker" => ANSI_MAGENTA,
        "refresh-worker" => ANSI_GREEN,
        "graph-worker" => ANSI_YELLOW,
        _ => ANSI_BLUE,
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct ComposeServiceStatus {
    #[serde(rename = "Service")]
    pub(super) service: String,
    #[serde(rename = "Name")]
    pub(super) name: String,
    #[serde(rename = "State")]
    pub(super) state: String,
    #[serde(rename = "Health", default)]
    pub(super) health: String,
    #[serde(rename = "Status", default)]
    pub(super) status: String,
}

impl ComposeServiceStatus {
    pub(super) fn is_healthy(&self) -> bool {
        if self.state != "running" {
            return false;
        }
        self.health.is_empty() || self.health == "healthy"
    }

    pub(super) fn summary(&self) -> String {
        if self.health.is_empty() {
            format!(
                "{} (state={}, status={})",
                self.name, self.state, self.status
            )
        } else {
            format!(
                "{} (state={}, health={}, status={})",
                self.name, self.state, self.health, self.status
            )
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ChildSpec {
    pub(super) name: String,
    pub(super) program: OsString,
    pub(super) args: Vec<OsString>,
    pub(super) cwd: Option<PathBuf>,
    pub(super) env: Vec<(OsString, OsString)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PortBinding {
    pub(super) name: &'static str,
    pub(super) host: String,
    pub(super) port: u16,
}

impl PortBinding {
    pub(super) fn new(name: &'static str, host: &str, port: u16) -> Self {
        Self {
            name,
            host: host.to_string(),
            port,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PortOwner {
    pub(super) pid: u32,
    pub(super) command: String,
}

impl PortOwner {
    pub(super) fn summary(&self) -> String {
        format!("pid={} cmd={}", self.pid, self.command)
    }
}

impl ChildSpec {
    pub(super) fn axon<const N: usize, I>(name: &str, exe: &Path, args: [&str; N], env: I) -> Self
    where
        I: IntoIterator<Item = (&'static str, &'static str)>,
    {
        Self {
            name: name.to_string(),
            program: exe.as_os_str().to_os_string(),
            args: args.into_iter().map(OsString::from).collect(),
            cwd: None,
            env: env
                .into_iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value)))
                .collect(),
        }
    }

    pub(super) fn external<const N: usize, I>(
        name: &str,
        program: &str,
        args: [&str; N],
        env: I,
        cwd: &Path,
    ) -> Self
    where
        I: IntoIterator<Item = (&'static str, String)>,
    {
        Self {
            name: name.to_string(),
            program: OsString::from(program),
            args: args.into_iter().map(OsString::from).collect(),
            cwd: Some(cwd.to_path_buf()),
            env: env
                .into_iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value)))
                .collect(),
        }
    }
}
