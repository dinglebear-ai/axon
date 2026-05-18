//! Inline tests ported verbatim from
//! `~/workspace/webclaw/crates/webclaw-core/src/structured_data.rs`
//! plus the bead-specified cap / dominant tests.

use super::*;
use serde_json::Value;

#[test]
fn extracts_single_json_ld() {
    let html = r#"
        <html><head>
        <script type="application/ld+json">{"@type":"Product","name":"Test"}</script>
        </head><body></body></html>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["@type"], "Product");
    assert_eq!(results[0]["name"], "Test");
}

#[test]
fn extracts_multiple_json_ld_blocks() {
    let html = r#"
        <script type="application/ld+json">{"@type":"WebSite","url":"https://example.com"}</script>
        <script type="application/ld+json">{"@type":"Product","name":"Shoe","offers":{"price":99.99}}</script>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0]["@type"], "WebSite");
    assert_eq!(results[1]["@type"], "Product");
}

#[test]
fn handles_array_json_ld() {
    let html = r#"
        <script type="application/ld+json">[{"@type":"BreadcrumbList"},{"@type":"Product"}]</script>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 2);
}

#[test]
fn skips_invalid_json_ld_block() {
    let html = r#"
        <script type="application/ld+json">{invalid json here}</script>
        <script type="application/ld+json">{"@type":"Product","name":"Valid"}</script>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "Valid");
}

#[test]
fn ignores_regular_script_tags() {
    let html = r#"
        <script>console.log("not json-ld")</script>
        <script type="text/javascript">var x = 1;</script>
        <script type="application/ld+json">{"@type":"Product"}</script>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
}

#[test]
fn handles_no_json_ld() {
    let html = "<html><body><p>No structured data here</p></body></html>";
    let results = extract_json_ld(html);
    assert!(results.is_empty());
}

#[test]
fn case_insensitive_type() {
    let html = r#"
        <script type="Application/LD+JSON">{"@type":"Product"}</script>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
}

#[test]
fn handles_whitespace_in_json_ld() {
    let html = r#"
        <script type="application/ld+json">
            {
                "@type": "Product",
                "name": "Test"
            }
        </script>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["name"], "Test");
}

#[test]
fn empty_script_tag_skipped() {
    let html = r#"
        <script type="application/ld+json">   </script>
        <script type="application/ld+json">{"@type":"Product"}</script>
    "#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
}

#[test]
fn handles_raw_newlines_in_json_ld() {
    let html = "<script type=\"application/ld+json\">{\"@type\":\"ProfilePage\",\"mainEntity\":{\"name\":\"Jay\",\"description\":\"Founder @ Bluesky\n\nWorking on stuff\n🌱\"}}</script>";
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["@type"], "ProfilePage");
    let desc = results[0]["mainEntity"]["description"].as_str().unwrap();
    assert!(desc.contains("Founder"));
    assert!(desc.contains("Working on stuff"));
}

#[test]
fn extracts_next_data_page_props() {
    let html = r#"
        <script id="__NEXT_DATA__" type="application/json">
            {"props":{"pageProps":{"title":"Hello","items":[1,2,3]}},"page":"/x","buildId":"abc"}
        </script>
    "#;
    let results = extract_next_data(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["title"], "Hello");
    assert_eq!(results[0]["items"][2], 3);
}

#[test]
fn next_data_app_router_page_returns_empty() {
    let html = r#"
        <script>self.__next_f.push([1, "0:[\"$\",\"main\",null,{}]"])</script>
    "#;
    let results = extract_next_data(html);
    assert!(results.is_empty());
}

#[test]
fn extracts_sveltekit_data() {
    let html = r#"
        <script>
            kit.start(app, document.querySelector('#sv'), {
                data: [null, {"type":"data","data":{"title":"Hello","count":42}}, null]
            });
        </script>
    "#;
    let results = extract_sveltekit(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["title"], "Hello");
    assert_eq!(results[0]["count"], 42);
}

#[test]
fn sanitize_preserves_valid_escapes() {
    let input = "{\"text\":\"line1\\nline2\",\"raw\":\"has\nnewline\"}";
    let sanitized = sanitize_json_newlines(input);
    let parsed: Value = serde_json::from_str(&sanitized).unwrap();
    assert_eq!(parsed["text"], "line1\nline2");
    assert_eq!(parsed["raw"], "has\nnewline");
}

#[test]
fn sanitize_idempotent_on_clean_input() {
    let clean = r#"{"a":1,"b":"hello"}"#;
    assert_eq!(sanitize_json_newlines(clean), clean);
}

