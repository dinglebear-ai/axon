use super::model::{
    ANSI_BOLD, ANSI_DIM, ANSI_RED, ANSI_RESET, ANSI_YELLOW, ChildSpec, MAX_UNSTABLE_RESTARTS,
    RESTART_BACKOFF_INITIAL_SECS, RESTART_BACKOFF_MAX_SECS, RESTART_STABLE_WINDOW_SECS,
    SERVE_CHILD_ROLE_BRIDGE, SERVE_CHILD_ROLE_ENV, SHUTDOWN_GRACE_SECS, child_color,
};
use super::preflight::{preflight_dependencies, supervised_child_specs};
use crate::crates::core::config::Config;
use std::error::Error;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(super) fn is_internal_bridge_runtime() -> bool {
    matches!(
        std::env::var(SERVE_CHILD_ROLE_ENV).ok().as_deref(),
        Some(SERVE_CHILD_ROLE_BRIDGE)
    )
}

pub(super) async fn run_supervisor(cfg: &Config) -> Result<(), Box<dyn Error>> {
    preflight_dependencies(cfg).await?;

    let shutdown = CancellationToken::new();
    let child_specs = supervised_child_specs(cfg)?;
    let (fatal_tx, mut fatal_rx) = mpsc::unbounded_channel::<String>();
    let mut tasks = Vec::with_capacity(child_specs.len());

    for spec in child_specs {
        let token = shutdown.clone();
        let tx = fatal_tx.clone();
        tasks.push(tokio::spawn(async move {
            supervise_child(spec, token, tx).await;
        }));
    }
    drop(fatal_tx);

    let fatal_message = tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log_supervisor("serve", ANSI_YELLOW, "shutdown requested");
            None
        }
        message = fatal_rx.recv() => message,
    };
    shutdown.cancel();
    wait_for_tasks(tasks).await;
    if let Some(message) = fatal_message {
        return Err(message.into());
    }
    Ok(())
}

async fn wait_for_tasks(tasks: Vec<JoinHandle<()>>) {
    for task in tasks {
        if let Err(err) = task.await {
            log_supervisor(
                "serve",
                ANSI_RED,
                &format!("supervisor task join error: {err}"),
            );
        }
    }
}

async fn supervise_child(
    spec: ChildSpec,
    shutdown: CancellationToken,
    fatal_tx: mpsc::UnboundedSender<String>,
) {
    let mut backoff_secs = RESTART_BACKOFF_INITIAL_SECS;
    let mut unstable_restarts = 0usize;

    loop {
        if shutdown.is_cancelled() {
            return;
        }

        let start = Instant::now();
        match spawn_child(&spec) {
            Ok(mut child) => {
                let stdout_task = spawn_log_task(spec.name.clone(), "stdout", child.stdout.take());
                let stderr_task = spawn_log_task(spec.name.clone(), "stderr", child.stderr.take());

                let exit_status = tokio::select! {
                    _ = shutdown.cancelled() => {
                        terminate_child(&spec.name, &mut child).await;
                        None
                    }
                    status = child.wait() => status.ok(),
                };

                await_log_task(stdout_task).await;
                await_log_task(stderr_task).await;

                if shutdown.is_cancelled() {
                    return;
                }

                let uptime = start.elapsed();
                let code = exit_status
                    .map(|status| status.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                log_child_event(
                    &spec.name,
                    &format!("exited after {:?} with {}", uptime, code),
                );

                let delay = if uptime.as_secs() >= RESTART_STABLE_WINDOW_SECS {
                    backoff_secs = RESTART_BACKOFF_INITIAL_SECS;
                    unstable_restarts = 0;
                    RESTART_BACKOFF_INITIAL_SECS
                } else {
                    let delay = next_restart_delay(backoff_secs);
                    backoff_secs = (backoff_secs * 2).min(RESTART_BACKOFF_MAX_SECS);
                    unstable_restarts += 1;
                    delay
                };

                if reached_unstable_restart_limit(unstable_restarts) {
                    let message = format!(
                        "child `{}` failed {} times before reaching the {}s stability window",
                        spec.name, unstable_restarts, RESTART_STABLE_WINDOW_SECS
                    );
                    log_supervisor("serve", ANSI_RED, &message);
                    let _ = fatal_tx.send(message);
                    return;
                }

                log_child_event(&spec.name, &format!("restarting in {}s", delay));
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
                }
            }
            Err(err) => {
                log_child_event(&spec.name, &format!("spawn failed: {err}"));
                let delay = next_restart_delay(backoff_secs);
                backoff_secs = (backoff_secs * 2).min(RESTART_BACKOFF_MAX_SECS);
                unstable_restarts += 1;
                if reached_unstable_restart_limit(unstable_restarts) {
                    let message = format!(
                        "child `{}` failed to start {} times before reaching the {}s stability window",
                        spec.name, unstable_restarts, RESTART_STABLE_WINDOW_SECS
                    );
                    log_supervisor("serve", ANSI_RED, &message);
                    let _ = fatal_tx.send(message);
                    return;
                }
                log_child_event(&spec.name, &format!("restarting in {}s", delay));
                tokio::select! {
                    _ = shutdown.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(delay)) => {}
                }
            }
        }
    }
}

