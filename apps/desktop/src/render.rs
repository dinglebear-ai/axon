use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, App, Div, FontWeight, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, SharedString, Stateful, Styled, Window, div, ease_in_out,
    prelude::*, px, rgb,
};

use crate::ClearOutput;
use crate::actions::{ArgMode, CommandAction};
use crate::output::{CommandOutput, OutputKind};
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG,
    AURORA_CONTROL_SURFACE, AURORA_FONT_DISPLAY, AURORA_FONT_MONO, AURORA_HOVER_BG, AURORA_NAV_BG,
    AURORA_OUTPUT_MUTED, AURORA_PANEL_STRONG, AURORA_PRESSED_BG, AURORA_ROW_HOVER_BG,
    AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};

pub(crate) fn render_prompt_row(
    query_is_empty: bool,
    locked_command: Option<CommandAction>,
    prompt: SharedString,
    status_dot: impl IntoElement,
) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .h(px(52.0))
        .px_5()
        .border_b_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .child(status_dot)
        .child(
            div()
                .font_family(AURORA_FONT_DISPLAY)
                .font_weight(FontWeight(780.0))
                .text_size(px(16.0))
                .text_color(rgb(AURORA_TEXT_PRIMARY))
                .child("axon"),
        )
        // Thin vertical separator between the brand mark and the input area —
        // gives the eye a place to break so "axon" reads as a label, not as
        // part of whatever the user is typing.
        .child(
            div()
                .w(px(1.0))
                .h(px(18.0))
                .bg(rgb(AURORA_BORDER_DEFAULT))
                .flex_shrink_0(),
        )
        .when_some(locked_command, |el, action| {
            el.child(
                div()
                    .px_2()
                    .py(px(3.0))
                    .rounded_md()
                    .bg(rgb(AURORA_NAV_BG))
                    .border_1()
                    .border_color(rgb(AURORA_ACCENT_STRONG))
                    .font_family(AURORA_FONT_MONO)
                    .font_weight(FontWeight(680.0))
                    .text_size(px(11.0))
                    .text_color(rgb(AURORA_ACCENT_STRONG))
                    .child(action.subcommand),
            )
        })
        .child(
            div()
                .flex_1()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                // Animated caret prefix — gently pulses opacity when the
                // user is typing, so the palette feels alive even when no
                // keystroke is in flight. Reverts to a static muted glyph
                // when the query is empty (no flicker on the empty state).
                .child(
                    div()
                        .font_family(AURORA_FONT_MONO)
                        .font_weight(FontWeight(700.0))
                        .text_size(px(14.0))
                        .text_color(rgb(if query_is_empty {
                            AURORA_BORDER_STRONG
                        } else {
                            AURORA_ACCENT_PRIMARY
                        }))
                        .child("›")
                        .with_animation(
                            "palette-caret",
                            Animation::new(Duration::from_millis(1100))
                                .repeat()
                                .with_easing(ease_in_out),
                            move |el, t| {
                                let opacity = if query_is_empty {
                                    1.0
                                } else {
                                    // 0.45..1.0 triangle wave so the pulse is
                                    // visible but never "off".
                                    let v = 1.0 - (t * 2.0 - 1.0).abs();
                                    0.45 + 0.55 * v
                                };
                                el.opacity(opacity)
                            },
                        ),
                )
                .child(
                    div()
                        .flex_1()
                        .font_weight(FontWeight(500.0))
                        .text_size(px(14.5))
                        .text_color(if query_is_empty {
                            rgb(AURORA_TEXT_MUTED)
                        } else {
                            rgb(AURORA_TEXT_PRIMARY)
                        })
                        .child(prompt),
                ),
        )
}



