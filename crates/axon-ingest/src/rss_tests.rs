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

const RSS_WITH_TRACKED_LINK_FIRST: &str = r#"<?xml version="1.0"?>
<rss version="2.0"><channel>
  <title>Tracking First</title>
  <item>
    <title>Tracked Link</title>
    <link>https://example.com/a?utm_source=newsletter&amp;gclid=abc</link>
    <description>Body one</description>
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
fn duplicate_entry_links_collapse_before_embedding() {
    let feed = parse(RSS_WITH_DUPLICATE_LINKS);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Feed"), &feed);
    let urls: Vec<_> = docs.iter().map(|doc| doc.url()).collect();
    assert_eq!(urls, vec!["https://example.com/a"]);
}

#[test]
fn duplicate_entry_links_use_normalized_tracking_url_identity() {
    let feed = parse(RSS_WITH_TRACKING_PARAM_VARIANTS);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Feed"), &feed);
    assert_eq!(docs.len(), 1);
    assert_eq!(docs[0].url(), "https://example.com/a");
}

#[test]
fn feed_entry_doc_url_is_canonicalized_for_vector_identity() {
    let feed = parse(RSS_WITH_TRACKED_LINK_FIRST);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Feed"), &feed);
    assert_eq!(docs[0].url(), "https://example.com/a");
    assert_eq!(
        docs[0].extra().unwrap()["entry_link"],
        "https://example.com/a?utm_source=newsletter&gclid=abc"
    );
}

#[test]
fn entry_identity_prefers_link_over_mutable_guid() {
    let feed = parse(RSS_WITH_LINK_AND_GUID);
    let docs = prepare_feed_docs("https://example.com/feed.xml", Some("Feed"), &feed);
    assert_eq!(docs[0].url(), "https://example.com/a");
    assert_eq!(docs[0].extra().unwrap()["entry_id"], "guid-1");
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
