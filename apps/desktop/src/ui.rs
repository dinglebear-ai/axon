use std::time::{Duration, Instant};

use gpui::{App, Context, FocusHandle, Focusable, ScrollHandle, SharedString, Size, Window, px};

use crate::actions::{
    ACTIONS, ArgMode, CommandAction, action_invoked_by, action_matches, looks_like_url,
};
use crate::layout::{HeightSnapshot, MIN_WINDOW_HEIGHT};
use crate::output::CommandOutput;
use crate::theme::AURORA_BORDER_STRONG;
use crate::{ClearOutput, MoveDown, MoveUp, TabComplete, ToggleActionMenu, ToggleErrors};

#[cfg(test)]
#[path = "ui_tests.rs"]
mod tests;

// `Render for Palette` impl lives in `ui_render.rs`. Sibling file declared
// with `#[path]` so it remains a child module of `ui` and retains access
// to `Palette`'s private fields. See the project monolith policy.
#[path = "ui_render.rs"]
mod ui_render;

#[path = "ui_commands.rs"]
mod ui_commands;

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

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChromeMenu {
    File,
    Edit,
    View,
    Run,
    Help,
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
    action_menu_open: bool,
    chrome_menu_open: Option<ChromeMenu>,
    errors_open: bool,
    connection: ConnectionState,
    /// Monotonic id for in-flight health checks. Each spawn increments this;
    /// completions only apply when their captured id still matches the latest,
    /// so a slower older probe can't overwrite a newer result.
    health_check_id: u64,
    /// Last height the palette itself pushed via `Window::resize`.
    /// User-driven resizes are intentionally not written here; if we store a
    /// manual height as the auto height, the next render sees `auto != target`
    /// and snaps the window back to the content cap.
    last_auto_window_height: Option<f32>,
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
            action_menu_open: false,
            chrome_menu_open: None,
            errors_open: false,
            connection: ConnectionState::Unknown,
            health_check_id: 0,
            last_auto_window_height: None,
        };
        palette.spawn_health_check(cx);
        palette
    }

    fn matches(&self) -> Vec<CommandAction> {
        if self.action_menu_open {
            return ACTIONS.to_vec();
        }
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
        self.errors_open = false;
        cx.notify();
    }

    fn toggle_errors(&mut self, _: &ToggleErrors, _window: &mut Window, cx: &mut Context<Self>) {
        self.errors_open = !self.errors_open;
        cx.notify();
    }

    fn toggle_action_menu(
        &mut self,
        _: &ToggleActionMenu,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.action_menu_open = !self.action_menu_open;
        self.chrome_menu_open = None;
        self.selected = self.selected_for_locked_action();
        cx.notify();
    }

    fn tab_complete(&mut self, _: &TabComplete, _window: &mut Window, cx: &mut Context<Self>) {
        if self.query.trim().is_empty() {
            self.action_menu_open = true;
            self.chrome_menu_open = None;
            self.selected = self.selected_for_locked_action();
            cx.notify();
            return;
        }
        if self.locked_command.is_some() {
            return;
        }
        let actions = self.matches();
        if let Some(action) = actions.get(self.selected).copied() {
            self.select_action_mode(action, true, cx);
        }
    }

    fn move_down(&mut self, _: &MoveDown, _w: &mut Window, cx: &mut Context<Self>) {
        let n = self.matches().len();
        if n == 0 && self.query.trim().is_empty() {
            self.action_menu_open = true;
            self.chrome_menu_open = None;
            self.selected = self.selected_for_locked_action();
            cx.notify();
            return;
        }
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
                if self.chrome_menu_open.is_some() {
                    self.chrome_menu_open = None;
                } else if self.action_menu_open {
                    self.action_menu_open = false;
                } else if self.locked_command.is_some() {
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
                        self.action_menu_open = false;
                        self.chrome_menu_open = None;
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

    /// Snap the window height to `target_height` when the palette itself owns
    /// the current size. If the user manually resized taller, preserve that
    /// size and let the flex layout expand into it.
    fn sync_window_height(&mut self, target_height: f32, window: &mut Window) {
        // The minimum target is `MIN_WINDOW_HEIGHT` — never let the
        // computed value collapse the window below the prompt row.
        let clamped = target_height.max(MIN_WINDOW_HEIGHT) + crate::layout::WINDOW_CHROME_HEIGHT;
        let current_height = window.bounds().size.height.as_f32();
        if preserves_manual_height(self.last_auto_window_height, current_height, clamped) {
            return;
        }

        let needs_resize = match self.last_auto_window_height {
            None => true,
            Some(prev) => current_height < clamped - 0.5 || (prev - clamped).abs() > 0.5,
        };
        if needs_resize {
            let current_width = window.bounds().size.width;
            window.resize(Size {
                width: current_width,
                height: px(clamped),
            });
            self.last_auto_window_height = Some(clamped);
        }
    }

    /// `/v1/ask` is currently stateless, so the REST-backed palette does not
    /// render a conversation footer.
    fn conversation_hint(&self) -> Option<SharedString> {
        None
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

    fn select_action_mode(
        &mut self,
        action: CommandAction,
        strip_query_command: bool,
        cx: &mut Context<Self>,
    ) {
        self.locked_command = Some(action);
        self.action_menu_open = false;
        if strip_query_command {
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
        }
        self.selected = 0;
        cx.notify();
    }

    fn selected_for_locked_action(&self) -> usize {
        self.locked_command
            .and_then(|locked| {
                ACTIONS
                    .iter()
                    .position(|action| action.subcommand == locked.subcommand)
            })
            .unwrap_or(0)
    }

    fn toggle_chrome_menu(&mut self, menu: ChromeMenu, cx: &mut Context<Self>) {
        self.chrome_menu_open = if self.chrome_menu_open == Some(menu) {
            None
        } else {
            Some(menu)
        };
        self.action_menu_open = false;
        cx.notify();
    }
}

fn preserves_manual_height(
    last_auto_height: Option<f32>,
    current_height: f32,
    target_height: f32,
) -> bool {
    match last_auto_height {
        Some(auto_height) => {
            (auto_height - current_height).abs() > 0.5 && current_height >= target_height
        }
        None => false,
    }
}

impl Focusable for Palette {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}
