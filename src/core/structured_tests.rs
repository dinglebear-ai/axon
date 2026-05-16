use super::*;

// ── extract_json_ld ──────────────────────────────────────────────────────

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

// ── extract_next_data ────────────────────────────────────────────────────

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
fn next_data_missing_page_props_falls_back_to_envelope() {
    let html = r#"
        <script id="__NEXT_DATA__" type="application/json">
            {"buildId":"abc","page":"/x"}
        </script>
    "#;
    let results = extract_next_data(html);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0]["buildId"], "abc");
}

#[test]
fn next_data_app_router_page_returns_empty() {
    // App Router pages use self.__next_f.push, NOT __NEXT_DATA__
    let html = r#"
        <script>self.__next_f.push([1, "0:[\"$\",\"main\",null,{}]"])</script>
    "#;
    let results = extract_next_data(html);
    assert!(results.is_empty());
}

// ── extract_sveltekit ────────────────────────────────────────────────────

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
fn sveltekit_no_kit_start_returns_empty() {
    let html = "<html><body><p>plain</p></body></html>";
    assert!(extract_sveltekit(html).is_empty());
}

// ── sanitize_json_newlines ───────────────────────────────────────────────

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
fn sanitize_leaves_chars_outside_strings_untouched() {
    // Newline OUTSIDE a string (between fields) must not be escaped.
    let input = "{\n  \"a\": 1\n}";
    let sanitized = sanitize_json_newlines(input);
    assert!(
        sanitized.contains('\n'),
        "newline outside strings preserved"
    );
    // And it still parses as valid JSON
    let parsed: Value = serde_json::from_str(&sanitized).unwrap();
    assert_eq!(parsed["a"], 1);
}

// ── extract_all + helpers ────────────────────────────────────────────────

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
fn schema_type_of_returns_none_when_absent() {
    let v: Value = serde_json::from_str(r#"{"name":"x"}"#).unwrap();
    assert!(schema_type_of(&v).is_none());
}

#[test]
fn schema_id_of_extracts_when_present() {
    let v: Value =
        serde_json::from_str(r#"{"@id":"https://example.com/p/1","@type":"Product"}"#).unwrap();
    assert_eq!(schema_id_of(&v).as_deref(), Some("https://example.com/p/1"));
}