pub(super) fn spawn_child(spec: &ChildSpec) -> Result<Child, String> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    if let Some(cwd) = &spec.cwd {
        command.current_dir(cwd);
    }
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    command.kill_on_drop(true);
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    let child = command.spawn().map_err(|err| {
        format!(
            "failed to spawn `{}` from `{}`: {err}",
            spec.name,
            spec.program.to_string_lossy()
        )
    })?;
    log_child_event(&spec.name, "started");
    Ok(child)
}

fn spawn_log_task(
    name: String,
    stream_name: &'static str,
    stream: Option<impl AsyncRead + Unpin + Send + 'static>,
) -> Option<JoinHandle<()>> {
    let stream = stream?;
    Some(tokio::spawn(async move {
        let mut lines = BufReader::new(stream).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        log_stream_line(&name, stream_name, trimmed);
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    tracing::warn!(
                        name = %name,
                        stream = %stream_name,
                        error = %e,
                        "log stream I/O error, stopping log forwarding"
                    );
                    break;
                }
            }
        }
    }))
}

async fn await_log_task(task: Option<JoinHandle<()>>) {
    if let Some(task) = task
        && let Err(err) = task.await
    {
        log_supervisor("serve", ANSI_RED, &format!("log task join error: {err}"));
    }
}

async fn terminate_child(name: &str, child: &mut Child) {
    if let Some(id) = child.id() {
        log_child_event(name, &format!("stopping pid={id}"));
    }
    let _ = child.start_kill();
    let _ = tokio::time::timeout(Duration::from_secs(SHUTDOWN_GRACE_SECS), child.wait()).await;
}

pub(super) fn log_supervisor(label: &str, color: &str, message: &str) {
    eprintln!(
        "{}{}[{}]{} {}",
        ANSI_BOLD, color, label, ANSI_RESET, message
    );
}

pub(super) fn log_child_event(name: &str, message: &str) {
    let color = child_color(name);
    eprintln!("{}{}[{}]{} {}", ANSI_BOLD, color, name, ANSI_RESET, message);
}

pub(super) fn log_stream_line(name: &str, stream_name: &str, message: &str) {
    let color = child_color(name);
    eprintln!(
        "{}{}[{}{}:{}{}]{} {}{}{}",
        ANSI_BOLD,
        color,
        name,
        ANSI_DIM,
        stream_name,
        color,
        ANSI_RESET,
        ANSI_DIM,
        message,
        ANSI_RESET
    );
}

pub(super) fn next_restart_delay(current_backoff_secs: u64) -> u64 {
    current_backoff_secs.clamp(RESTART_BACKOFF_INITIAL_SECS, RESTART_BACKOFF_MAX_SECS)
}

pub(super) fn reached_unstable_restart_limit(restarts: usize) -> bool {
    restarts >= MAX_UNSTABLE_RESTARTS
}
