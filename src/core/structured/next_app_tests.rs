use super::*;

#[test]
fn detects_app_router_page() {
    let html = "<script>self.__next_f.push([1, \"hello world that is long enough text\"])</script>";
    assert!(is_app_router_page(html));
}

#[test]
fn pages_router_page_is_not_app_router() {
    let html = r#"<script id="__NEXT_DATA__" type="application/json">{"props":{}}</script>"#;
    assert!(!is_app_router_page(html));
}

#[test]
fn page_with_neither_marker_is_not_app_router() {
    let html = "<html><body><p>nothing</p></body></html>";
    assert!(!is_app_router_page(html));
}

#[test]
fn extracts_long_string_from_push() {
    // Realistic Flight payload: outer push string contains RSC chunk
    // syntax with inner JSON string literals. Scanner walks the
    // decoded outer string for inner quoted leaves.
    let html = r#"<script>self.__next_f.push([1, "3:\"This is a complete sentence about something useful indeed.\""])</script>"#;
    let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
    assert_eq!(out.len(), 1);
    assert!(out[0].contains("This is a complete sentence"));
}

#[test]
fn filters_class_name_short_tokens() {
    // Mixed push: real content + short class-like tokens
    let html = r#"<script>self.__next_f.push([1, "{\"className\":\"mx-auto\",\"text\":\"Welcome to the project documentation page.\"}"])</script>"#;
    let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
    // The "mx-auto" class name is too short (8 chars); the prose passes
    assert!(
        !out.iter()
            .any(|s| s.contains("mx-auto") && s.len() < MIN_STRING_LEN)
    );
    assert!(out.iter().any(|s| s.contains("Welcome to the project")));
}

#[test]
fn filters_url_and_path_tokens() {
    let html = r#"<script>self.__next_f.push([1, "{\"href\":\"/some/nested/path/here/that/is/long\",\"src\":\"https://example.com/img.png\"}"])</script>"#;
    let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
    assert!(
        out.iter()
            .all(|s| !s.starts_with('/') && !s.starts_with("http"))
    );
}

#[test]
fn filters_asset_extensions() {
    let html = r#"<script>self.__next_f.push([1, "static/chunks/main-bundle-abcdef.js"])</script>"#;
    let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
    assert!(out.is_empty());
}

#[test]
fn dedupes_repeated_strings() {
    // Same RSC inner-string emitted by two pushes — dedup catches it.
    let html = r#"<script>self.__next_f.push([1, "3:\"This sentence appears more than once on the page.\""])</script>
        <script>self.__next_f.push([1, "4:\"This sentence appears more than once on the page.\""])</script>"#;
    let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
    assert_eq!(out.len(), 1);
}

#[test]
fn max_strings_cap_respected() {
    let mut html = String::new();
    for i in 0..20 {
        html.push_str(&format!(
            "<script>self.__next_f.push([1, \"{i}:\\\"Unique sentence number {i} long enough to pass filter.\\\"\"])</script>\n"
        ));
    }
    let out = extract_next_app_strings(&html, 3);
    assert!(out.len() <= 3);
}

#[test]
fn skips_external_and_module_scripts() {
    let html = r#"
            <script src="/_next/chunks/main.js"></script>
            <script type="module">import x from "y"; self.__next_f.push([1, "0:\"This should be ignored as a module payload entirely.\""])</script>
            <script>self.__next_f.push([1, "0:\"This is real inline content that the scanner picks up.\""])</script>
        "#;
    let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
    assert!(
        out.iter().any(|s| s.contains("real inline content")),
        "scanner must pick up the inline RSC string"
    );
    assert!(
        !out.iter().any(|s| s.contains("ignored as a module")),
        "scanner must skip type=module scripts"
    );
}

#[test]
fn no_push_calls_returns_empty() {
    let html = "<script>console.log('hi');</script>";
    assert!(extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS).is_empty());
}

#[test]
fn handles_escaped_quotes_in_payload() {
    // A push call whose string contains escaped quotes
    let html = r#"<script>self.__next_f.push([1, "He said \"hello world from the project page\" loudly."])</script>"#;
    let out = extract_next_app_strings(html, DEFAULT_MAX_NEXT_APP_STRINGS);
    assert!(
        out.iter()
            .any(|s| s.contains("hello world from the project"))
    );
}

#[test]
fn content_token_rejects_hex_id() {
    assert!(!is_content_token("a1b2c3d4e5f6a1b2c3d4e5f6"));
}

#[test]
fn content_token_accepts_real_sentence() {
    assert!(is_content_token(
        "This is a complete sentence with letters."
    ));
}

#[test]
fn content_token_rejects_short_string() {
    assert!(!is_content_token("hi there friend"));
}
