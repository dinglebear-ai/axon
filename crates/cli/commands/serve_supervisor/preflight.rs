use super::model::{
    ANSI_YELLOW, ChildSpec, ComposeServiceStatus, DOCKER_SERVICES_FILE, PortBinding, PortOwner,
    SERVE_CHILD_ROLE_BRIDGE, SERVE_CHILD_ROLE_ENV,
};
use super::runtime::log_supervisor;
use crate::crates::core::config::Config;
use std::error::Error;
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

pub(super) async fn preflight_dependencies(cfg: &Config) -> Result<(), Box<dyn Error>> {
    require_command("docker").await?;
    require_command("node").await?;
    require_command("pnpm").await?;
    reconcile_nextjs_dev_lock().await?;
    reconcile_required_ports(cfg).await?;

    let actual = inspect_compose_services().await?;
    let mut missing = Vec::new();
    let mut unhealthy = Vec::new();

    for required in required_container_services(cfg) {
        match actual.iter().find(|entry| entry.service == *required) {
            None => missing.push(required.to_string()),
            Some(entry) if !entry.is_healthy() => unhealthy.push(entry.summary()),
            Some(_) => {}
        }
    }

    if missing.is_empty() && unhealthy.is_empty() {
        return Ok(());
    }

    let mut problems = Vec::new();
    if !missing.is_empty() {
        problems.push(format!("missing: {}", missing.join(", ")));
    }
    if !unhealthy.is_empty() {
        problems.push(format!("unhealthy: {}", unhealthy.join(", ")));
    }

    Err(format!(
        "required infrastructure is not ready ({}) — start it first with `docker compose -f {DOCKER_SERVICES_FILE} up -d`",
        problems.join("; ")
    )
    .into())
}

pub(super) async fn require_command(name: &str) -> Result<(), Box<dyn Error>> {
    let status = Command::new(name)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map_err(|err| format!("required command `{name}` is unavailable: {err}"))?;
    if status.success() {
        return Ok(());
    }
    Err(format!("required command `{name}` is unavailable").into())
}

pub(super) async fn reconcile_nextjs_dev_lock() -> Result<(), Box<dyn Error>> {
    let cwd = std::env::current_dir().map_err(|err| {
        format!("failed to determine working directory for Next.js lock check: {err}")
    })?;
    let web_dir = cwd.join("apps/web");
    if !web_dir.is_dir() {
        return Err(format!(
            "`axon serve` must be invoked from the workspace root (expected {} to exist)",
            web_dir.display()
        )
        .into());
    }
    let lock_path = web_dir.join(".next/dev/lock");
    if !tokio::fs::try_exists(&lock_path).await.unwrap_or(false) {
        return Ok(());
    }

    let next_dev_processes =
        inspect_processes_matching(&["next dev", "pnpm exec next dev"]).await?;
    if next_dev_processes.is_empty() {
        tokio::fs::remove_file(&lock_path).await.map_err(|err| {
            format!(
                "failed to remove stale Next.js dev lock {}: {err}",
                lock_path.display()
            )
        })?;
        log_supervisor(
            "serve",
            ANSI_YELLOW,
            &format!("removed stale Next.js dev lock at {}", lock_path.display()),
        );
        return Ok(());
    }

    Err(format!(
        "Next.js dev lock exists at {} and active Next.js processes were found: {}. Stop the other dev server first.",
        lock_path.display(),
        next_dev_processes
            .iter()
            .map(PortOwner::summary)
            .collect::<Vec<_>>()
            .join(", ")
    )
    .into())
}

pub(super) async fn reconcile_required_ports(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let ports = required_bind_ports(cfg);
    let mut blocked = Vec::new();
    for binding in &ports {
        let owners = inspect_port_owners(binding.port).await?;
        if owners.is_empty() {
            continue;
        }
        let mut unknown = Vec::new();
        for owner in owners {
            if port_owner_matches_binding(&owner.command, binding) {
                terminate_port_owner(&owner).await?;
            } else {
                unknown.push(owner.summary());
            }
        }
        if !unknown.is_empty() {
            blocked.push(format!(
                "{}:{} ({}) owned by {}",
                binding.host,
                binding.port,
                binding.name,
                unknown.join(", ")
            ));
        }
    }

    if !blocked.is_empty() {
        return Err(format!(
            "required local ports are already in use by non-Axon processes: {}",
            blocked.join("; ")
        )
        .into());
    }

    let mut conflicts = Vec::new();
    for binding in ports {
        if let Err(err) = TcpListener::bind((binding.host.as_str(), binding.port))
            && err.kind() != std::io::ErrorKind::AddrInUse
        {
            conflicts.push(format!(
                "{}:{} bind probe failed: {}",
                binding.host, binding.port, err
            ));
        }
    }
    if conflicts.is_empty() {
        return Ok(());
    }
    Err(format!(
        "required local ports are already in use or unavailable: {}",
        conflicts.join(", ")
    )
    .into())
}

