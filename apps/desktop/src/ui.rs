use std::process::{Command, Output};

// Spawn `axon` without flashing a cmd.exe window on Windows (CREATE_NO_WINDOW).
fn axon_command() -> Command {
    let cmd = Command::new("axon");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = cmd;
        cmd.creation_flags(0x08000000);
        return cmd;
    }
    #[cfg(not(windows))]
    cmd
}

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    Render, ScrollHandle, SharedString, Styled, Window, div, prelude::*, px, rgb,
};

use crate::actions::{
    ACTIONS, ArgMode, CommandAction, action_invoked_by, action_matches, build_axon_args,
    display_command_line, looks_like_url,
};
use crate::output::{CommandOutput, OutputKind};
use crate::render::{
    reflow_palette_window, render_action_list, render_palette_footer, render_prompt_row,
};
use crate::render_output::render_output_body;
use crate::theme::{
    AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_FONT_SANS,
    AURORA_NAV_BG, AURORA_PAGE_BG, AURORA_PANEL_MEDIUM, AURORA_PANEL_STRONG, AURORA_TEXT_PRIMARY,
};
use crate::{ClearOutput, MoveDown, MoveUp, Submit, TabComplete};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    Unknown,
    Checking,
    Connected,
    Disconnected,
}

impl ConnectionState {
    pub(crate) fn dot_color(self) -> u32 {
        match self {
            Self::Unknown | Self::Checking => AURORA_BORDER_STRONG,
            Self::Connected => 0x4ade80,
            Self::Disconnected => 0xf87171,
        }
    }
}

pub(crate) struct Palette {
    query: String,
    selected: usize,
    focus: FocusHandle,
    command_output: Option<CommandOutput>,
    running: Option<RunningCommand>,
    next_run_id: u64,
    output_scroll: ScrollHandle,
    locked_command: Option<CommandAction>,
    connection: ConnectionState,
    /// Monotonic id for in-flight health checks. Each spawn increments this;
    /// completions only apply when their captured id still matches the latest,
    /// so a slower older probe can't overwrite a newer result.
    health_check_id: u64,
    // Last height we asked window.resize() for; reflow uses a deadband.
    last_window_height: f32,
}

struct RunningCommand {
    id: u64,
    subcommand: &'static str,
}

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
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let mut palette = Self {
            query: String::new(),
            selected: 0,
            focus: cx.focus_handle(),
            command_output: None,
            running: None,
            next_run_id: 1,
            output_scroll: ScrollHandle::new(),
            locked_command: None,
            connection: ConnectionState::Unknown,
            health_check_id: 0,
            last_window_height: 108.0, // matches main.rs initial window size
        };
        palette.spawn_health_check(cx);
        palette
    }

    fn spawn_health_check(&mut self, cx: &mut Context<Self>) {
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
                    // Check for "all_ok":true with or without a space after the colon.
                    stdout.contains(r#""all_ok":true"#) || stdout.contains(r#""all_ok": true"#)
                })
                .unwrap_or(false);
            HealthResult { ok }
        });

        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                // Ignore stale completions — a newer probe has been spawned and
                // its result is authoritative.
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

    fn matches(&self) -> Vec<CommandAction> {
        if self.locked_command.is_some() {
            return vec![];
        }
        let input = self.query.trim();
        if input.is_empty() {
            return vec![];
        }
        let head = input.split_whitespace().next().unwrap_or("");
        let direct_url = looks_like_url(input);

        ACTIONS
            .iter()
            .copied()
            .filter(|action| {
                action_matches(*action, head)
                    || action_matches(*action, input)
                    || (direct_url && action.accepts_direct_url())
            })
            .collect()
    }

    fn clear_output(&mut self, _: &ClearOutput, _window: &mut Window, cx: &mut Context<Self>) {
        self.command_output = None;
        cx.notify();
    }

    fn tab_complete(&mut self, _: &TabComplete, _window: &mut Window, cx: &mut Context<Self>) {
        if self.locked_command.is_some() {
            return;
        }
        let actions = self.matches();
        if let Some(action) = actions.get(self.selected).copied() {
            self.locked_command = Some(action);
            // Strip the command token from the query if the user typed it exactly
            // OR if the head matches a fuzzy prefix the palette used to surface
            // this action — otherwise a fragment like "scra" would stay in
            // self.query and leak into argv when the locked command is submitted.
            let input = self.query.trim();
            let mut parts = input.splitn(2, char::is_whitespace);
            let head = parts.next().unwrap_or("");
            let tail = parts.next().map(str::trim).unwrap_or("");
            if action_invoked_by(action, head) || action_matches(action, head) {
                self.query = tail.to_string();
            }
            self.selected = 0;
            cx.notify();
        }
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        let (action, arg) = if let Some(locked) = self.locked_command {
            (locked, self.query.trim().to_string())
        } else {
            let actions = self.matches();
            let Some(action) = actions.get(self.selected).copied() else {
                return;
            };
            (action, self.argument_for(action).to_string())
        };

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

        let args = match build_axon_args(action, &arg) {
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
        let command_line = display_command_line(&args);
        let run_id = self.next_run_id;
        self.next_run_id += 1;
        self.running = Some(RunningCommand {
            id: run_id,
            subcommand: action.subcommand,
        });
        self.command_output = Some(CommandOutput::running(&command_line, action));

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

                this.command_output = Some(match result.result {
                    Ok(output) => {
                        CommandOutput::from_process(&result.command_line, result.subcommand, output)
                    }
                    Err(error) => CommandOutput::spawn_error(&result.command_line, error),
                });
                cx.notify();
            });
        })
        .detach();

        self.locked_command = None;
        self.query.clear();
        self.selected = 0;
        cx.notify();
    }

    fn build_status_dot(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        div()
            .id("status-dot")
            .size(px(8.0))
            .rounded_full()
            .flex_shrink_0()
            .cursor_pointer()
            .bg(rgb(self.connection.dot_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _window, cx| {
                    this.spawn_health_check(cx);
                }),
            )
    }

    fn move_down(&mut self, _: &MoveDown, _w: &mut Window, cx: &mut Context<Self>) {
        let n = self.matches().len();
        if n > 0 {
            self.selected = (self.selected + 1) % n;
            cx.notify();
        }
    }

    fn move_up(&mut self, _: &MoveUp, _w: &mut Window, cx: &mut Context<Self>) {
        let n = self.matches().len();
        if n > 0 {
            self.selected = if self.selected == 0 {
                n - 1
            } else {
                self.selected - 1
            };
            cx.notify();
        }
    }

    fn on_key(&mut self, ev: &gpui::KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let key = ev.keystroke.key.as_str();
        match key {
            "backspace" => {
                if self.query.is_empty() && self.locked_command.is_some() {
                    self.locked_command = None;
                } else {
                    self.query.pop();
                }
            }
            "escape" => {
                if self.locked_command.is_some() {
                    // Unlock but preserve typed argument so user can reselect a command.
                    self.locked_command = None;
                } else if self.command_output.is_some() {
                    self.command_output = None;
                } else if !self.query.is_empty() {
                    self.query.clear();
                } else {
                    cx.hide();
                }
            }
            _ => {
                let m = &ev.keystroke.modifiers;
                if !m.control && !m.alt && !m.platform && !m.function {
                    if let Some(ch) = ev.keystroke.key_char.as_deref() {
                        self.query.push_str(ch);
                    }
                }
            }
        }
        self.selected = 0;
        cx.notify();
    }

    fn argument_for(&self, action: CommandAction) -> &str {
        if action.arg_mode == ArgMode::None {
            return "";
        }

        let input = self.query.trim();
        let mut parts = input.splitn(2, char::is_whitespace);
        let head = parts.next().unwrap_or("");
        let tail = parts.next().map(str::trim).unwrap_or("");

        if action_invoked_by(action, head) {
            tail
        } else {
            input
        }
    }
}

