use super::*;

#[test]
fn markdown_sections_does_not_split_inside_a_fenced_code_block() {
    let text = "# Title\n\n```\n# not a heading\n## also not\n```\n\n## Real Heading\nbody\n";
    let chunks = markdown_sections(text);

    let titled: Vec<&str> = chunks.iter().filter_map(|c| c.title.as_deref()).collect();
    assert_eq!(titled, vec!["Title", "Real Heading"]);
    assert!(chunks.iter().any(|c| c.content.contains("# not a heading")));
}

#[test]
fn markdown_sections_carries_full_heading_breadcrumb() {
    let text = "# A\n## B\n### C\nleaf content\n";
    let chunks = markdown_sections(text);

    let leaf = chunks.last().unwrap();
    assert_eq!(leaf.heading_path, vec!["A", "B", "C"]);
}

#[test]
fn markdown_sections_pops_breadcrumb_on_sibling_heading() {
    let text = "# A\n## B\ntext\n## C\nmore\n";
    let chunks = markdown_sections(text);

    let c_section = chunks
        .iter()
        .find(|c| c.title.as_deref() == Some("C"))
        .unwrap();
    assert_eq!(c_section.heading_path, vec!["A", "C"]);
}

#[test]
fn markdown_sections_extracts_frontmatter_as_its_own_chunk() {
    let text = "---\ntitle: Doc\n---\n# Heading\nbody\n";
    let chunks = markdown_sections(text);

    assert_eq!(
        chunks[0].metadata.get("markdown_block_kind").unwrap(),
        "frontmatter"
    );
    assert!(chunks[0].content.contains("title: Doc"));
    assert_eq!(chunks[1].title.as_deref(), Some("Heading"));
}

#[test]
fn markdown_sections_stamps_code_fence_language() {
    let text = "## Snippet\n```rust\nfn main() {}\n```\n";
    let chunks = markdown_sections(text);

    assert_eq!(
        chunks[0].metadata.get("code_fence_language").unwrap(),
        "rust"
    );
}

#[test]
fn markdown_sections_packs_adjacent_small_sections_within_configured_limits() {
    let text = "# A\nsmall\n## B\nsmall too\n## C\nthis section stays separate\n";
    let chunks = markdown_sections_with_limits(
        text,
        MarkdownChunkLimits {
            max_chars: 40,
            min_chars: 20,
        },
    );

    assert_eq!(chunks.len(), 2);
    assert!(chunks[0].content.contains("# A"));
    assert!(chunks[0].content.contains("## B"));
    assert_eq!(chunks[0].range.byte_start, Some(0));
    assert_eq!(chunks[0].range.byte_end, Some(25));
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 40)
    );
}

#[test]
fn markdown_sections_splits_oversized_sections_at_the_configured_max() {
    let body = (0..40)
        .map(|index| format!("row {index}: {}", "value ".repeat(8)))
        .collect::<Vec<_>>()
        .join("\n");
    let text = format!("# Large table\n{body}\n");
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 160,
            min_chars: 40,
        },
    );

    assert!(chunks.len() > 1);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 160)
    );
    assert_eq!(chunks.first().unwrap().range.byte_start, Some(0));
    assert_eq!(
        chunks.last().unwrap().range.byte_end,
        Some(text.trim_end().len() as u64)
    );
}

#[test]
fn html_article_excludes_non_content_payloads_before_chunking() {
    let hydration =
        "window.__next_f.push(['<script data-template>', 'secret-looking-auth-token']);"
            .repeat(2_000);
    let html = format!(
        r#"<!doctype html>
        <html>
          <head>
            <style>.hidden {{ display: none }}</style>
            <script>{hydration}</script>
          </head>
          <body>
            <nav>Documentation navigation</nav>
            <main><h1>Authorization</h1><p>Use Bearer authentication for protected requests.</p></main>
            <footer>Site footer</footer>
          </body>
        </html>"#
    );

    let chunks = html_article(&html);
    let content = chunks
        .iter()
        .map(|chunk| chunk.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");

    assert!(content.contains("Authorization"));
    assert!(content.contains("Use Bearer authentication"));
    assert!(content.contains("Site footer"));
    assert!(!content.contains("window.__next_f"));
    assert!(!content.contains("secret-looking-auth-token"));
    assert!(!content.contains("display: none"));
    assert!(chunks.len() <= 10, "hydration data must not amplify chunks");
}

#[test]
fn html_article_does_not_emit_one_chunk_per_dom_node() {
    let nodes = (0..500)
        .map(|index| format!("<span>word-{index}</span>"))
        .collect::<String>();
    let chunks = html_article(&format!("<main>{nodes}</main>"));

    assert!(
        chunks.len() <= 3,
        "DOM nodes must coalesce into text windows"
    );
    assert!(
        chunks
            .iter()
            .any(|chunk| chunk.content.contains("word-499"))
    );
}

#[test]
fn html_article_preserves_content_after_unclosed_non_content_tag() {
    let chunks = html_article("<p>before</p><script>broken markup then visible fallback");
    let text = chunks
        .iter()
        .map(|chunk| chunk.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(text.contains("before"));
    assert!(text.contains("visible fallback"));
}
