use std::time::{Duration, Instant};

#[cfg(test)]
#[path = "ui_commands_tests.rs"]
mod tests;

use gpui::{Context, Window, prelude::*};

use super::{ConnectionState, Palette, RunningCommand};
use crate::Submit;
use crate::actions::ArgMode;
use crate::output::{CommandOutput, OutputKind};
use crate::rest_client::{
    RestClient, RestOutput, RestRequest, build_rest_request, display_rest_request,
};

struct CommandResult {
    id: u64,
    subcommand: &'static str,
    command_line: String,
    result: Result<RestOutput, String>,
    image_bytes: Option<Vec<u8>>,
}

struct HealthResult {
    ok: bool,
}

/// Maximum poll attempts for async job commands (~5 minutes at 2s intervals).
const JOB_POLL_MAX_ATTEMPTS: u32 = 150;
const JOB_POLL_INTERVAL: Duration = Duration::from_millis(2_000);

/// Returns true for subcommands that start async jobs and should be polled
/// to a terminal state before showing results.
pub(crate) fn is_async_job_command(subcommand: &str) -> bool {
    matches!(subcommand, "crawl" | "embed" | "extract" | "ingest")
}

/// Terminal job statuses — stop polling when any of these appear.
pub(crate) fn is_terminal_job_status(status: &str) -> bool {
    matches!(status, "completed" | "failed" | "canceled" | "cancelled")
}

/// Extract the `status_url` polling path from an accepted-job JSON response.
fn accepted_job_poll_path(json_text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json_text).ok()?;
    value.get("status_url")?.as_str().map(ToString::to_string)
}

/// If `output` is a 202 accepted-job response, poll the status path until a
/// terminal state is reached or the max-attempt cap is hit. Falls back to the
/// original accepted response on timeout so the job ID remains visible.
fn poll_accepted_job(
    client: &RestClient,
    output: Result<RestOutput, String>,
) -> Result<RestOutput, String> {
    let Ok(ref accepted) = output else {
        return output;
    };
    if !accepted.ok || accepted.status != 202 {
        return output;
    }
    let Some(poll_path) = accepted.stdout.as_deref().and_then(accepted_job_poll_path) else {
        return output;
    };
    let poll_request = RestRequest {
        method: "GET",
        path: poll_path.clone(),
        body: None,
        label: format!("GET {poll_path}"),
    };
    for attempt in 0..JOB_POLL_MAX_ATTEMPTS {
        if attempt > 0 {
            std::thread::sleep(JOB_POLL_INTERVAL);
        }
        let Ok(status_output) = client.execute(&poll_request) else {
            continue;
        };
        if !status_output.ok {
            continue;
        }
        let reached_terminal = status_output
            .stdout
            .as_deref()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
            .and_then(|v| v.get("job").cloned())
            .and_then(|j| {
                j.get("status")
                    .and_then(|s| s.as_str())
                    .map(is_terminal_job_status)
            });
        if reached_terminal == Some(true) {
            return Ok(status_output);
        }
    }
    output // timed out — show accepted response so job ID is still visible
}

/// Extract `/v1/artifacts/<relative_path>` from a screenshot JSON response.
fn screenshot_artifact_path(json_text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json_text).ok()?;
    let rel = value
        .get("artifact_handle")?
        .get("relative_path")?
        .as_str()?;
    Some(format!("/v1/artifacts/{rel}"))
}

/// Run a REST command, poll async jobs to terminal state, and fetch screenshot
/// artifacts. Extracted from `submit` so the background spawn closure stays concise.
fn run_rest_command(
    subcommand: &'static str,
    request: &RestRequest,
    run_id: u64,
    command_line: String,
) -> CommandResult {
    let (result, image_bytes) = match RestClient::from_env() {
        Err(e) => (Err(e), None),
        Ok(client) => {
            let output = client.execute(request);
            // For async job commands, poll until a terminal state is reached
            // so users see results/metrics instead of just a job ID.
            let output = if is_async_job_command(subcommand) {
                poll_accepted_job(&client, output)
            } else {
                output
            };
            // For screenshot, fetch the artifact PNG after a successful response
            let image_bytes = if subcommand == "screenshot" {
                output
                    .as_ref()
                    .ok()
                    .filter(|o| o.ok)
                    .and_then(|o| o.stdout.as_deref())
                    .and_then(screenshot_artifact_path)
                    .and_then(|path| client.fetch_bytes(&path))
            } else {
                None
            };
            (output, image_bytes)
        }
    };
    CommandResult {
        id: run_id,
        subcommand,
        command_line,
        result,
        image_bytes,
    }
}

