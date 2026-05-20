use gpui::{
    Context, FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement, ScrollHandle,
    SharedString, Styled, div, prelude::*, px, rgb,
};

use crate::settings::{AxonSettings, SettingsEntry, SettingsStatusKind};
use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_BORDER_DEFAULT, AURORA_BORDER_STRONG, AURORA_FONT_MONO,
    AURORA_NAV_BG, AURORA_PANEL_STRONG, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};
use crate::ui::Palette;

pub(crate) fn render_settings_view(
    settings: &AxonSettings,
    settings_scroll: &ScrollHandle,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    div()
        .id("settings-scroll")
        .flex_1()
        .min_h_0()
        .overflow_hidden()
        .p_5()
        .flex()
        .flex_col()
        .child(
            div()
                .w_full()
                .mx_auto()
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .rounded_md()
                .bg(rgb(AURORA_PANEL_STRONG))
                .border_1()
                .border_color(rgb(AURORA_BORDER_STRONG))
                .shadow_lg()
                .child(render_settings_header(settings))
                .child(render_settings_rows(settings, settings_scroll, cx))
                .child(render_settings_status(settings)),
        )
}

fn render_settings_header(settings: &AxonSettings) -> impl IntoElement {
    div()
        .h(px(72.0))
        .px_4()
        .flex()
        .flex_col()
        .justify_center()
        .gap_1()
        .border_b_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .font_weight(FontWeight(650.0))
                        .text_size(px(15.0))
                        .child("Settings"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(rgb(AURORA_ACCENT_PRIMARY))
                        .child(format!("{} changed", settings.dirty_count())),
                ),
        )
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .text_size(px(11.0))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .child(format!(
                    "{}  |  {}",
                    settings.env_path.display(),
                    settings.toml_path.display()
                )),
        )
}

fn render_settings_rows(
    settings: &AxonSettings,
    settings_scroll: &ScrollHandle,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    let mut rows = Vec::new();
    let mut last_section = "";
    for (idx, entry) in settings.entries.iter().enumerate() {
        if entry.section != last_section {
            last_section = entry.section;
            rows.push(render_section_header(entry.section).into_any_element());
        }
        rows.push(
            render_setting_row(
                entry,
                idx,
                idx == settings.selected,
                settings.reveal_secrets,
                cx,
            )
            .into_any_element(),
        );
    }

    div()
        .id("settings-scroll")
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .scrollbar_width(px(12.0))
        .track_scroll(settings_scroll)
        .block_mouse_except_scroll()
        .bg(rgb(AURORA_NAV_BG))
        .children(rows)
}

fn render_section_header(label: &'static str) -> impl IntoElement {
    div()
        .h(px(30.0))
        .px_4()
        .mt_2()
        .flex()
        .items_end()
        .border_b_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .text_size(px(11.0))
        .font_weight(FontWeight(650.0))
        .text_color(rgb(AURORA_TEXT_MUTED))
        .child(label)
}

fn render_setting_row(
    entry: &SettingsEntry,
    idx: usize,
    selected: bool,
    reveal_secrets: bool,
    cx: &mut Context<Palette>,
) -> impl IntoElement {
    let value = display_value(entry, reveal_secrets);
    div()
        .id(SharedString::from(format!("settings-row-{idx}")))
        .block_mouse_except_scroll()
        .min_h(px(54.0))
        .px_4()
        .py_2()
        .flex()
        .items_center()
        .gap_3()
        .border_b_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .when(selected, |el| {
            el.bg(rgb(0x162334))
                .border_color(rgb(AURORA_ACCENT_PRIMARY))
        })
        .hover(|el| el.bg(rgb(0x1a2633)))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _: &MouseDownEvent, _window, cx| {
                this.settings.selected = idx;
                cx.notify();
            }),
        )
        .child(render_key_cell(entry))
        .child(render_value_cell(value, selected, entry.dirty()))
}

fn render_key_cell(entry: &SettingsEntry) -> impl IntoElement {
    div()
        .w(px(260.0))
        .flex_shrink_0()
        .flex()
        .flex_col()
        .gap_1()
        .child(
            div()
                .font_family(AURORA_FONT_MONO)
                .text_size(px(12.0))
                .text_color(rgb(AURORA_TEXT_PRIMARY))
                .child(entry.key.clone()),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .child(format!("{} | {}", entry.source.label(), entry.description)),
        )
}

fn render_value_cell(value: String, selected: bool, dirty: bool) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .h(px(34.0))
        .px_3()
        .flex()
        .items_center()
        .rounded_sm()
        .border_1()
        .border_color(if selected {
            rgb(AURORA_ACCENT_PRIMARY)
        } else {
            rgb(AURORA_BORDER_DEFAULT)
        })
        .bg(rgb(0x0b141d))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .font_family(AURORA_FONT_MONO)
                .text_size(px(12.0))
                .text_color(if value.is_empty() {
                    rgb(AURORA_TEXT_MUTED)
                } else {
                    rgb(AURORA_TEXT_PRIMARY)
                })
                .child(if value.is_empty() {
                    "unset".to_string()
                } else {
                    value
                }),
        )
        .when(dirty, |el| {
            el.child(
                div()
                    .ml_2()
                    .size(px(7.0))
                    .rounded_full()
                    .bg(rgb(AURORA_ACCENT_PRIMARY)),
            )
        })
}

fn render_settings_status(settings: &AxonSettings) -> impl IntoElement {
    let (message, color) = match &settings.status {
        Some(status) => {
            let color = match status.kind {
                SettingsStatusKind::Info => AURORA_TEXT_MUTED,
                SettingsStatusKind::Success => 0x4ade80,
                SettingsStatusKind::Error => 0xf87171,
            };
            (status.message.clone(), color)
        }
        None => (
            "Type to edit selected value. Backspace clears chars. File > Save Settings writes changed keys.".to_string(),
            AURORA_TEXT_MUTED,
        ),
    };

    div()
        .min_h(px(38.0))
        .px_4()
        .flex()
        .items_center()
        .border_t_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .text_size(px(12.0))
        .text_color(rgb(color))
        .child(message)
}

fn display_value(entry: &SettingsEntry, reveal_secrets: bool) -> String {
    if entry.secret && !entry.value.is_empty() && !reveal_secrets {
        "********".to_string()
    } else {
        entry.value.clone()
    }
}
