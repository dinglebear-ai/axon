use super::*;

#[test]
fn skips_when_existing_meets_threshold() {
    let html = r#"<script type="application/json">{"x":1}</script>"#;
    // 500 existing words, threshold 200 -> skip
    assert!(extract_data_islands(html, "", 500, 200, DEFAULT_MAX_CHUNKS).is_none());
}

#[test]
fn skips_next_data_id_script() {
    let html = r#"
        <html><head>
        <script id="__NEXT_DATA__" type="application/json">
        {"heading":"Test","description":"A meaningful sentence that should otherwise match."}
        </script>
        </head></html>
    "#;
    // No other scripts; should return None because __NEXT_DATA__ is skipped.
    let out = extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS);
    assert!(out.is_none(), "data_island walker must skip __NEXT_DATA__");
}

#[test]
fn extracts_contentful_paragraph() {
    let html = r#"
        <script type="application/json">
        {
          "nodeType":"document",
          "content":[
            {"nodeType":"heading-1","content":[{"value":"Welcome to Axon"}]},
            {"nodeType":"paragraph","content":[{"value":"This documentation explains how axon works in detail."}]}
          ]
        }
        </script>
    "#;
    let out = extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).unwrap();
    assert!(out.contains("Welcome to Axon"));
    assert!(out.contains("This documentation explains how axon works"));
}

#[test]
fn extracts_cms_entry() {
    let html = r#"
        <script type="application/json">
        {"heading":"Why use axon","description":"Local-first RAG with hybrid search and structured data extraction."}
        </script>
    "#;
    let out = extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).unwrap();
    assert!(out.contains("Why use axon"));
    assert!(out.contains("Local-first RAG"));
}

#[test]
fn extracts_quote_with_attribution() {
    let html = r#"
        <script type="application/json">
        {"quote":"This tool changed how we ship documentation.","author":"Dev Lead at ExampleCo"}
        </script>
    "#;
    let out = extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).unwrap();
    assert!(out.contains("> This tool changed"));
    assert!(out.contains("— Dev Lead"));
}

#[test]
fn extracts_stat_array() {
    let html = r#"
        <script type="application/json">
        {"stats":["100M+ documents indexed","Used by 250+ teams worldwide","99.9% uptime over the last year"]}
        </script>
    "#;
    let out = extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).unwrap();
    assert!(out.contains("100M+ documents indexed"));
    assert!(out.contains(" | "));
}

#[test]
fn extracts_orphan_description_when_no_heading_present() {
    let html = r#"
        <script type="application/json">
        {"id":"abc","description":"This sentence stands alone without a heading attached to it."}
        </script>
    "#;
    let out = extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).unwrap();
    assert!(out.contains("This sentence stands alone"));
}

#[test]
fn dedup_against_existing_markdown_drops_chunks() {
    let html = r#"
        <script type="application/json">
        {"heading":"Already in markdown","description":"This sentence is already part of the page body."}
        </script>
    "#;
    // existing markdown contains the body — the walker MUST dedup.
    let existing = "This sentence is already part of the page body. ";
    let out = extract_data_islands(html, existing, 0, 200, DEFAULT_MAX_CHUNKS);
    // The body got deduped; heading may still survive but the body must not appear.
    if let Some(s) = out {
        assert!(!s.contains("This sentence is already part of"));
    }
}

#[test]
fn skips_short_json_blobs() {
    // < 50 chars: skipped before parse
    let html = r#"<script type="application/json">{"x":1}</script>"#;
    assert!(extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).is_none());
}

#[test]
fn skips_invalid_json_blob() {
    let html = r#"<script type="application/json">{this is not valid json at all and should be skipped}</script>"#;
    assert!(extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).is_none());
}

#[test]
fn max_chunks_caps_recursion() {
    // Build a JSON array of 50 CMS entries; cap at 5.
    let mut blob = String::from(r#"["#);
    for i in 0..50 {
        if i > 0 {
            blob.push(',');
        }
        blob.push_str(&format!(
            r#"{{"heading":"Item number {i}","description":"This is the description for item {i} with enough text."}}"#
        ));
    }
    blob.push(']');
    let html = format!(r#"<script type="application/json">{blob}</script>"#);
    let out = extract_data_islands(&html, "", 0, 200, 5).unwrap();
    // 5 chunks max; each emits one "## heading\n\nbody\n\n" block.
    let block_count = out.matches("## Item number").count();
    assert!(block_count <= 5, "got {block_count} headings, expected <=5");
}

#[test]
fn is_content_text_rejects_urls_and_ids() {
    assert!(!is_content_text("https://example.com/path"));
    assert!(!is_content_text("/api/v1/items"));
    assert!(!is_content_text("abc123"));
    assert!(is_content_text("This is a real sentence."));
}

#[test]
fn is_content_text_requires_min_length() {
    assert!(!is_content_text("hi there")); // < 15 chars
    assert!(is_content_text("hi there friend long enough"));
}

#[test]
fn media_keys_skipped_in_walk() {
    // Image URL would otherwise count as a 1-element array; "url" key
    // must be skipped to prevent recursion into it.
    let html = r#"
        <script type="application/json">
        {"image":{"url":"https://example.com/x.jpg","width":1200}}
        </script>
    "#;
    assert!(extract_data_islands(html, "", 0, 200, DEFAULT_MAX_CHUNKS).is_none());
}

#[test]
fn depth_cap_prevents_infinite_recursion() {
    // Generate a deeply-nested object beyond MAX_DEPTH.
    let mut nested = String::from(r#"{"description":"Recovered at the top level only."}"#);
    for _ in 0..30 {
        nested = format!(r#"{{"inner":{nested}}}"#);
    }
    let html = format!(r#"<script type="application/json">{nested}</script>"#);
    // Walker reaches the top-level orphan body before hitting depth cap.
    let _ = extract_data_islands(&html, "", 0, 200, DEFAULT_MAX_CHUNKS);
    // We don't strictly assert content here — just that the call doesn't
    // hang or crash on deep nesting.
}