impl Focusable for Palette {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for Palette {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let actions = self.matches();
        let selected = self.selected;
        let selected_action = actions.get(selected).copied();
        let running_subcommand = self.running.as_ref().map(|running| running.subcommand);
        let command_output = self.command_output.clone();
        let locked = self.locked_command;
        let hide_list = self.query.is_empty() || locked.is_some();

        reflow_palette_window(
            &mut self.last_window_height,
            if hide_list { 0 } else { actions.len() },
            selected_action.is_some(),
            command_output.as_ref().is_some_and(|o| o.has_body()),
            window,
        );

        let prompt: SharedString = if self.query.is_empty() {
            if let Some(action) = locked {
                let hint = action
                    .example
                    .splitn(2, ' ')
                    .nth(1)
                    .unwrap_or(action.example);
                SharedString::from(hint.to_string())
            } else {
                SharedString::from("type a command or URL")
            }
        } else {
            SharedString::from(self.query.clone())
        };
        let query_is_empty = self.query.is_empty();

        let status_dot = self.build_status_dot(cx);

        div()
            .key_context("Palette")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::tab_complete))
            .on_action(cx.listener(Self::clear_output))
            .on_key_down(cx.listener(Self::on_key))
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .font_family(AURORA_FONT_SANS)
            .bg(rgb(AURORA_PAGE_BG))
            .text_color(rgb(AURORA_TEXT_PRIMARY))
            .p_5()
            .child(
                div()
                    .w_full()
                    .max_w(px(760.0))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .rounded_lg()
                    // Top-to-bottom gradient gives depth; border accents when typing.
                    .bg(gpui::linear_gradient(
                        180.0,
                        gpui::linear_color_stop(rgb(AURORA_PANEL_STRONG), 0.0),
                        gpui::linear_color_stop(rgb(AURORA_PANEL_MEDIUM), 1.0),
                    ))
                    .border_1()
                    .border_color(rgb(if query_is_empty {
                        AURORA_BORDER_STRONG
                    } else {
                        AURORA_ACCENT_STRONG
                    }))
                    .shadow_2xl()
                    .child(render_prompt_row(
                        query_is_empty,
                        locked,
                        prompt,
                        status_dot,
                    ))
                    .child(render_action_list(
                        actions,
                        selected,
                        running_subcommand,
                        hide_list,
                        |i| cx.listener(move |this, _: &MouseDownEvent, w, cx| {
                            this.selected = i;
                            this.submit(&Submit, w, cx);
                        }),
                        |i| cx.listener(move |this, hovered: &bool, _, cx| {
                            if *hovered && this.selected != i {
                                this.selected = i;
                                cx.notify();
                            }
                        }),
                    ))
                    .when_some(selected_action, |el, action| {
                        el.child(render_palette_footer(
                            action,
                            command_output.as_ref(),
                            self.running.is_some(),
                        ))
                    })
                    .when_some(command_output.clone(), |el, output| {
                        if output.has_body() {
                            el.child(
                                div()
                                    .id("palette-output")
                                    .max_h(px(320.0))
                                    .overflow_scroll()
                                    .scrollbar_width(px(12.0))
                                    .track_scroll(&self.output_scroll)
                                    .block_mouse_except_scroll()
                                    .border_t_1()
                                    .border_color(rgb(AURORA_BORDER_DEFAULT))
                                    .bg(rgb(AURORA_NAV_BG))
                                    .child(render_output_body(output)),
                            )
                        } else {
                            el
                        }
                    }),
            )
    }
}