pub(super) async fn inspect_compose_services() -> Result<Vec<ComposeServiceStatus>, Box<dyn Error>>
{
    let output = Command::new("docker")
        .args([
            "compose",
            "-f",
            DOCKER_SERVICES_FILE,
            "ps",
            "--format",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("docker compose ps failed: {}", stderr.trim()).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut statuses = Vec::new();
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        statuses.push(serde_json::from_str::<ComposeServiceStatus>(line)?);
    }
    Ok(statuses)
}

pub(super) async fn inspect_port_owners(port: u16) -> Result<Vec<PortOwner>, Box<dyn Error>> {
    if cfg!(target_os = "macos") {
        return inspect_port_owners_lsof(port).await;
    }
    inspect_port_owners_ss(port).await
}

async fn inspect_port_owners_ss(port: u16) -> Result<Vec<PortOwner>, Box<dyn Error>> {
    let output = Command::new("ss")
        .args(["-ltnp", &format!("( sport = :{port} )")])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ss port inspection failed: {}", stderr.trim()).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut owners = Vec::new();
    for line in stdout.lines() {
        for pid in extract_pids_from_ss_line(line) {
            if owners.iter().any(|owner: &PortOwner| owner.pid == pid) {
                continue;
            }
            owners.push(PortOwner {
                pid,
                command: inspect_process_command(pid).await?,
            });
        }
    }
    Ok(owners)
}

async fn inspect_port_owners_lsof(port: u16) -> Result<Vec<PortOwner>, Box<dyn Error>> {
    let output = Command::new("lsof")
        .args(["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("lsof port inspection failed: {}", stderr.trim()).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut owners = Vec::new();
    let mut current_pid = None;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix('p') {
            current_pid = rest.parse::<u32>().ok();
            continue;
        }
        if let Some(rest) = line.strip_prefix('c')
            && let Some(pid) = current_pid
        {
            if owners.iter().any(|owner: &PortOwner| owner.pid == pid) {
                continue;
            }
            owners.push(PortOwner {
                pid,
                command: rest.to_string(),
            });
        }
    }
    Ok(owners)
}

pub(super) async fn inspect_process_command(pid: u32) -> Result<String, Box<dyn Error>> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "args="])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stderr_trim = stderr.trim();
        if stderr_trim.is_empty() {
            // ps exits non-zero with empty stderr when the PID simply doesn't exist.
            return Err(format!("ps pid not found: {pid}").into());
        }
        // ps itself failed (permission denied, unsupported flag, etc.) — propagate.
        return Err(format!("ps inspection failed for pid {pid}: {stderr_trim}").into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(super) async fn inspect_processes_matching(
    patterns: &[&str],
) -> Result<Vec<PortOwner>, Box<dyn Error>> {
    let output = Command::new("ps")
        .args(["-eo", "pid=,args="])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ps process listing failed: {}", stderr.trim()).into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(parse_matching_processes(&stdout, patterns))
}

pub(super) fn parse_matching_processes(raw: &str, patterns: &[&str]) -> Vec<PortOwner> {
    let mut matches = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Some((pid_text, command)) = trimmed.split_once(' ') else {
            continue;
        };
        let Ok(pid) = pid_text.trim().parse::<u32>() else {
            continue;
        };
        let command = command.trim();
        if patterns.iter().any(|pattern| command.contains(pattern)) {
            matches.push(PortOwner {
                pid,
                command: command.to_string(),
            });
        }
    }
    matches
}

pub(super) fn extract_pids_from_ss_line(line: &str) -> Vec<u32> {
    let mut pids = Vec::new();
    let mut search = line;
    while let Some(index) = search.find("pid=") {
        let remainder = &search[index + 4..];
        let digits: String = remainder
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .collect();
        if let Ok(pid) = digits.parse::<u32>() {
            pids.push(pid);
        }
        search = remainder;
    }
    pids
}

pub(super) fn port_owner_matches_binding(command: &str, binding: &PortBinding) -> bool {
    match binding.name {
        "mcp-http" => command.contains("axon mcp") && command.contains("--transport http"),
        "serve-runtime" => command.contains("axon serve"),
        "shell-server" => command.contains("shell-server.mjs"),
        "nextjs" => command.contains("next dev") || command.contains("pnpm dev"),
        _ => false,
    }
}

