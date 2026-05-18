use std::process::Output;
use std::time::{Duration, Instant};

use gpui::{Context, Window, prelude::*};

use super::{ConnectionState, Palette, RunningCommand, axon_command};
use crate::Submit;
use crate::actions::{ArgMode, build_axon_args, display_command_line};
use crate::conversation::{AskConversation, inject_follow_up};
use crate::output::{CommandOutput, OutputKind};

struct CommandResult {
    id: u64,
    subcommand: &'static str,
    command_line: String,
    result: Result<Output, String>,
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
            let ok = axon_command()
                .args(["doctor", "--json"])
                .output()
                .map(|o| {
                    let stdout = String::from_utf8_lossy(&o.stdout);
                    stdout.contains(r#""all_ok":true"#) || stdout.contains(r#""all_ok": true"#)
                })
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

        if self
            .ask_conversation
            .is_some_and(|c| c.is_stale(Instant::now()))
        {
            self.ask_conversation = None;
        }

        if action.arg_mode != ArgMode::None && arg.is_empty() {
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

        let mut args = match build_axon_args(action, &arg) {
            Ok(args) => args,
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
        inject_follow_up(action.subcommand, &mut args, self.ask_conversation.as_ref());
        let command_line = display_command_line(&args);
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
            let mut cmd = axon_command();
            cmd.args(&args);
            let result = cmd.output().map_err(|error| error.to_string());
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
        self.ask_conversation = None;
        self.command_output = Some(CommandOutput::notice(
            OutputKind::Success,
            "Conversation reset",
            "Next ask will start a fresh session.",
        ));
        self.query.clear();
        self.selected = 0;
        cx.notify();
    }

    fn finalize_result(&mut self, result: CommandResult) -> CommandOutput {
        match result.result {
            Ok(output) => {
                if result.subcommand == "ask" && output.status.success() {
                    let now = Instant::now();
                    match self.ask_conversation.as_mut() {
                        Some(c) => c.bump(now),
                        None => self.ask_conversation = Some(AskConversation::new(now)),
                    }
                }
                CommandOutput::from_process(&result.command_line, result.subcommand, output)
            }
            Err(error) => CommandOutput::spawn_error(&result.command_line, error),
        }
    }
}
