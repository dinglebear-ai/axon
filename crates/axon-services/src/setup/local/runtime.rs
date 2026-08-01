use super::{LocalSetupPhase, LocalSetupStatus, PhaseTimer, SETUP_HARD_MAX_SECS};
use std::io;
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::process::Command;

/// Single-shot health probe — no retry loop. Unlike `wait_http` (which polls for
/// up to 60s while a freshly-started container boots), this answers the narrow
/// question "is the endpoint serving right now?" so an already-deployed host can
/// short-circuit the plugin hook before any preflight/compose work runs.
pub(super) async fn probe_http_once(url: &str, timeout: Duration) -> bool {
    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(client) => client,
        Err(_) => return false,
    };
    client
        .get(url)
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false)
}

pub(super) async fn run_compose<const N: usize>(
    compose_dir: &Path,
    env_path: &Path,
    args: [&str; N],
) -> LocalSetupPhase {
    let timer = PhaseTimer::start(match args.first().copied() {
        Some("pull") => "compose-pull",
        Some("down") => "compose-down",
        Some("restart") => "compose-restart",
        Some("build") => "compose-build",
        _ => "compose-up",
    });
    if args.first().copied() == Some("up")
        && let Err(error) = ensure_compose_network(env_path).await
    {
        return timer.finish(LocalSetupStatus::Error, error.to_string());
    }
    let mut cmd = Command::new("docker");
    cmd.arg("compose")
        .arg("--env-file")
        .arg(env_path)
        .arg("-f")
        .arg(compose_dir.join("docker-compose.yaml"));
    if let Err(error) = add_external_qdrant_overlay(&mut cmd, compose_dir, env_path) {
        return timer.finish(LocalSetupStatus::Error, error.to_string());
    }
    if let Err(error) = add_external_providers_overlay(&mut cmd, compose_dir, env_path) {
        return timer.finish(LocalSetupStatus::Error, error.to_string());
    }
    cmd.args(args).current_dir(compose_dir);
    run_timed_command(timer, cmd, Duration::from_secs(SETUP_HARD_MAX_SECS)).await
}

async fn ensure_compose_network(env_path: &Path) -> io::Result<()> {
    let network = compose_network_name(env_path)?;
    let inspected = Command::new("docker")
        .args(["network", "inspect", &network])
        .output()
        .await?;
    if inspected.status.success() {
        return Ok(());
    }

    let created = Command::new("docker")
        .args(["network", "create", &network])
        .output()
        .await?;
    if created.status.success() {
        return Ok(());
    }

    // Treat a concurrent creator as success.
    let reinspected = Command::new("docker")
        .args(["network", "inspect", &network])
        .output()
        .await?;
    if reinspected.status.success() {
        return Ok(());
    }
    Err(io::Error::other(command_failure_detail(&created)))
}

pub(super) fn compose_network_name(env_path: &Path) -> io::Result<String> {
    let values = super::env::read_env_values(env_path)?;
    Ok(values
        .get("DOCKER_NETWORK")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("axon")
        .to_string())
}

pub(super) async fn follow_logs(compose_dir: &Path, env_path: &Path) -> LocalSetupPhase {
    let timer = PhaseTimer::start("compose-logs");
    let mut cmd = Command::new("docker");
    cmd.arg("compose")
        .arg("--env-file")
        .arg(env_path)
        .arg("-f")
        .arg(compose_dir.join("docker-compose.yaml"));
    if let Err(error) = add_external_qdrant_overlay(&mut cmd, compose_dir, env_path) {
        return timer.finish(LocalSetupStatus::Error, error.to_string());
    }
    if let Err(error) = add_external_providers_overlay(&mut cmd, compose_dir, env_path) {
        return timer.finish(LocalSetupStatus::Error, error.to_string());
    }
    cmd.args(["logs", "-f"]).current_dir(compose_dir);
    match cmd.status().await {
        Ok(status) if status.success() => timer.finish(LocalSetupStatus::Ok, "log stream ended"),
        Ok(status) => timer.finish(
            LocalSetupStatus::Error,
            format!("docker compose logs exited with {status}"),
        ),
        Err(err) => timer.finish(LocalSetupStatus::Error, err.to_string()),
    }
}

fn add_external_qdrant_overlay(
    cmd: &mut Command,
    compose_dir: &Path,
    env_path: &Path,
) -> io::Result<()> {
    if let Some(url) = external_qdrant_url(env_path)? {
        cmd.arg("-f")
            .arg(compose_dir.join("docker-compose.external-qdrant.yaml"))
            .env("AXON_EXTERNAL_QDRANT_URL", url);
    }
    Ok(())
}

fn add_external_providers_overlay(
    cmd: &mut Command,
    compose_dir: &Path,
    env_path: &Path,
) -> io::Result<()> {
    if let Some((tei_url, chrome_url)) = external_provider_urls(env_path)? {
        cmd.arg("-f")
            .arg(compose_dir.join("docker-compose.external-providers.yaml"))
            .env("AXON_EXTERNAL_TEI_URL", tei_url)
            .env("AXON_EXTERNAL_CHROME_REMOTE_URL", chrome_url);
    }
    Ok(())
}

pub(super) fn external_provider_urls(env_path: &Path) -> io::Result<Option<(String, String)>> {
    let values = super::env::read_env_values(env_path)?;
    let tei = values
        .get("AXON_EXTERNAL_TEI_URL")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    let chrome = values
        .get("AXON_EXTERNAL_CHROME_REMOTE_URL")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty());
    match (tei, chrome) {
        (None, None) => Ok(None),
        (Some(tei), Some(chrome)) => Ok(Some((tei.to_string(), chrome.to_string()))),
        _ => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "AXON_EXTERNAL_TEI_URL and AXON_EXTERNAL_CHROME_REMOTE_URL must be set together",
        )),
    }
}

pub(super) fn external_qdrant_url(env_path: &Path) -> io::Result<Option<String>> {
    let values = super::env::read_env_values(env_path)?;
    Ok(values
        .get("QDRANT_URL")
        .map(|value| value.trim())
        .filter(|value| {
            !value.is_empty()
                && !value.contains("://axon-qdrant:")
                && !value.contains("://127.0.0.1:")
                && !value.contains("://localhost:")
                && !value.contains("://[::1]:")
        })
        .map(str::to_string))
}

pub(super) async fn wait_http(name: &'static str, url: impl Into<String>) -> LocalSetupPhase {
    let timer = PhaseTimer::start(name);
    let url = url.into();
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(err) => return timer.finish(LocalSetupStatus::Error, err.to_string()),
    };
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match client.get(&url).send().await {
            Ok(response) if response.status().is_success() => {
                return timer.finish(LocalSetupStatus::Ok, format!("{url} ready"));
            }
            Ok(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Ok(response) => {
                return timer.finish(
                    LocalSetupStatus::Error,
                    format!("{url} returned {}", response.status()),
                );
            }
            Err(_) if Instant::now() < deadline => {
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(err) => {
                return timer.finish(
                    LocalSetupStatus::Error,
                    format!("timed out waiting for {url}: {err}"),
                );
            }
        }
    }
}

pub(super) async fn prewarm_tei(tei_url: &str) -> LocalSetupPhase {
    let timer = PhaseTimer::start("tei-prewarm");
    let client = reqwest::Client::new();
    let embed_url = format!("{}/embed", tei_url.trim_end_matches('/'));
    match tokio::time::timeout(
        Duration::from_secs(120),
        client
            .post(embed_url)
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

pub(super) async fn run_smoke<const N: usize>(
    name: &'static str,
    args: [&str; N],
) -> LocalSetupPhase {
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
