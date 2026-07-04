use super::{LocalSetupPhase, LocalSetupStatus, PhaseTimer};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::time::Duration;
use tokio::process::Command;

pub(super) async fn check_command<const N: usize>(
    name: &'static str,
    args: [&str; N],
) -> LocalSetupPhase {
    let timer = PhaseTimer::start(name);
    // "docker compose" is a phase label; the actual binary to invoke is `docker`.
    let binary = if name == "docker compose" {
        "docker"
    } else {
        name
    };
    let result =
        crate::setup::diagnostics::check_command(binary, args, Duration::from_secs(10)).await;

    let status = match result.status {
        crate::setup::diagnostics::CommandStatus::Ok => LocalSetupStatus::Ok,
        crate::setup::diagnostics::CommandStatus::Failed
        | crate::setup::diagnostics::CommandStatus::NotFound
        | crate::setup::diagnostics::CommandStatus::TimedOut => LocalSetupStatus::Error,
    };
    timer.finish(status, result.detail)
}

pub(super) async fn check_gemini_cli() -> LocalSetupPhase {
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

pub(super) fn check_oauth_config(env_values: Option<&BTreeMap<String, String>>) -> LocalSetupPhase {
    let timer = PhaseTimer::start("oauth");
    match setup_env_value(env_values, "AXON_AUTH_MODE") {
        Some(value) if value.trim().eq_ignore_ascii_case("oauth") => {
            let missing: Vec<&str> = [
                "AXON_PUBLIC_URL",
                "AXON_GOOGLE_CLIENT_ID",
                "AXON_GOOGLE_CLIENT_SECRET",
                "AXON_AUTH_ADMIN_EMAIL",
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
