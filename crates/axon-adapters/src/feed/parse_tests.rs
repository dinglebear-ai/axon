use super::*;

fn parse(xml: &str) -> Feed {
    parse_feed_bytes(xml.as_bytes()).expect("sample feed parses")
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

const RSS_WITH_DUPLICATE_LINKS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Duplicate Links</title>
  <item>
    <title>First Copy</title>
    <link>https://example.com/a</link>
    <description>Body one</description>
    <guid>guid-1</guid>
  </item>
  <item>
    <title>Second Copy</title>
    <link>https://example.com/a</link>
    <description>Body two</description>
    <guid>guid-2</guid>
  </item>
</channel></rss>"#;

const RSS_WITH_TRACKING_PARAM_VARIANTS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Tracking Links</title>
  <item>
    <title>Clean Link</title>
    <link>https://example.com/a</link>
    <description>Body one</description>
  </item>
  <item>
    <title>Tracked Link</title>
    <link>https://example.com/a?utm_source=newsletter&amp;utm_medium=email&amp;gclid=abc</link>
    <description>Body two</description>
  </item>
</channel></rss>"#;

const RSS_WITH_LINK_AND_GUID: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Link Guid</title>
  <item>
    <title>Linked Post</title>
    <link>https://example.com/a</link>
    <guid>guid-1</guid>
    <description>Body one</description>
  </item>
</channel></rss>"#;

const EMPTY_RSS: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Empty</title>
</channel></rss>"#;

#[test]
fn rss_items_become_entries() {
    let feed = parse(RSS_TWO_ITEMS);
    let entries = extract_entries(&feed);
    assert_eq!(entries.len(), 2);
}

#[test]
fn atom_entry_becomes_entry() {
    let feed = parse(ATOM_ONE_ENTRY);
    let entries = extract_entries(&feed);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].link, "https://example.com/x");
    assert_eq!(entries[0].body_html, "<p>Hi there</p>");
}

#[test]
fn empty_feed_has_zero_entries() {
    let feed = parse(EMPTY_RSS);
    let entries = extract_entries(&feed);
    assert!(entries.is_empty());
}

#[test]
fn duplicate_entry_links_collapse() {
    let feed = parse(RSS_WITH_DUPLICATE_LINKS);
    let entries = extract_entries(&feed);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].link, "https://example.com/a");
}

#[test]
fn duplicate_entry_links_use_normalized_tracking_identity() {
    let feed = parse(RSS_WITH_TRACKING_PARAM_VARIANTS);
    let entries = extract_entries(&feed);
    assert_eq!(entries.len(), 1);
}

#[test]
fn entry_identity_prefers_link_over_mutable_guid() {
    let feed = parse(RSS_WITH_LINK_AND_GUID);
    let entries = extract_entries(&feed);
    assert_eq!(entries[0].link, "https://example.com/a");
    assert_eq!(entries[0].entry_id, "guid-1");
}

#[test]
fn entry_without_link_or_content_is_skipped() {
    let xml = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Sparse</title>
  <item><title>Titleless link</title></item>
</channel></rss>"#;
    let feed = parse(xml);
    let entries = extract_entries(&feed);
    assert!(entries.is_empty());
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

#[test]
fn malformed_xml_returns_error_not_panic() {
    let xml = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Broken</title>
  <item>
    <title>Truncated"#;
    let result = parse_feed_bytes(xml.as_bytes());
    assert!(result.is_err());
}

#[test]
fn json_feed_entries_become_entries() {
    let json = r#"{
        "version": "https://jsonfeed.org/version/1.1",
        "title": "JSON Example",
        "items": [
            {
                "id": "1",
                "url": "https://example.com/json-a",
                "title": "JSON Post",
                "content_html": "<p>Body</p>"
            }
        ]
    }"#;
    let feed = parse(json);
    let entries = extract_entries(&feed);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].link, "https://example.com/json-a");
}
