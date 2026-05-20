use std::time::{Duration, Instant};

use gpui::{Context, Window, prelude::*};

use super::{ConnectionState, Palette, RunningCommand};
use crate::Submit;
use crate::actions::ArgMode;
use crate::output::{CommandOutput, OutputKind};
use crate::rest_client::{RestClient, RestOutput, build_rest_request, display_rest_request};

struct CommandResult {
    id: u64,
    subcommand: &'static str,
    command_line: String,
    result: Result<RestOutput, String>,
}

struct HealthResult {
    ok: bool,
}

impl Palette {
    pub(super) fn spawn_health_check(&mut self, cx: &mut Context<Self>) {
        self.health_check_id = self.health_check_id.wrapping_add(1);
        let my_id = self.health_check_id;
        self.connection = ConnectionState::Checking;
        cx.notify();

        let task = cx.background_spawn(async move {
            let ok = RestClient::from_env()
                .and_then(|client| {
                    let request = build_rest_request(
                        crate::actions::CommandAction {
                            label: "Doctor",
                            subcommand: "doctor",
                            arg_mode: ArgMode::None,
                            aliases: &[],
                            description: "",
                            example: "",
                        },
                        "",
                    )?;
                    client.execute(&request)
                })
                .map(|output| output.ok)
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
            let result = RestClient::from_env().and_then(|client| client.execute(&request));
            CommandResult {
                id: run_id,
                subcommand: action.subcommand,
                command_line,
                result,
            }
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
            Ok(output) => CommandOutput::from_rest(&result.command_line, result.subcommand, output),
            Err(error) => CommandOutput::spawn_error(&result.command_line, error),
        }
    }
}
