use super::*;
use axon_services::types::{BrandColor, BrandResult, ColorUsage, LogoVariant};

fn make_brand_result() -> BrandResult {
    BrandResult {
        url: "https://example.com".to_string(),
        name: Some("Acme Corp".to_string()),
        colors: vec![
            BrandColor {
                hex: "#3498DB".to_string(),
                usage: ColorUsage::Primary,
                count: 5,
            },
            BrandColor {
                hex: "#2ECC71".to_string(),
                usage: ColorUsage::Secondary,
                count: 3,
            },
        ],
        fonts: vec!["Inter".to_string(), "Fira Code".to_string()],
        logos: vec![LogoVariant {
            url: "https://example.com/logo.svg".to_string(),
            kind: "logo".to_string(),
        }],
        logo_url: Some("https://example.com/logo.svg".to_string()),
        favicon_url: Some("https://example.com/favicon.ico".to_string()),
        og_image: None,
    }
}

#[test]
fn test_format_brand_summary_contains_name() {
    let result = make_brand_result();
    let output = format_brand_summary(&result);
    assert!(
        output.contains("Acme Corp"),
        "should include brand name, got: {output}"
    );
}

#[test]
fn test_format_brand_summary_contains_color_count() {
    let result = make_brand_result();
    let output = format_brand_summary(&result);
    assert!(
        output.contains("colors=2"),
        "should mention exact color count as 'colors=2', got: {output}"
    );
}

#[test]
fn test_format_brand_summary_contains_font_name() {
    let result = make_brand_result();
    let output = format_brand_summary(&result);
    assert!(
        output.contains("Inter"),
        "should include font names, got: {output}"
    );
}