pub(crate) fn render_palette_footer(
    action: CommandAction,
    output: Option<&CommandOutput>,
    is_running: bool,
) -> impl IntoElement {
    let status = output.map(|o| o.kind).unwrap_or(OutputKind::Warning);
    let accent = if is_running {
        OutputKind::Running.accent_color()
    } else if output.is_some() {
        status.accent_color()
    } else {
        AURORA_BORDER_STRONG
    };
    let label = if is_running {
        "running"
    } else if let Some(output) = output {
        output.kind.label()
    } else {
        "enter"
    };
    let title = output
        .map(|o| o.title.as_str())
        .unwrap_or(action.description);
    let detail = output
        .map(|o| o.subtitle.as_str())
        .unwrap_or(action.example);
    let has_output = output.is_some();

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_3()
        .px_5()
        .py(px(10.0))
        .border_t_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_CONTROL_SURFACE))
        .child(
            // Status pip with a soft accent ring — gives the dot weight
            // without making it loud.
            div()
                .size(px(8.0))
                .rounded_full()
                .flex_shrink_0()
                .border_1()
                .border_color(rgb(AURORA_BORDER_STRONG))
                .bg(rgb(accent)),
        )
        .child(
            div()
                .px_2()
                .py(px(2.0))
                .rounded_md()
                .bg(rgb(AURORA_NAV_BG))
                .border_1()
                .border_color(rgb(accent))
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(680.0))
                .text_size(px(10.5))
                .text_color(rgb(accent))
                .child(label),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .gap_px()
                .child(
                    div()
                        .font_weight(FontWeight(640.0))
                        .text_size(px(12.5))
                        .text_color(rgb(AURORA_TEXT_PRIMARY))
                        .child(SharedString::from(title.to_string())),
                )
                .child(
                    div()
                        .font_family(AURORA_FONT_MONO)
                        .font_weight(FontWeight(480.0))
                        .text_size(px(11.0))
                        .text_color(rgb(AURORA_TEXT_MUTED))
                        .child(SharedString::from(detail.to_string())),
                ),
        )
        // Keybinding hints — Spotlight/Raycast pattern. Small ghosted chips
        // pointing at the shortcuts that work right here. Two slots: the
        // primary action (Enter / clear) and an Esc fallback.
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .flex_shrink_0()
                .child(keybinding_chip("↵", if has_output { "again" } else { "run" }))
                .child(keybinding_chip("esc", "dismiss")),
        )
        // Dismiss affordance — shown only when there's output to clear.
        // Hover gives it accent-color so it reads as "this does something".
        .when(has_output, |el| {
            el.child(
                div()
                    .id("palette-footer-clear")
                    .cursor_pointer()
                    .px_2()
                    .py(px(3.0))
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(AURORA_BORDER_DEFAULT))
                    .font_family(AURORA_FONT_MONO)
                    .font_weight(FontWeight(600.0))
                    .text_size(px(11.0))
                    .text_color(rgb(AURORA_TEXT_MUTED))
                    .hover(|el| {
                        el.text_color(rgb(AURORA_TEXT_PRIMARY))
                            .border_color(rgb(AURORA_BORDER_STRONG))
                    })
                    .on_mouse_down(MouseButton::Left, |_: &MouseDownEvent, window, cx| {
                        window.dispatch_action(Box::new(ClearOutput), cx);
                    })
                    .child("clear ✕"),
            )
        })
}

// ── private helpers ───────────────────────────────────────────────────────────

// Spotlight-style keybinding chip: ghosted box with the key glyph + tiny label.
fn keybinding_chip(key: &'static str, label: &'static str) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .items_center()
        .gap_1()
        .child(
            div()
                .min_w(px(20.0))
                .px_1()
                .py(px(1.0))
                .rounded_sm()
                .border_1()
                .border_color(rgb(AURORA_BORDER_STRONG))
                .bg(rgb(AURORA_NAV_BG))
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(620.0))
                .text_size(px(10.0))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .child(key),
        )
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(480.0))
                .text_size(px(10.0))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .child(label),
        )
}

// Resize the palette window to hug content. 6px deadband stops render→resize loops.
pub(crate) fn reflow_palette_window(
    last_height: &mut f32,
    rows: usize,
    footer: bool,
    output: bool,
    window: &mut Window,
) {
    let mut target = 40.0 + 2.0 + 48.0; // outer p_5 + border + prompt row
    if rows > 0 {
        target += 8.0 + (rows as f32) * 36.0;
    }
    if footer {
        target += 56.0;
    }
    if output {
        target += 320.0;
    }
    if (target - *last_height).abs() > 6.0 {
        let current = window.bounds().size;
        window.resize(gpui::size(current.width, px(target)));
        *last_height = target;
    }
}

// Action list with per-row click + hover. Hover updates `selected_index`
// so keyboard and mouse share one source of truth — no "two highlighted
// rows" problem.
pub(crate) fn render_action_list<F, L, H, R>(
    actions: Vec<CommandAction>,
    selected: usize,
    running_subcommand: Option<&'static str>,
    hide_list: bool,
    mut make_on_click: F,
    mut make_on_hover: H,
) -> impl IntoElement
where
    F: FnMut(usize) -> L,
    L: Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    H: FnMut(usize) -> R,
    R: Fn(&bool, &mut Window, &mut App) + 'static,
{
    let is_empty = actions.is_empty();
    let rows: Vec<AnyElement> = actions
        .into_iter()
        .enumerate()
        .map(|(i, action)| {
            let is_running = running_subcommand == Some(action.subcommand);
            // Per-row fade-in: each row is keyed by its index, so a new
            // result set re-runs the animation. Cheap (one shot, 140ms).
            render_action_row(action, i, i == selected, is_running)
                .on_mouse_down(MouseButton::Left, make_on_click(i))
                .on_hover(make_on_hover(i))
                .with_animation(
                    ("axon-action-row-fade", i),
                    Animation::new(Duration::from_millis(140)).with_easing(ease_in_out),
                    |el, t| el.opacity(t),
                )
                .into_any_element()
        })
        .collect();
    div()
        .flex()
        .flex_col()
        .py_1()
        .when(is_empty && !hide_list, |el| el.child(render_empty_row()))
        .when(!is_empty, |el| el.children(rows))
}

