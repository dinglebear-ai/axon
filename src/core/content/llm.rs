use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashSet;

const PARSER_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_FOOTNOTES)
    .union(Options::ENABLE_TASKLISTS);
const LINKS_MAX: usize = 200;
// Cap dedup tracking to avoid unbounded HashSet growth on pathological inputs
const SEEN_URLS_MAX: usize = LINKS_MAX * 4;

/// Convert markdown to LLM-optimised plain text.
///
/// Strips emphasis/images, replaces inline links with labels, appends a
/// deduplicated `## Links` section, and prepends a URL metadata header.
pub fn to_llm_text(markdown: &str, url: &str) -> String {
    // Strip newlines from url to prevent header injection (defense-in-depth; callers
    // also SSRF-validate, but that does not guarantee no embedded newlines).
    let safe_url = url.replace(['\n', '\r'], "%0A");
    let header = format!("> URL: {safe_url}\n\n");
    let mut body = String::with_capacity(markdown.len() + 256);
    let mut links: Vec<(String, String)> = Vec::with_capacity(64);
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut in_link = false;
    let mut link_url = String::new();
    let mut link_label = String::new();
    let mut in_image = false;
    let mut total_unique: usize = 0;

    for event in Parser::new_ext(markdown, PARSER_OPTIONS) {
        match event {
            Event::Start(Tag::Emphasis | Tag::Strong)
            | Event::End(TagEnd::Emphasis | TagEnd::Strong) => {}
            Event::Start(Tag::Image { .. }) => in_image = true,
            Event::End(TagEnd::Image) => in_image = false,
            Event::Start(Tag::Link { dest_url, .. }) => {
                let d = dest_url.as_ref();
                if d.starts_with("http://") || d.starts_with("https://") {
                    in_link = true;
                    link_url = d.to_string();
                    link_label.clear();
                }
            }
            Event::End(TagEnd::Link) => {
                if in_link {
                    let label = link_label.trim().to_string();
                    body.push_str(&label);
                    if seen_urls.len() < SEEN_URLS_MAX {
                        if seen_urls.insert(link_url.clone()) {
                            total_unique += 1;
                            if links.len() < LINKS_MAX {
                                links.push((label, link_url.clone()));
                            }
                        }
                    } else {
                        // Past dedup horizon — count for overflow display only
                        total_unique += 1;
                    }
                    in_link = false;
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                body.push('\n');
                for _ in 0..level as u8 {
                    body.push('#');
                }
                body.push(' ');
            }
            Event::End(TagEnd::Heading(_)) => body.push('\n'),
            Event::Start(Tag::CodeBlock(_)) => body.push_str("```\n"),
            Event::End(TagEnd::CodeBlock) => body.push_str("```\n"),
            // Table/BlockQuote structural events: text content still emitted via Event::Text
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::Code(span) => body.push_str(&format!("`{span}`")),
            Event::Text(s) => {
                if in_link {
                    link_label.push_str(&s);
                } else if !in_image {
                    body.push_str(&s);
                }
            }
            Event::SoftBreak => body.push(' '),
            Event::HardBreak => body.push('\n'),
            Event::Rule => body.push_str("\n---\n"),
            _ => {}
        }
    }

    let body_trimmed = body.trim();
    let overflow = total_unique.saturating_sub(links.len());
    let links_section = if links.is_empty() {
        String::new()
    } else {
        let mut s = "\n\n## Links\n".to_string();
        for (label, dest) in &links {
            s.push_str(&format!("- {}: {dest}\n", label.replace(['\n', '\r'], " ")));
        }
        if overflow > 0 {
            s.push_str(&format!("... and {overflow} more links\n"));
        }
        s
    };

    format!("{header}{body_trimmed}{links_section}")
}

#[cfg(test)]
#[path = "llm_tests.rs"]
mod tests;
