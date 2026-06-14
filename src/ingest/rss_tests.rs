use super::*;

fn parse(xml: &str) -> Feed {
    feed_rs::parser::parse(xml.as_bytes()).expect("sample feed parses")
}

const RSS_TWO_ITEMS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Example Feed</title>
  <item>
    <title>First Post</title>
    <link>https://example.com/a</link>
    <description>Hello &lt;b&gt;world&lt;/b&gt;</description>
  </item>
  <item>
    <title>Second Post</title>
    <link>https://example.com/b</link>
    <description>Body two</description>
  </item>
</channel></rss>"#;

const ATOM_ONE_ENTRY: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Atom Example</title>
  <entry>
    <title>Atom Post</title>
    <link href="https://example.com/x"/>
    <content type="html">&lt;p&gt;Hi there&lt;/p&gt;</content>
  </entry>
</feed>"#;

#[test]
fn rss_items_become_docs() {
    let feed = parse(RSS_TWO_ITEMS);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Example Feed"), &feed);
    assert_eq!(docs.len(), 2);
}

#[test]
fn atom_entry_becomes_doc() {
    let feed = parse(ATOM_ONE_ENTRY);
    let docs = prepare_feed_docs("https://example.com/atom.xml", Some("Atom Example"), &feed);
    assert_eq!(docs.len(), 1);
}

#[test]
fn entry_without_link_or_content_is_skipped() {
    // An item with neither a link nor a guid URL nor body content cannot be
    // embedded (no canonical URL) and must be dropped.
    let xml = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Sparse</title>
  <item><title>Titleless link</title></item>
</channel></rss>"#;
    let feed = parse(xml);
    let docs = prepare_feed_docs("https://example.com/feed", None, &feed);
    assert!(docs.is_empty());
}

#[test]
fn entry_link_prefers_alternate() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Rel test</title>
  <entry>
    <title>Multi link</title>
    <link rel="edit" href="https://example.com/edit"/>
    <link rel="alternate" href="https://example.com/canonical"/>
    <content type="html">body</content>
  </entry>
</feed>"#;
    let feed = parse(xml);
    let entry = &feed.entries[0];
    assert_eq!(
        entry_link(entry).as_deref(),
        Some("https://example.com/canonical")
    );
}

#[test]
fn entry_link_falls_back_to_url_id() {
    let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <title>Id fallback</title>
  <entry>
    <id>https://example.com/by-id</id>
    <title>Only id</title>
    <content type="html">body</content>
  </entry>
</feed>"#;
    let feed = parse(xml);
    let entry = &feed.entries[0];
    assert_eq!(
        entry_link(entry).as_deref(),
        Some("https://example.com/by-id")
    );
}
