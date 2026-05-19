use super::*;

#[test]
fn matches_abs_url() {
    assert!(matches("https://arxiv.org/abs/2301.00001"));
    assert!(matches("https://arxiv.org/abs/cs.LG/2301.00001"));
}

#[test]
fn matches_pdf_url() {
    assert!(matches("https://arxiv.org/pdf/2301.00001"));
    assert!(matches("https://arxiv.org/pdf/2301.00001.pdf"));
}

#[test]
fn rejects_other_arxiv_paths() {
    assert!(!matches("https://arxiv.org/"));
    assert!(!matches("https://arxiv.org/search/"));
    assert!(!matches("https://arxiv.org/list/cs.AI/2024"));
}

#[test]
fn rejects_non_arxiv() {
    assert!(!matches("https://example.com/abs/1234.5678"));
}

#[test]
fn extract_arxiv_id_abs() {
    let id = extract_arxiv_id("https://arxiv.org/abs/2301.00001");
    assert_eq!(id.as_deref(), Some("2301.00001"));
}

#[test]
fn extract_arxiv_id_pdf_with_suffix() {
    let id = extract_arxiv_id("https://arxiv.org/pdf/2301.00001.pdf");
    assert_eq!(id.as_deref(), Some("2301.00001"));
}

#[test]
fn extract_tag_simple() {
    let xml = "<title>Test Paper</title><summary>Abstract</summary>";
    assert_eq!(extract_tag(xml, "title").as_deref(), Some("Test Paper"));
    assert_eq!(extract_tag(xml, "summary").as_deref(), Some("Abstract"));
}

#[test]
fn extract_authors_multiple() {
    let xml = r#"<author><name>Alice</name></author><author><name>Bob</name></author>"#;
    let authors = extract_authors(xml);
    assert_eq!(authors, vec!["Alice", "Bob"]);
}

#[test]
fn extract_categories_term() {
    let xml = r#"<category term="cs.LG" scheme="http://arxiv.org/schemas/atom"/><category term="stat.ML" scheme="http://arxiv.org/schemas/atom"/>"#;
    let cats = extract_categories(xml);
    assert!(cats.contains(&"cs.LG".to_string()));
    assert!(cats.contains(&"stat.ML".to_string()));
}

#[test]
fn extract_entry_block_scopes_correctly() {
    let xml = r#"<feed><title>Feed Title</title><entry><title>Paper Title</title><summary>Abstract</summary></entry></feed>"#;
    let entry = extract_entry_block(xml).unwrap();
    assert!(entry.contains("Paper Title"));
    assert!(!entry.contains("Feed Title"));
}

#[test]
fn extract_pdf_url_finds_href() {
    let xml =
        r#"<link title="pdf" type="application/pdf" href="https://arxiv.org/pdf/2301.00001"/>"#;
    let url = extract_pdf_url(xml);
    assert_eq!(url.as_deref(), Some("https://arxiv.org/pdf/2301.00001"));
}