// Polished empty state — vertically centered glyph + headline + hint.
// Replaces the previous one-liner so a no-match query feels intentional,
// not broken.
fn render_empty_row() -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .px_5()
        .py(px(20.0))
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(420.0))
                .text_size(px(20.0))
                .text_color(rgb(AURORA_BORDER_STRONG))
                .child("⌕"),
        )
        .child(
            div()
                .font_weight(FontWeight(580.0))
                .text_size(px(12.5))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .child("No matching commands"),
        )
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(440.0))
                .text_size(px(10.5))
                .text_color(rgb(AURORA_OUTPUT_MUTED))
                .child("try ‘scrape’, ‘ask’, or paste a URL"),
        )
}

// Builds a row Stateful<Div>; caller chains .on_mouse_down/.on_hover for behavior.
fn render_action_row(
    action: CommandAction,
    index: usize,
    is_selected: bool,
    is_running: bool,
) -> Stateful<Div> {
    let meta = if is_running {
        "running"
    } else if action.arg_mode == ArgMode::None {
        action.subcommand
    } else {
        "argument"
    };
    let meta_color = if is_running {
        AURORA_ACCENT_PRIMARY
    } else if action.arg_mode == ArgMode::None {
        AURORA_TEXT_MUTED
    } else {
        AURORA_ACCENT_STRONG
    };

    let base_bg = if is_selected {
        AURORA_HOVER_BG
    } else {
        AURORA_PANEL_STRONG
    };
    // Keep the selected row's highlight from being "dimmed" by a hover, while
    // still giving plain rows a soft hover lift.
    let hover_bg = if is_selected {
        AURORA_HOVER_BG
    } else {
        AURORA_ROW_HOVER_BG
    };

    // Left-edge accent bar (Spotlight/Raycast pattern): a thin vertical strip
    // that lights up on the selected row. Eliminates the ambiguity between
    // "background-tinted row" and "actually focused row" at a glance.
    let accent_bar = div()
        .w(px(3.0))
        .h(px(20.0))
        .rounded_full()
        .flex_shrink_0()
        .bg(rgb(if is_selected {
            AURORA_ACCENT_PRIMARY
        } else {
            0x00000000
        }));

    div()
        .id(("axon-action-row", index))
        .h(px(38.0))
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .gap_3()
        .mx_1()
        .pl_2()
        .pr_3()
        .rounded_md()
        .cursor_pointer()
        // 1px outline ring on the selected row, accent-colored. Pairs with
        // the left accent bar so selection reads both as a glyph (the bar)
        // and as a chrome treatment (the ring) — picker-style focus_visible.
        .border_1()
        .border_color(rgb(if is_selected {
            AURORA_ACCENT_PRIMARY
        } else {
            0x00000000
        }))
        .bg(rgb(base_bg))
        .hover(move |el| el.bg(rgb(hover_bg)))
        .active(|el| el.bg(rgb(AURORA_PRESSED_BG)))
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_3()
                .child(accent_bar)
                .child(
                    div()
                        .font_weight(if is_selected {
                            FontWeight(640.0)
                        } else {
                            FontWeight(500.0)
                        })
                        .text_size(px(13.0))
                        .text_color(if is_selected {
                            rgb(AURORA_TEXT_PRIMARY)
                        } else {
                            rgb(AURORA_TEXT_MUTED)
                        })
                        .child(SharedString::from(action.label)),
                ),
        )
        .child(
            // Pill-style meta badge: soft border + bg gives the keyword an
            // identifiable shape that reads as "input affordance" without
            // shouting. Replaces the previous bare-text label.
            div()
                .px_2()
                .py(px(2.0))
                .rounded_sm()
                .border_1()
                .border_color(rgb(AURORA_BORDER_DEFAULT))
                .bg(rgb(AURORA_NAV_BG))
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(580.0))
                .text_size(px(10.0))
                .text_color(rgb(meta_color))
                .child(meta),
        )
}
