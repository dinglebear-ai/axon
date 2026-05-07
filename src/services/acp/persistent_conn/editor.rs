/// Parse `<axon:editor>` XML blocks from agent response text.
///
/// Returns a list of `(content, operation)` pairs for each block found.
/// `operation` is either `"replace"` (default) or `"append"`.
///
/// # Format
///
/// ```xml
/// <axon:editor op="replace">
/// # Hello World
/// Content here
/// </axon:editor>
/// ```
pub(crate) fn parse_editor_blocks(text: &str) -> Vec<(String, String)> {
    const OPEN: &str = "<axon:editor";
    const CLOSE: &str = "</axon:editor>";

    let mut blocks = Vec::new();
    let mut remaining = text;

    while let Some(tag_start) = remaining.find(OPEN) {
        remaining = &remaining[tag_start + OPEN.len()..];
        let Some(tag_end) = remaining.find('>') else {
            break;
        };
        let tag_attrs = &remaining[..tag_end];
        remaining = &remaining[tag_end + 1..];

        let operation = if tag_attrs.contains(r#"op="append""#) {
            "append".to_string()
        } else {
            "replace".to_string()
        };

        let Some(content_end) = remaining.find(CLOSE) else {
            break;
        };
        let content = remaining[..content_end].trim().to_string();
        remaining = &remaining[content_end + CLOSE.len()..];

        if !content.is_empty() {
            blocks.push((content, operation));
        }
    }

    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_editor_blocks_replace() {
        let text = r#"Here is some text.
<axon:editor op="replace">
# Hello

World
</axon:editor>
Done."#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "# Hello\n\nWorld");
        assert_eq!(blocks[0].1, "replace");
    }

    #[test]
    fn parse_editor_blocks_append() {
        let text = r#"<axon:editor op="append">## Section
Content</axon:editor>"#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "append");
    }

    #[test]
    fn parse_editor_blocks_multiple() {
        let text = r#"<axon:editor op="replace">First</axon:editor>
<axon:editor op="append">Second</axon:editor>"#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].0, "First");
        assert_eq!(blocks[0].1, "replace");
        assert_eq!(blocks[1].0, "Second");
        assert_eq!(blocks[1].1, "append");
    }

    #[test]
    fn parse_editor_blocks_default_op_is_replace() {
        let text = r#"<axon:editor>content</axon:editor>"#;
        let blocks = parse_editor_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].1, "replace");
    }

    #[test]
    fn parse_editor_blocks_empty_content_skipped() {
        let text = r#"<axon:editor op="replace">   </axon:editor>"#;
        let blocks = parse_editor_blocks(text);
        assert!(blocks.is_empty());
    }

    #[test]
    fn parse_editor_blocks_no_blocks() {
        let blocks = parse_editor_blocks("just some text with no editor blocks");
        assert!(blocks.is_empty());
    }
}
