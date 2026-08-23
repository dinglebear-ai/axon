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
fn markdown_sections_does_not_pack_incompatible_heading_paths() {
    let text = "# A\nsmall\n## B\nsmall too\n## C\nthis section stays separate\n";
    let chunks = markdown_sections_with_limits(
        text,
        MarkdownChunkLimits {
            max_chars: 40,
            min_chars: 20,
            overlap_chars: 0,
        },
    );

    assert_eq!(chunks.len(), 3);
    assert!(chunks[0].content.contains("# A"));
    assert!(!chunks[0].content.contains("## B"));
    assert_eq!(chunks[0].range.byte_start, Some(0));
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
            overlap_chars: 0,
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
fn oversized_markdown_prefers_paragraph_boundaries() {
    let first = "alpha beta gamma delta epsilon zeta eta theta";
    let second = "second paragraph keeps enough words to cross the configured character limit";
    let text = format!("# Paragraphs\n{first}\n\n{second}\n");
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 72,
            min_chars: 1,
            overlap_chars: 0,
        },
    );

    assert!(chunks.len() >= 2);
    assert_eq!(
        chunks[0].content,
        format!("# Paragraphs\n{first}"),
        "the first window should stop at the blank-line paragraph boundary"
    );
    assert!(chunks[1].content.starts_with("second paragraph"));
}

#[test]
fn fitting_markdown_list_is_not_cut_when_prefix_fills_the_window() {
    let list = "- alpha item has useful detail\n- beta item has useful detail\n- gamma item has useful detail";
    let text = format!(
        "# Items\n{}\n\n{list}\n",
        "introductory prose ".repeat(3).trim_end()
    );
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: list.chars().count(),
            min_chars: 1,
            overlap_chars: 8,
        },
    );

    let list_chunks = chunks
        .iter()
        .filter(|chunk| chunk.content.contains("- alpha item"))
        .collect::<Vec<_>>();
    assert_eq!(list_chunks.len(), 1);
    assert_eq!(list_chunks[0].content, list);
    assert!(list_chunks[0].content.contains("- gamma item"));
}

#[test]
fn fitting_markdown_table_is_not_cut_when_prefix_fills_the_window() {
    let table = "| Name | Value |\n| --- | --- |\n| alpha | one |\n| beta | two |";
    let text = format!(
        "# Metrics\n{}\n\n{table}\n",
        "introductory prose ".repeat(3).trim_end()
    );
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: table.chars().count(),
            min_chars: 1,
            overlap_chars: 8,
        },
    );

    let table_chunks = chunks
        .iter()
        .filter(|chunk| chunk.content.contains("| Name | Value |"))
        .collect::<Vec<_>>();
    assert_eq!(table_chunks.len(), 1);
    assert_eq!(table_chunks[0].content, table);
    assert!(table_chunks[0].content.contains("| beta | two |"));
}

#[test]
fn markdown_packing_keeps_frontmatter_and_sibling_headings_separate() {
    let text = "---\ntitle: Doc\n---\n# A\nshort\n## B\nalso short\n";
    let chunks = markdown_sections_with_limits(
        text,
        MarkdownChunkLimits {
            max_chars: 200,
            min_chars: 200,
            overlap_chars: 0,
        },
    );

    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].metadata["markdown_block_kind"], "frontmatter");
    assert_eq!(chunks[1].heading_path, vec!["A"]);
    assert_eq!(chunks[2].heading_path, vec!["A", "B"]);
}

#[test]
fn oversized_frontmatter_remains_one_frontmatter_chunk() {
    let text = format!(
        "---\ndescription: {}\n---\n# Body\ntext\n",
        "metadata ".repeat(40)
    );
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 64,
            min_chars: 1,
            overlap_chars: 8,
        },
    );

    assert_eq!(chunks[0].metadata["markdown_block_kind"], "frontmatter");
    assert!(chunks[0].content.chars().count() > 64);
    assert!(chunks[0].content.starts_with("---\n"));
    assert!(chunks[0].content.ends_with("\n---"));
    assert_eq!(
        chunks
            .iter()
            .filter(|chunk| chunk.metadata["markdown_block_kind"] == "frontmatter")
            .count(),
        1
    );
}

