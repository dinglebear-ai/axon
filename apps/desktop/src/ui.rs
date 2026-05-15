use std::process::{Command, Output};

use gpui::{
    App, Context, FocusHandle, Focusable, FontWeight, IntoElement, ParentElement, Render,
    ScrollHandle, SharedString, Styled, Window, div, prelude::*, px, rgb,
};

use crate::actions::{
    ACTIONS, ArgMode, CommandAction, action_invoked_by, action_matches, build_axon_args,
    display_command_line, looks_like_url,
};
use crate::output::{CommandOutput, OutputKind};
use crate::render::{
    render_action_rows, render_output_body, render_palette_footer, render_prompt_row,
};
use crate::theme::{
    AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_FONT_SANS, AURORA_NAV_BG, AURORA_PAGE_BG,
    AURORA_PANEL_STRONG, AURORA_TEXT_PRIMARY,
};
use crate::{MoveDown, MoveUp, Submit};

pub(crate) struct Palette {
    query: String,
    selected: usize,
    focus: FocusHandle,
    command_output: Option<CommandOutput>,
    running: Option<RunningCommand>,
    next_run_id: u64,
    output_scroll: ScrollHandle,
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

impl Palette {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            query: String::new(),
            selected: 0,
            focus: cx.focus_handle(),
            command_output: None,
            running: None,
            next_run_id: 1,
            output_scroll: ScrollHandle::new(),
        }
    }

    fn matches(&self) -> Vec<CommandAction> {
        let input = self.query.trim();
        let head = input.split_whitespace().next().unwrap_or("");
        let direct_url = looks_like_url(input);

        ACTIONS
            .iter()
            .copied()
            .filter(|action| {
                input.is_empty()
                    || action_matches(*action, head)
                    || action_matches(*action, input)
                    || (direct_url && action.accepts_direct_url())
            })
            .collect()
    }

    fn submit(&mut self, _: &Submit, _window: &mut Window, cx: &mut Context<Self>) {
        let actions = self.matches();
        let Some(action) = actions.get(self.selected).copied() else {
            return;
        };

        let arg = self.argument_for(action);

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

        let args = match build_axon_args(action, arg) {
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
            let mut cmd = Command::new("axon");
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
                self.query.pop();
            }
            "escape" => {
                if self.command_output.is_some() {
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let actions = self.matches();
        let selected = self.selected;
        let selected_action = actions.get(selected).copied();
        let running_subcommand = self.running.as_ref().map(|running| running.subcommand);
        let command_output = self.command_output.clone();
        let prompt = if self.query.is_empty() {
            SharedString::from("type a command or URL")
        } else {
            SharedString::from(format!("> {}", self.query))
        };

        div()
            .key_context("Palette")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::submit))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_up))
            .on_key_down(cx.listener(Self::on_key))
            .flex()
            .flex_col()
            .size_full()
            .overflow_hidden()
            .font_family(AURORA_FONT_SANS)
            .font_weight(FontWeight(480.0))
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
                    .rounded_md()
                    .bg(rgb(AURORA_PANEL_STRONG))
                    .border_1()
                    .border_color(rgb(AURORA_BORDER_STRONG))
                    .shadow_lg()
                    .child(render_prompt_row(self.query.is_empty(), prompt))
                    .child(render_action_rows(actions, selected, running_subcommand))
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
