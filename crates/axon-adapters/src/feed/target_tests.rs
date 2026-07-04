use super::*;

#[test]
fn html_to_text_strips_tags_and_decodes_entities() {
    let text = html_to_text("<p>Hello &amp; welcome, &lt;friend&gt;</p>");
    assert_eq!(text, "Hello & welcome, <friend>");
}

#[test]
fn html_to_text_collapses_whitespace() {
    let text = html_to_text("<div>\n  Line one\n\n  Line two  </div>");
    assert_eq!(text, "Line one Line two");
}

#[test]
fn html_to_text_handles_empty_input() {
    assert_eq!(html_to_text(""), "");
}