#[test]
fn oversized_markdown_keeps_fence_whole_and_bounds_surrounding_prose() {
    let fence_body = "let value = 42;\n".repeat(12);
    let text = format!(
        "# Mixed\n{}\n```rust\n{fence_body}```\n{}\n",
        "before prose ".repeat(12),
        "after prose ".repeat(12),
    );
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 64,
            min_chars: 1,
            overlap_chars: 8,
        },
    );

    let code = chunks
        .iter()
        .filter(|chunk| chunk.metadata["markdown_block_kind"] == "code")
        .collect::<Vec<_>>();
    assert_eq!(code.len(), 1, "one fence must remain one chunk");
    assert!(code[0].content.starts_with("```rust\n"));
    assert!(code[0].content.ends_with("```"));
    assert!(code[0].content.chars().count() > 64);
    assert!(
        chunks
            .iter()
            .filter(|chunk| chunk.metadata["markdown_block_kind"] != "code")
            .all(|chunk| chunk.content.chars().count() <= 64)
    );
}

#[test]
fn consecutive_oversized_fences_ignore_blank_prose_between_them() {
    let first = "first();\n".repeat(20);
    let second = "second();\n".repeat(20);
    let text = format!("# Fences\n```rust\n{first}```\n\n```rust\n{second}```\n");
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 64,
            min_chars: 1,
            overlap_chars: 0,
        },
    );

    assert_eq!(
        chunks
            .iter()
            .filter(|chunk| chunk.metadata["markdown_block_kind"] == "code")
            .count(),
        2
    );
    assert!(chunks.iter().all(|chunk| !chunk.content.trim().is_empty()));
}

#[test]
fn whitespace_only_windows_are_skipped_without_losing_surrounding_prose() {
    let text = format!("# Spacing\nalpha{}omega\n", " ".repeat(160));
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 16,
            min_chars: 1,
            overlap_chars: 0,
        },
    );

    assert!(chunks.iter().all(|chunk| !chunk.content.trim().is_empty()));
    let compact_chunks = chunks
        .iter()
        .flat_map(|chunk| chunk.content.chars())
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let compact_source = text
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    assert_eq!(compact_chunks, compact_source);
}

#[test]
fn mismatched_fence_marker_does_not_expose_internal_headings() {
    let text = "# Outer\n~~~rust\n```\n# not a heading\n~~~\n## Real\nbody\n";
    let chunks = markdown_sections(text);
    let titles = chunks
        .iter()
        .filter_map(|chunk| chunk.title.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(titles, vec!["Outer", "Real"]);
    assert!(chunks[0].content.contains("# not a heading"));
}

#[test]
fn shorter_fence_run_does_not_expose_internal_headings() {
    let text = "# Outer\n````rust\n```\n# not a heading\n````\n## Real\nbody\n";
    let chunks = markdown_sections(text);
    let titles = chunks
        .iter()
        .filter_map(|chunk| chunk.title.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(titles, vec!["Outer", "Real"]);
    assert!(chunks[0].content.contains("# not a heading"));
}

#[test]
fn prose_windows_apply_overlap_without_breaking_utf8_boundaries() {
    let text = format!("# Unicode\n{}", "éclair ".repeat(80));
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 48,
            min_chars: 1,
            overlap_chars: 7,
        },
    );

    assert!(chunks.len() > 2);
    for chunk in &chunks {
        assert!(chunk.content.chars().count() <= 48);
        let start = chunk.range.byte_start.unwrap() as usize;
        let end = chunk.range.byte_end.unwrap() as usize;
        assert!(text.is_char_boundary(start));
        assert!(text.is_char_boundary(end));
    }
    for pair in chunks.windows(2) {
        let previous_end = pair[0].range.byte_end.unwrap();
        let next_start = pair[1].range.byte_start.unwrap();
        assert!(
            next_start < previous_end,
            "configured prose overlap must be represented in ranges"
        );
    }
}

#[test]
fn markdown_window_count_grows_linearly_at_large_boundaries() {
    let text = format!("# Large\n{}", "0123456789abcdef\n".repeat(20_000));
    let chunks = markdown_sections_with_limits(
        &text,
        MarkdownChunkLimits {
            max_chars: 512,
            min_chars: 1,
            overlap_chars: 32,
        },
    );

    assert!(chunks.len() > 500);
    assert!(chunks.len() < 1_000);
    assert!(
        chunks
            .iter()
            .all(|chunk| chunk.content.chars().count() <= 512)
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
