use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use gpui::{
    FontWeight, IntoElement, MouseButton, MouseDownEvent, ParentElement, SharedString, Styled, div,
    prelude::*, px, rgb,
};

use crate::theme::{
    AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_BORDER_DEFAULT, AURORA_CONTROL_SURFACE,
    AURORA_FONT_MONO, AURORA_OUTPUT_TEXT, AURORA_TEXT_MUTED, AURORA_TEXT_PRIMARY,
};

// ── inline span ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct Span {
    text: String,
    bold: bool,
    italic: bool,
    code: bool,
    /// When set, the span is a clickable hyperlink.
    link_url: Option<String>,
}

// ── block types ───────────────────────────────────────────────────────────────

enum Block {
    Heading { level: HeadingLevel, spans: Vec<Span> },
    Paragraph(Vec<Span>),
    Code(String),
    ListItem { ordered: bool, number: u64, spans: Vec<Span> },
    Rule,
}

// ── public entry point ────────────────────────────────────────────────────────

pub(crate) fn render_markdown(text: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .gap_2()
        .children(parse(text).into_iter().map(render_block))
}

// ── parser ────────────────────────────────────────────────────────────────────

fn parse(input: &str) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut spans: Vec<Span> = Vec::new();
    let mut bold = false;
    let mut italic = false;
    let mut link_stack: Vec<String> = Vec::new(); // dest URLs for nested links
    let mut in_heading: Option<HeadingLevel> = None;
    let mut in_code_block = false;
    let mut code_text = String::new();
    let mut list_stack: Vec<Option<u64>> = Vec::new();
    let mut in_item = false;

    let opts = Options::ENABLE_STRIKETHROUGH;
    for event in Parser::new_ext(input, opts) {
        match event {
            // ── headings ──────────────────────────────────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                in_heading = Some(level);
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(level) = in_heading.take() {
                    let s = std::mem::take(&mut spans);
                    if !s.is_empty() {
                        blocks.push(Block::Heading { level, spans: s });
                    }
                }
            }

            // ── paragraphs ────────────────────────────────────────────────────
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => {
                flush_spans(&mut blocks, &mut spans, in_item, &list_stack);
            }

            // ── code blocks ───────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                code_text.clear();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                let text = std::mem::take(&mut code_text);
                if !text.is_empty() {
                    blocks.push(Block::Code(text));
                }
            }

            // ── lists ─────────────────────────────────────────────────────────
            Event::Start(Tag::List(start)) => {
                list_stack.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                list_stack.pop();
            }
            Event::Start(Tag::Item) => {
                in_item = true;
            }
            Event::End(TagEnd::Item) => {
                flush_spans(&mut blocks, &mut spans, in_item, &list_stack);
                if let Some(Some(n)) = list_stack.last_mut() {
                    *n += 1;
                }
                in_item = false;
            }

            // ── inline styles ─────────────────────────────────────────────────
            Event::Start(Tag::Strong) => bold = true,
            Event::End(TagEnd::Strong) => bold = false,
            Event::Start(Tag::Emphasis) => italic = true,
            Event::End(TagEnd::Emphasis) => italic = false,

            // ── links — show the URL, skip the label text ─────────────────────
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_stack.push(dest_url.into_string());
            }
            Event::End(TagEnd::Link) => {
                if let Some(url) = link_stack.pop() {
                    if !url.is_empty() {
                        spans.push(Span {
                            text: url.clone(),
                            bold: false,
                            italic: false,
                            code: false,
                            link_url: Some(url),
                        });
                    }
                }
            }

            // ── text ──────────────────────────────────────────────────────────
            Event::Text(text) if in_code_block => {
                code_text.push_str(&text);
            }
            Event::Text(_) if !link_stack.is_empty() => {
                // We're inside a link — skip the label; we'll show the URL at End(Link).
            }
            Event::Text(text) => {
                let t = text.into_string();
                if !t.is_empty() {
                    spans.push(Span { text: t, bold, italic, code: false, link_url: None });
                }
            }
            Event::Code(text) => {
                let t = text.into_string();
                if !t.is_empty() {
                    spans.push(Span { text: t, bold, italic, code: true, link_url: None });
                }
            }
            Event::SoftBreak => {
                if link_stack.is_empty() {
                    spans.push(Span {
                        text: " ".into(),
                        bold: false,
                        italic: false,
                        code: false,
                        link_url: None,
                    });
                }
            }
            Event::HardBreak => {
                flush_spans(&mut blocks, &mut spans, in_item, &list_stack);
            }
            Event::Rule => {
                blocks.push(Block::Rule);
            }
            _ => {}
        }
    }

    flush_spans(&mut blocks, &mut spans, in_item, &list_stack);
    blocks
}