#[test]
fn extract_all_bundles_outputs() {
    let html = r#"
        <script type="application/ld+json">{"@type":"Article","headline":"X"}</script>
        <script id="__NEXT_DATA__" type="application/json">
            {"props":{"pageProps":{"q":1}}}
        </script>
    "#;
    let pass = extract_all(html);
    assert_eq!(pass.json_ld.len(), 1);
    assert_eq!(pass.next_data.len(), 1);
    assert!(pass.sveltekit.is_empty());
    assert_eq!(pass.len(), 2);
    assert!(!pass.is_empty());
}

#[test]
fn extract_all_empty_when_no_structured_data() {
    let html = "<html><body><p>nothing structured</p></body></html>";
    let pass = extract_all(html);
    assert!(pass.is_empty());
    assert_eq!(pass.len(), 0);
}

#[test]
fn schema_type_of_extracts_string_type() {
    let v: Value = serde_json::from_str(r#"{"@type":"Product","name":"x"}"#).unwrap();
    assert_eq!(schema_type_of(&v).as_deref(), Some("Product"));
}

#[test]
fn schema_type_of_extracts_first_when_array() {
    let v: Value = serde_json::from_str(r#"{"@type":["Article","TechArticle"]}"#).unwrap();
    assert_eq!(schema_type_of(&v).as_deref(), Some("Article"));
}

#[test]
fn schema_id_of_extracts_when_present() {
    let v: Value =
        serde_json::from_str(r#"{"@id":"https://example.com/p/1","@type":"Product"}"#).unwrap();
    assert_eq!(schema_id_of(&v).as_deref(), Some("https://example.com/p/1"));
}

// ──────────────────────────────────────────────────────────────────────
// Bead-specified additions (xvu9): dominant() preference + 64KB cap +
// adversarial input does-not-panic guard.
// ──────────────────────────────────────────────────────────────────────

#[test]
fn dominant_prefers_jsonld_over_next_data() {
    let pass = StructuredDataPass {
        json_ld: vec![serde_json::json!({"@type": "Article"})],
        next_data: vec![serde_json::json!({"title": "x"})],
        sveltekit: vec![],
    };
    let (kind, _) = pass.dominant().expect("non-empty pass");
    assert_eq!(kind, "jsonld");
}

#[test]
fn dominant_prefers_next_data_over_sveltekit() {
    let pass = StructuredDataPass {
        json_ld: vec![],
        next_data: vec![serde_json::json!({"q": 1})],
        sveltekit: vec![serde_json::json!({"k": 2})],
    };
    let (kind, _) = pass.dominant().expect("non-empty pass");
    assert_eq!(kind, "next_data");
}

#[test]
fn dominant_returns_sveltekit_when_only_source() {
    let pass = StructuredDataPass {
        json_ld: vec![],
        next_data: vec![],
        sveltekit: vec![serde_json::json!({"loaded": true})],
    };
    let (kind, _) = pass.dominant().expect("non-empty pass");
    assert_eq!(kind, "sveltekit");
}

#[test]
fn dominant_returns_none_when_empty() {
    let pass = StructuredDataPass::default();
    assert!(pass.dominant().is_none());
}

#[test]
fn oversized_blob_does_not_panic() {
    // Synthesize an oversized JSON-LD block (~120KB) to ensure the parser
    // accepts it without panicking — the cap is the caller's responsibility,
    // not the extractor's.
    let big_string = "x".repeat(120_000);
    let html = format!(
        "<script type=\"application/ld+json\">{{\"@type\":\"Article\",\"body\":\"{}\"}}</script>",
        big_string
    );
    let results = extract_json_ld(&html);
    assert_eq!(results.len(), 1);
}

#[test]
fn adversarial_unbalanced_brackets_no_panic() {
    let html = r#"<script>kit.start(app, target, { data: [ {{{{ </script>"#;
    let results = extract_sveltekit(html);
    assert!(results.is_empty());
}

#[test]
fn adversarial_truncated_next_data_no_panic() {
    let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{"page"#;
    let results = extract_next_data(html);
    assert!(results.is_empty());
}

#[test]
fn sanitize_handles_unterminated_string_no_panic() {
    let input = r#"{"a":"never closed"#;
    // Should not panic — just round-trip the bytes without changing classification.
    let _ = sanitize_json_newlines(input);
}

#[test]
fn extract_json_ld_skips_oversized_block_pre_parse() {
    // Synthesize a JSON-LD block > MAX_JSON_LD_BLOCK_BYTES (512 KiB).
    // Must be skipped without invoking serde_json::from_str so a hostile
    // page cannot pin a worker on multi-MB parse cost.
    let big_payload = "x".repeat(600 * 1024);
    let html = format!(
        "<script type=\"application/ld+json\">{{\"@type\":\"Article\",\"body\":\"{}\"}}</script>",
        big_payload
    );
    let results = extract_json_ld(&html);
    assert!(results.is_empty());
}

#[test]
fn extract_json_ld_handles_mixed_case_close_tag() {
    // ASCII-case-insensitive byte search must match `</SCRIPT>` exactly
    // like `</script>` — this used to require to_lowercase() on the full
    // suffix.
    let html = r#"<script type="application/ld+json">{"@type":"Article"}</SCRIPT>"#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["@type"], "Article");
}

#[test]
fn extract_json_ld_handles_mixed_case_open_tag() {
    // cubic finding #1 (json_ld.rs:16): the `<script` opener scan was
    // case-sensitive — valid `<SCRIPT type="application/ld+json">` or
    // `<Script ...>` tags were silently skipped. The fix swaps the
    // bytewise `.find()` for `ascii_case_insensitive_find`.
    let html = r#"<SCRIPT type="application/ld+json">{"@type":"Article","name":"Up"}</SCRIPT>"#;
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1, "mixed-case <SCRIPT> opener must match");
    assert_eq!(results[0]["@type"], "Article");
    assert_eq!(results[0]["name"], "Up");

    let mixed = r#"<Script type="application/LD+JSON">{"@type":"Product"}</Script>"#;
    let results = extract_json_ld(mixed);
    assert_eq!(results.len(), 1, "mixed-case <Script> opener must match");
    assert_eq!(results[0]["@type"], "Product");
}

#[test]
fn extract_json_ld_non_ascii_before_script_no_panic() {
    // cubic finding #2 (json_ld.rs:34): the old code called
    // `remaining.to_lowercase()` before computing the close offset, then
    // sliced the original `remaining` with that offset. On non-ASCII
    // input the byte offset is wrong (German `ß` → `ss` shifts every
    // following byte) and the slice can panic at a UTF-8 boundary.
    // The fix searches on bytes and never lowercases the haystack —
    // this test pins the regression by mixing multi-byte text with a
    // valid JSON-LD block.
    let html = "<p>weiß und groß — Δοκιμή 🌱</p>\
                <script type=\"application/ld+json\">{\"@type\":\"Article\",\"headline\":\"Δοκιμή\"}</script>";
    let results = extract_json_ld(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["headline"], "Δοκιμή");
}

#[test]
fn next_data_ignores_decoy_before_real_script() {
    // cubic finding #4 (next_data.rs:17): the original code grabbed the
    // first `__NEXT_DATA__` occurrence anywhere in the document and
    // walked `<script>` boundaries from there — so a comment, a
    // `data-*="__NEXT_DATA__"` attribute on some other tag, or an
    // inline mention earlier in the body could mask the real
    // `<script id="__NEXT_DATA__">` block. The new implementation
    // walks `<script>` tags in order and only accepts one whose
    // opening tag carries `id="__NEXT_DATA__"`.
    let html = r#"
        <!-- mentions __NEXT_DATA__ in a comment -->
        <div data-marker="__NEXT_DATA__">decoy</div>
        <script type="application/json">{"__NEXT_DATA__": "not it"}</script>
        <script id="__NEXT_DATA__" type="application/json">
            {"props":{"pageProps":{"title":"Real","items":[1,2,3]}},"page":"/p","buildId":"abc"}
        </script>
    "#;
    let results = extract_next_data(html);
    assert_eq!(results.len(), 1, "must find the real script block");
    assert_eq!(results[0]["title"], "Real");
    assert_eq!(results[0]["items"][2], 3);
}

#[test]
fn next_data_accepts_alternate_attribute_order_and_quoting() {
    // The real script tag may come after other attributes and use single
    // quotes — the attribute scanner must tolerate both.
    let html = r#"<script type="application/json" id='__NEXT_DATA__' data-foo="bar">
        {"props":{"pageProps":{"v":42}}}
    </script>"#;
    let results = extract_next_data(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["v"], 42);
}

#[test]
fn next_data_rejects_data_attribute_decoy() {
    // A `data-id="__NEXT_DATA__"` attribute on some other script must
    // NOT be picked up as the real `id="__NEXT_DATA__"` script.
    let html = r#"<script data-id="__NEXT_DATA__" type="application/json">
        {"props":{"pageProps":{"v":"wrong"}}}
    </script>"#;
    let results = extract_next_data(html);
    assert!(results.is_empty(), "data-id decoy must not match");
}

#[test]
fn sveltekit_preserves_non_ascii_string_literals() {
    // SvelteKit data payloads can include non-ASCII string values
    // (titles, descriptions in CJK / emoji). Per-byte `b as char` casting
    // inside js_literal_to_json would corrupt the UTF-8 bytes; this test
    // guards against regression by reading the value back through the
    // public extractor.
    let html = "<script>kit.start(app, target, { data: [null, {\"type\":\"data\",\"data\":{\"title\":\"日本語タイトル 🌱 dasdadasd\"}}, null] });</script>";
    let results = extract_sveltekit(html);
    assert_eq!(results.len(), 1, "should extract one sveltekit payload");
    assert_eq!(results[0]["title"], "日本語タイトル 🌱 dasdadasd");
}
