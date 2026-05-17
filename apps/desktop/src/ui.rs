use std::process::{Command, Output};
use std::time::{Duration, Instant};

/// Spawn `axon` without flashing a console window on Windows. On Unix this
/// is just `Command::new("axon")` — the flag is a no-op outside Windows.
fn axon_command() -> Command {
    let cmd = Command::new("axon");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW = 0x08000000 — prevents the child from getting its
        // own console; otherwise GPUI's non-console parent flashes a cmd.exe
        // window per subprocess (every health check + every command run).
        let mut cmd = cmd;
        cmd.creation_flags(0x08000000);
        return cmd;
    }
    #[cfg(not(windows))]
    cmd
}

use gpui::{App, Context, FocusHandle, Focusable, ScrollHandle, Size, Window, prelude::*, px};

use crate::actions::{
    ACTIONS, ArgMode, CommandAction, action_invoked_by, action_matches, build_axon_args,
    display_command_line, looks_like_url,
};
use crate::layout::{HeightSnapshot, MIN_WINDOW_HEIGHT};
use crate::output::{CommandOutput, OutputKind};
use crate::theme::AURORA_BORDER_STRONG;
use crate::{ClearOutput, MoveDown, MoveUp, Submit, TabComplete};

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;

// `Render for Palette` impl lives in `ui_render.rs`. Sibling file declared
// with `#[path]` so it remains a child module of `ui` and retains access
// to `Palette`'s private fields. See the project monolith policy.
#[path = "ui_render.rs"]
mod ui_render;

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
    /// Last window height we pushed via `Window::resize`. Used to avoid
    /// re-resizing on every render frame when the target is unchanged.
    /// `None` until the first render commits a height. Starts as `None`
    /// so the very first render unconditionally syncs to the computed
    /// target (which, on a cold launch with no input, equals
    /// `MIN_WINDOW_HEIGHT` — already the launch size).
    current_window_height: Option<f32>,
}

pub(crate) struct RunningCommand {
    id: u64,
    pub(crate) subcommand: &'static str,
    pub(crate) label: &'static str,
    pub(crate) started_at: Instant,
}

impl RunningCommand {
    pub(crate) fn elapsed_label(&self) -> String {
        format_elapsed(self.started_at.elapsed())
    }
}

/// Format a `Duration` as a short human-readable label: `"0.4s"`, `"12s"`,
/// `"1m 03s"`. Used by the running indicator so the user can see at a glance
/// that time is passing.
pub(crate) fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 1 {
        // Sub-second: show one decimal so the indicator visibly moves
        // immediately after submit.
        let tenths = elapsed.subsec_millis() / 100;
        return format!("0.{tenths}s");
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    let mins = secs / 60;
    let rem = secs % 60;
    format!("{mins}m {rem:02}s")
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
            current_window_height: None,
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

    /// Force a re-render every ~250ms while `run_id` is the active running
    /// command. The pulsing-dot animation already re-paints on its own (GPUI
    /// drives that via `request_animation_frame`), but the elapsed-time label
    /// is computed inside `render()` and only refreshes on `cx.notify()`.
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
            label: action.label,
            started_at: Instant::now(),
        });
        self.command_output = Some(CommandOutput::running(&command_line, action));
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

    /// Build a `HeightSnapshot` from the current palette state. Called
    /// once per `render()` to drive the window-resize sync.
    fn height_snapshot(&self, actions: &[CommandAction], hide_list: bool) -> HeightSnapshot {
        let selected_action = actions.get(self.selected).copied();
        let footer_visible = selected_action.is_some();
        // Only `has_body()` outputs are actually rendered (see `ui_render.rs`
        // line ~127); a body-less notice produces no card and therefore no
        // height contribution. Tracking only the body case avoids reserving
        // blank space for content that is never drawn.
        let output_body_visible = matches!(&self.command_output, Some(o) if o.has_body());
        // The action list mounts when not hidden. With zero matches it
        // still renders a one-row "No matching commands" placeholder.
        let list_visible = !hide_list;
        let empty_placeholder_visible = list_visible && actions.is_empty();
        HeightSnapshot {
            action_row_count: if list_visible { actions.len() } else { 0 },
            empty_placeholder_visible,
            footer_visible,
            output_body_visible,
        }
    }

    /// Snap the window height to `target_height` when it differs from the
    /// last committed value. This is the v1 "no easing" approach — render
    /// is already called when content changes, so the resize tracks the
    /// content. Per the PR #99 launch-time hotfix, we do NOT spawn a
    /// repeating tick task; resize only fires in response to user-driven
    /// content changes (typing, command runs, dismissals).
    fn sync_window_height(&mut self, target_height: f32, window: &mut Window) {
        // The minimum target is `MIN_WINDOW_HEIGHT` — never let the
        // computed value collapse the window below the prompt row.
        let clamped = target_height.max(MIN_WINDOW_HEIGHT);
        let needs_resize = match self.current_window_height {
            None => true,
            Some(prev) => (prev - clamped).abs() > 0.5,
        };
        if needs_resize {
            let current_width = window.bounds().size.width;
            window.resize(Size {
                width: current_width,
                height: px(clamped),
            });
            self.current_window_height = Some(clamped);
        }
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