fn flush_spans(
    blocks: &mut Vec<Block>,
    spans: &mut Vec<Span>,
    in_item: bool,
    list_stack: &[Option<u64>],
) {
    let s = std::mem::take(spans);
    if s.is_empty() {
        return;
    }
    if in_item {
        let (ordered, number) = list_info(list_stack);
        blocks.push(Block::ListItem { ordered, number, spans: s });
    } else {
        blocks.push(Block::Paragraph(s));
    }
}

fn list_info(stack: &[Option<u64>]) -> (bool, u64) {
    match stack.last() {
        Some(Some(n)) => (true, *n),
        _ => (false, 0),
    }
}

// ── rendering ─────────────────────────────────────────────────────────────────

fn render_block(block: Block) -> impl IntoElement {
    match block {
        Block::Heading { level, spans } => render_heading(level, spans).into_any_element(),
        Block::Paragraph(spans) => render_para(spans).into_any_element(),
        Block::Code(text) => render_code_block(text).into_any_element(),
        Block::ListItem { ordered, number, spans } => {
            render_list_item(ordered, number, spans).into_any_element()
        }
        Block::Rule => render_rule().into_any_element(),
    }
}

fn render_heading(level: HeadingLevel, spans: Vec<Span>) -> impl IntoElement {
    let (size, weight) = match level {
        HeadingLevel::H1 => (px(17.0), FontWeight(760.0)),
        HeadingLevel::H2 => (px(15.0), FontWeight(720.0)),
        HeadingLevel::H3 => (px(13.0), FontWeight(680.0)),
        _ => (px(12.0), FontWeight(660.0)),
    };
    div()
        .font_weight(weight)
        .text_size(size)
        .text_color(rgb(AURORA_TEXT_PRIMARY))
        .pt_2()
        .child(render_spans(spans))
}

fn render_para(spans: Vec<Span>) -> impl IntoElement {
    div()
        .font_weight(FontWeight(480.0))
        .text_size(px(13.0))
        .text_color(rgb(AURORA_OUTPUT_TEXT))
        .child(render_spans(spans))
}

fn render_code_block(text: String) -> impl IntoElement {
    div()
        .rounded_sm()
        .border_1()
        .border_color(rgb(AURORA_BORDER_DEFAULT))
        .bg(rgb(AURORA_CONTROL_SURFACE))
        .px_3()
        .py_2()
        .flex()
        .flex_col()
        .children(text.lines().map(|line| {
            div()
                .font_family(AURORA_FONT_MONO)
                .font_weight(FontWeight(500.0))
                .text_size(px(11.0))
                .line_height(px(18.0))
                .text_color(rgb(AURORA_OUTPUT_TEXT))
                .child(SharedString::from(line.to_string()))
        }))
}

fn render_list_item(ordered: bool, number: u64, spans: Vec<Span>) -> impl IntoElement {
    let bullet: SharedString = if ordered {
        SharedString::from(format!("{number}."))
    } else {
        SharedString::from("•")
    };
    div()
        .flex()
        .flex_row()
        .gap_2()
        .items_start()
        .child(
            div()
                .font_weight(FontWeight(500.0))
                .text_size(px(13.0))
                .text_color(rgb(AURORA_TEXT_MUTED))
                .flex_shrink_0()
                .child(bullet),
        )
        .child(
            div()
                .flex_1()
                .font_weight(FontWeight(480.0))
                .text_size(px(13.0))
                .text_color(rgb(AURORA_OUTPUT_TEXT))
                .child(render_spans(spans)),
        )
}

fn render_rule() -> impl IntoElement {
    div().h(px(1.0)).w_full().my_1().bg(rgb(AURORA_BORDER_DEFAULT))
}

fn render_spans(spans: Vec<Span>) -> impl IntoElement {
    div()
        .flex()
        .flex_row()
        .flex_wrap()
        .children(spans.into_iter().map(render_span))
}

fn render_span(span: Span) -> impl IntoElement {
    if let Some(url) = span.link_url {
        // Clickable URL — opens in the system browser.
        let url_clone = url.clone();
        div()
            .cursor_pointer()
            .font_weight(FontWeight(500.0))
            .text_size(px(13.0))
            .text_color(rgb(AURORA_ACCENT_PRIMARY))
            .on_mouse_down(
                MouseButton::Left,
                move |_: &MouseDownEvent, _window, _cx| {
                    let _ = open::that(&url_clone);
                },
            )
            .child(SharedString::from(url))
            .into_any_element()
    } else {
        div()
            .font_weight(if span.bold { FontWeight(700.0) } else { FontWeight(480.0) })
            .when(span.italic, |el| el.italic())
            .when(span.code, |el| {
                el.font_family(AURORA_FONT_MONO)
                    .text_size(px(11.0))
                    .text_color(rgb(AURORA_ACCENT_STRONG))
                    .bg(rgb(AURORA_CONTROL_SURFACE))
                    .rounded_sm()
                    .px_1()
            })
            .text_size(px(13.0))
            .text_color(rgb(AURORA_OUTPUT_TEXT))
            .child(SharedString::from(span.text))
            .into_any_element()
    }
}
