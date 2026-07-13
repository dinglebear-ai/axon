use axon_api::MetadataMap;
use serde_json::json;

use super::validate;

fn single(key: &str, value: serde_json::Value) -> MetadataMap {
    let mut values = MetadataMap::new();
    values.insert(key.to_string(), value);
    values
}

#[test]
fn accepts_every_documented_option_with_a_good_value() {
    let good_values: &[(&str, serde_json::Value)] = &[
        ("max_pages", json!(2000)),
        ("max_pages", json!(0)),
        ("max_depth", json!(10)),
        ("include_subdomains", json!(false)),
        ("render_mode", json!("http")),
        ("render_mode", json!("chrome")),
        ("render_mode", json!("auto_switch")),
        ("discover_sitemaps", json!(true)),
        ("max_sitemaps", json!(512)),
        ("sitemap_since_days", json!(0)),
        ("url_whitelist", json!(["/docs", "/blog"])),
        ("url_whitelist", json!([])),
        ("url_blacklist", json!(["/private"])),
        ("etag_conditional", json!(true)),
        ("min_markdown_chars", json!(200)),
        ("drop_thin_markdown", json!(true)),
        ("warc_path", json!("artifact://warc/site.warc")),
        (
            "automation_script",
            json!("artifact://automation/steps.json"),
        ),
        ("verticals_enabled", json!(true)),
    ];

    for (key, value) in good_values {
        let values = single(key, value.clone());
        assert!(
            validate(&values).is_ok(),
            "expected {key}={value} to validate"
        );
    }
}

#[test]
fn rejects_bad_render_mode_enum_value() {
    let values = single("render_mode", json!("carrier_pigeon"));

    let err = validate(&values).expect_err("unsupported render_mode must fail");

    assert_eq!(err.code.0, "route.options.invalid");
    assert_eq!(err.stage, axon_error::ErrorStage::Routing);
    assert_eq!(
        err.details.get("option").map(String::as_str),
        Some("render_mode")
    );
}

#[test]
fn rejects_non_string_render_mode() {
    let values = single("render_mode", json!(true));

    let err = validate(&values).expect_err("non-string render_mode must fail");

    assert_eq!(err.code.0, "route.options.invalid");
}

#[test]
fn rejects_negative_integer_options() {
    for key in [
        "max_pages",
        "max_depth",
        "max_sitemaps",
        "sitemap_since_days",
        "min_markdown_chars",
    ] {
        let values = single(key, json!(-1));
        let err = validate(&values).expect_err(&format!("negative {key} must fail"));
        assert_eq!(err.code.0, "route.options.invalid", "{key}");
    }
}

#[test]
fn rejects_non_integer_number_options() {
    let values = single("max_pages", json!(3.5));

    let err = validate(&values).expect_err("fractional max_pages must fail");

    assert_eq!(err.code.0, "route.options.invalid");
}

#[test]
fn rejects_wrong_type_for_integer_option() {
    let values = single("max_depth", json!("ten"));

    let err = validate(&values).expect_err("string max_depth must fail");

    assert_eq!(err.code.0, "route.options.invalid");
}

#[test]
fn rejects_wrong_type_for_bool_options() {
    for key in [
        "include_subdomains",
        "discover_sitemaps",
        "etag_conditional",
        "drop_thin_markdown",
        "verticals_enabled",
    ] {
        let values = single(key, json!("yes"));
        let err = validate(&values).expect_err(&format!("string {key} must fail"));
        assert_eq!(err.code.0, "route.options.invalid", "{key}");
    }
}

#[test]
fn rejects_non_array_url_lists() {
    for key in ["url_whitelist", "url_blacklist"] {
        let values = single(key, json!("/docs"));
        let err = validate(&values).expect_err(&format!("non-array {key} must fail"));
        assert_eq!(err.code.0, "route.options.invalid", "{key}");
    }
}

#[test]
fn rejects_non_string_entries_in_url_lists() {
    let values = single("url_whitelist", json!(["/docs", 5]));

    let err = validate(&values).expect_err("non-string entry must fail");

    assert_eq!(err.code.0, "route.options.invalid");
}

#[test]
fn rejects_blank_entries_in_url_lists() {
    let values = single("url_blacklist", json!(["   "]));

    let err = validate(&values).expect_err("blank entry must fail");

    assert_eq!(err.code.0, "route.options.invalid");
}

#[test]
fn rejects_empty_artifact_ref_options() {
    for key in ["warc_path", "automation_script"] {
        let values = single(key, json!(""));
        let err = validate(&values).expect_err(&format!("empty {key} must fail"));
        assert_eq!(err.code.0, "route.options.invalid", "{key}");

        let wrong_type = single(key, json!(42));
        let err = validate(&wrong_type).expect_err(&format!("non-string {key} must fail"));
        assert_eq!(err.code.0, "route.options.invalid", "{key}");
    }
}

#[test]
fn leaves_legacy_disk_handoff_keys_unvalidated() {
    // manifest_path/markdown_root/map_urls are the pre-existing legacy keys
    // this pass intentionally does not harden (see module docs).
    let mut values = MetadataMap::new();
    values.insert("manifest_path".to_string(), json!(12345));
    values.insert("markdown_root".to_string(), json!(true));
    values.insert("map_urls".to_string(), json!("not-actually-an-array"));

    assert!(validate(&values).is_ok());
}

#[test]
fn ignores_keys_it_does_not_recognize() {
    // Key-membership rejection is the router's job (`allowed_option_keys`);
    // this module only validates values for keys it knows about.
    let values = single("definitely_not_a_web_option", json!({"anything": "goes"}));

    assert!(validate(&values).is_ok());
}

#[test]
fn accepts_full_documented_option_set_together() {
    let mut values = MetadataMap::new();
    values.insert("max_pages".to_string(), json!(2000));
    values.insert("max_depth".to_string(), json!(10));
    values.insert("include_subdomains".to_string(), json!(false));
    values.insert("render_mode".to_string(), json!("auto_switch"));
    values.insert("discover_sitemaps".to_string(), json!(true));
    values.insert("max_sitemaps".to_string(), json!(512));
    values.insert("sitemap_since_days".to_string(), json!(0));
    values.insert("url_whitelist".to_string(), json!(["/docs"]));
    values.insert("url_blacklist".to_string(), json!(["/private"]));
    values.insert("etag_conditional".to_string(), json!(true));
    values.insert("min_markdown_chars".to_string(), json!(200));
    values.insert("drop_thin_markdown".to_string(), json!(true));
    values.insert("warc_path".to_string(), json!("artifact://warc/site.warc"));
    values.insert(
        "automation_script".to_string(),
        json!("artifact://automation/steps.json"),
    );
    values.insert("verticals_enabled".to_string(), json!(true));

    assert!(validate(&values).is_ok());
}