pub(super) async fn terminate_port_owner(owner: &PortOwner) -> Result<(), Box<dyn Error>> {
    // Re-verify the PID still belongs to the expected command before sending
    // SIGTERM.  Between the initial `ss` inspection and this point the PID
    // could have been recycled by the kernel for an unrelated process (TOCTOU).
    let current_cmd = match inspect_process_command(owner.pid).await {
        Ok(cmd) => cmd,
        Err(e) => {
            let msg = e.to_string();
            if msg.starts_with("ps pid not found:") {
                // `ps -p <pid>` exits non-zero with empty stderr when the PID doesn't exist.
                log_supervisor(
                    "serve",
                    ANSI_YELLOW,
                    &format!("pid {} vanished before SIGTERM — skipping", owner.pid),
                );
                return Ok(());
            }
            // Propagate real ps failures (permission denied, ps not found, etc.)
            return Err(e);
        }
    };

    if current_cmd != owner.command {
        log_supervisor(
            "serve",
            ANSI_YELLOW,
            &format!(
                "pid {} was recycled (expected `{}`, found `{}`) — skipping SIGTERM",
                owner.pid, owner.command, current_cmd
            ),
        );
        return Ok(());
    }

    log_supervisor(
        "serve",
        ANSI_YELLOW,
        &format!(
            "stopping stale process on required port: pid={} cmd={}",
            owner.pid, owner.command
        ),
    );
    let status = Command::new("kill")
        .args(["-TERM", &owner.pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await?;
    if !status.success() {
        return Err(format!("failed to signal pid {}", owner.pid).into());
    }
    tokio::time::sleep(Duration::from_millis(250)).await;
    Ok(())
}

pub(super) fn required_container_services(cfg: &Config) -> &'static [&'static str] {
    if cfg.lite_mode {
        &["axon-qdrant", "axon-tei"]
    } else {
        &[
            "axon-postgres",
            "axon-redis",
            "axon-rabbitmq",
            "axon-qdrant",
            "axon-tei",
            "axon-chrome",
        ]
    }
}

pub(super) fn required_bind_ports(cfg: &Config) -> Vec<PortBinding> {
    vec![
        PortBinding::new("serve-runtime", "0.0.0.0", cfg.serve_port),
        PortBinding::new("mcp-http", cfg.mcp_http_host.as_str(), cfg.mcp_http_port),
        PortBinding::new("nextjs", "127.0.0.1", cfg.web_dev_port),
        PortBinding::new("shell-server", "127.0.0.1", cfg.shell_server_port),
    ]
}

pub(super) fn supervised_child_specs(cfg: &Config) -> Result<Vec<ChildSpec>, Box<dyn Error>> {
    let exe = std::env::current_exe()?;
    let web_dir = PathBuf::from("apps/web");
    let mut specs = vec![
        ChildSpec::axon(
            "serve-runtime",
            &exe,
            ["serve", "--port", &cfg.serve_port.to_string()],
            [
                (SERVE_CHILD_ROLE_ENV, SERVE_CHILD_ROLE_BRIDGE),
                ("AXON_SERVE_HOST", "0.0.0.0"),
            ],
        ),
        ChildSpec::axon(
            "mcp-http",
            &exe,
            ["mcp", "--transport", "http"],
            std::iter::empty::<(&str, &str)>(),
        ),
        ChildSpec::external(
            "shell-server",
            "node",
            ["shell-server.mjs"],
            [("SHELL_SERVER_PORT", cfg.shell_server_port.to_string())],
            &web_dir,
        ),
        ChildSpec::external(
            "nextjs",
            "pnpm",
            [
                "exec",
                "next",
                "dev",
                "--port",
                &cfg.web_dev_port.to_string(),
            ],
            std::iter::empty::<(&str, String)>(),
            &web_dir,
        ),
    ];

    if !cfg.lite_mode {
        for worker in ["crawl", "embed", "extract", "ingest", "refresh"] {
            specs.push(ChildSpec::axon(
                &format!("{worker}-worker"),
                &exe,
                [worker, "worker"],
                std::iter::empty::<(&str, &str)>(),
            ));
        }
        if graph_worker_enabled(cfg) {
            specs.push(ChildSpec::axon(
                "graph-worker",
                &exe,
                ["graph", "worker"],
                std::iter::empty::<(&str, &str)>(),
            ));
        }
    }

    Ok(specs)
}

pub(super) fn graph_worker_enabled(cfg: &Config) -> bool {
    !cfg.neo4j_url.trim().is_empty()
}
