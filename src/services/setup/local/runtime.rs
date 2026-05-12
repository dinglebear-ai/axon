use super::{LocalSetupPhase, LocalSetupStatus, PhaseTimer, SETUP_HARD_MAX_SECS};
use std::path::Path;
use std::time::{Duration, Instant};
use tokio::process::Command;

pub(super) async fn run_compose<const N: usize>(
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
