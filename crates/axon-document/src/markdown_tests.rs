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
