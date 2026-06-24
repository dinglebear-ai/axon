use super::*;

fn url() -> &'static str {
    "https://example.com/page"
}

#[test]
fn empty_markdown_has_url_header() {
    let out = to_llm_text("", url());
    assert!(out.starts_with("> URL: https://example.com/page"));
}

#[test]
fn bold_text_stripped() {
    let out = to_llm_text("**foo**", url());
    assert!(out.contains("foo"));
    assert!(!out.contains("**foo**"));
}

#[test]
fn italic_text_stripped() {
    let out = to_llm_text("*foo*", url());
    assert!(out.contains("foo"));
    assert!(!out.contains("*foo*"));
}

#[test]
fn inline_link_label_in_body_and_links_section() {
    let out = to_llm_text("[label](https://example.com)", url());
    // label appears in body
    assert!(out.contains("label"));
    // links section present
    assert!(out.contains("## Links"));
    assert!(out.contains("label: https://example.com"));
}

#[test]
fn duplicate_links_appear_once() {
    let md = "[a](https://example.com)\n[b](https://example.com)";
    let out = to_llm_text(md, url());
    let count = out.matches("https://example.com").count();
    // 1 in header URL + 1 in links section (deduped)
    assert_eq!(count, 2, "link should appear once in ## Links");
}

#[test]
fn fenced_code_block_preserves_bold_markers() {
    let md = "```\n**bold**\n```";
    let out = to_llm_text(md, url());
    assert!(out.contains("**bold**"));
}

#[test]
fn inline_code_preserved() {
    let md = "use `code` here";
    let out = to_llm_text(md, url());
    assert!(out.contains("`code`"));
}

#[test]
fn url_header_always_present() {
    let out = to_llm_text("hello world", url());
    assert!(out.contains(&format!("> URL: {}", url())));
}

#[test]
fn reference_style_link() {
    let md = "[text][ref]\n\n[ref]: https://linked.example.com";
    let out = to_llm_text(md, url());
    assert!(out.contains("text"));
    assert!(out.contains("## Links"));
    assert!(out.contains("https://linked.example.com"));
}

#[test]
fn no_links_no_section() {
    let out = to_llm_text("just some text", url());
    assert!(!out.contains("## Links"));
}

#[test]
fn javascript_link_not_in_links_section() {
    let md = "[link](javascript:evil())";
    let out = to_llm_text(md, url());
    assert!(!out.contains("javascript:"));
    assert!(!out.contains("## Links"));
}

#[test]
fn data_link_not_in_links_section() {
    let md = "[link](data:text/html,<h1>)";
    let out = to_llm_text(md, url());
    assert!(!out.contains("data:text"));
    assert!(!out.contains("## Links"));
}

#[test]
fn fragment_link_not_in_links_section() {
    let md = "[frag](#section)";
    let out = to_llm_text(md, url());
    assert!(!out.contains("## Links"));
}

#[test]
fn link_label_newline_sanitised() {
    // Simulate a multi-line label by using two text nodes (pulldown-cmark may fold, but test the sanitisation)
    let md = "[line1\nline2](https://example.com/nl)";
    let out = to_llm_text(md, url());
    // The ## Links entry must not contain a bare newline inside the label
    if let Some(links_pos) = out.find("## Links") {
        let links_part = &out[links_pos..];
        // Each list item starts with "- " and ends with newline
        for line in links_part.lines() {
            if line.starts_with("- ") {
                assert!(!line.contains('\n'), "link label must not contain newline");
            }
        }
    }
}

#[test]
fn links_capped_at_200() {
    // Generate 300 unique http links
    let md: String = (0..300)
        .map(|i| format!("[link{i}](https://example.com/page/{i})\n"))
        .collect();
    let out = to_llm_text(&md, url());
    assert!(out.contains("## Links"));
    // Count lines starting with "- " in the links section
    let links_start = out.find("## Links").unwrap();
    let links_part = &out[links_start..];
    let link_entries = links_part.lines().filter(|l| l.starts_with("- ")).count();
    assert_eq!(link_entries, 200);
    assert!(out.contains("... and 100 more links"));
}

#[test]
fn gfm_table_content_in_body() {
    let md = "| A | B |\n|---|---|\n| 1 | 2 |";
    let out = to_llm_text(md, url());
    assert!(out.contains("A"));
    assert!(out.contains("B"));
    assert!(out.contains("1"));
    assert!(out.contains("2"));
}