impl Palette {
    pub(super) fn spawn_health_check(&mut self, cx: &mut Context<Self>) {
        self.health_check_id = self.health_check_id.wrapping_add(1);
        let my_id = self.health_check_id;
        self.connection = ConnectionState::Checking;
        cx.notify();

        let task = cx.background_spawn(async move {
            // Use /healthz — unauthenticated, lightweight, no Qdrant/TEI probes.
            // Cheaper and more reliable than /v1/doctor for a connection dot.
            let ok = RestClient::from_env()
                .and_then(|client| client.health_check())
                .unwrap_or(false);
            HealthResult { ok }
        });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.health_check_id != my_id {
                    return;
                }
                this.connection = if result.ok {
                    ConnectionState::Connected
                } else {
                    ConnectionState::Disconnected
                };
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        if self.action_menu_open {
            let actions = self.matches();
            if let Some(action) = actions.get(self.selected).copied() {
                self.select_action_mode(action, false, cx);
            }
            return;
        }

        let (action, arg) = if let Some(locked) = self.locked_command {
            (locked, self.query.trim().to_string())
        } else {
            let actions = self.matches();
            let Some(action) = actions.get(self.selected).copied() else {
                return;
            };
            (action, self.argument_for(action).to_string())
        };

        if action.subcommand == "ask-reset" {
            if self.running.is_some() {
                self.command_output = Some(CommandOutput::notice(
                    OutputKind::Warning,
                    "Command already running",
                    "Wait for the current axon command to finish.",
                ));
                cx.notify();
                return;
            }
            self.handle_reset_conversation(cx);
            return;
        }

        if action.subcommand == "settings" {
            self.open_settings(cx);
            return;
        }

        if matches!(action.arg_mode, ArgMode::Single | ArgMode::Split) && arg.is_empty() {
            self.command_output = Some(CommandOutput::notice(
                OutputKind::Warning,
                "Argument required",
                action.example,
            ));
            cx.notify();
            return;
        }

        if self.running.is_some() {
            self.command_output = Some(CommandOutput::notice(
                OutputKind::Warning,
                "Command already running",
                "Wait for the current axon command to finish.",
            ));
            cx.notify();
            return;
        }

        if matches!(self.connection, ConnectionState::Disconnected) {
            self.command_output = Some(CommandOutput::notice(
                OutputKind::Warning,
                "Axon server not reachable",
                "Start `axon serve` to enable commands, then click the status dot to reconnect.",
            ));
            cx.notify();
            return;
        }

        let request = match build_rest_request(action, &arg) {
            Ok(request) => request,
            Err(error) => {
                self.command_output = Some(CommandOutput::notice(
                    OutputKind::Error,
                    "Invalid input",
                    error,
                ));
                cx.notify();
                return;
            }
        };
        let command_line = display_rest_request(&request);
        let run_id = self.next_run_id;
        self.next_run_id += 1;
        self.running = Some(RunningCommand {
            id: run_id,
            subcommand: action.subcommand,
            label: action.label,
            started_at: Instant::now(),
        });
        self.command_output = Some(CommandOutput::running(&command_line, action));
        self.errors_open = false;
        self.spawn_running_tick(run_id, cx);

        let task = cx.background_spawn(async move {
            run_rest_command(action.subcommand, &request, run_id, command_line)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                if this
                    .running
                    .as_ref()
                    .map(|running| running.id)
                    .is_some_and(|running_id| running_id == result.id)
                {
                    this.running = None;
                }

                let output = this.finalize_result(result);
                this.errors_open = output.stdout.is_none() && output.stderr.is_some();
                this.command_output = Some(output);
                cx.notify();
            });
        })
        .detach();

        self.query.clear();
        self.selected = 0;
        cx.notify();
    }

    fn spawn_running_tick(&self, run_id: u64, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            loop {
                executor.timer(Duration::from_millis(250)).await;
                let still_running = this
                    .update(cx, |this, cx| {
                        let active = this
                            .running
                            .as_ref()
                            .is_some_and(|running| running.id == run_id);
                        if active {
                            cx.notify();
                        }
                        active
                    })
                    .unwrap_or(false);
                if !still_running {
                    break;
                }
            }
        })
        .detach();
    }

    fn handle_reset_conversation(&mut self, cx: &mut Context<Self>) {
        self.command_output = Some(CommandOutput::notice(
            OutputKind::Success,
            "Ask state cleared",
            "REST ask requests are stateless in the current server API.",
        ));
        self.query.clear();
        self.selected = 0;
        cx.notify();
    }

    fn finalize_result(&mut self, result: CommandResult) -> CommandOutput {
        match result.result {
            Ok(output) => CommandOutput::from_rest(
                &result.command_line,
                result.subcommand,
                output,
                result.image_bytes,
            ),
            Err(error) => CommandOutput::spawn_error(&result.command_line, error),
        }
    }
}
