use super::*;

#[test]
fn markdown_document_precomputes_blocks_once() {
    let document = MarkdownDocument::parse(
        "\
# Heading

Body with `code`.

- one
- two

```text
sample
```",
    );

    assert_eq!(document.block_count(), 5);
}

#[test]
fn markdown_document_preserves_empty_input_as_empty_blocks() {
    let document = MarkdownDocument::parse("");

    assert_eq!(document.block_count(), 0);
}
