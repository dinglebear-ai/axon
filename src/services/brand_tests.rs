use super::*;

#[test]
fn test_extracts_hex_colors() {
    let html = r#"<html><head><style>
        .header { background-color: #3498db; }
        .text { color: #2c3e50; }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(hexes.contains(&"#3498DB"), "should find header bg color");
    assert!(hexes.contains(&"#2C3E50"), "should find text color");
}

#[test]
fn test_filters_boring_colors() {
    let html = r#"<html><head><style>
        body { background-color: #ffffff; color: #000000; }
        .brand { color: #3498db; }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(!hexes.contains(&"#FFFFFF"), "white should be filtered");
    assert!(!hexes.contains(&"#000000"), "black should be filtered");
    assert!(hexes.contains(&"#3498DB"), "brand color should survive");
}

#[test]
fn test_extracts_fonts() {
    let html = r#"<html><head><style>
        body { font-family: "Inter", "Helvetica Neue", sans-serif; }
        code { font-family: 'Fira Code', monospace; }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    assert!(
        brand.fonts.contains(&"Inter".to_string()),
        "should find Inter"
    );
    assert!(
        brand.fonts.contains(&"Fira Code".to_string()),
        "should find Fira Code"
    );
    assert!(
        !brand.fonts.contains(&"sans-serif".to_string()),
        "generic should be excluded"
    );
    assert!(
        !brand.fonts.contains(&"monospace".to_string()),
        "generic should be excluded"
    );
}

#[test]
fn test_extracts_favicon() {
    let html = r#"<html><head>
        <link rel="icon" href="/favicon.ico">
    </head><body></body></html>"#;

    let brand = extract_brand_from_html(html, Some("https://example.com"));
    assert_eq!(
        brand.favicon_url.as_deref(),
        Some("https://example.com/favicon.ico")
    );
}

#[test]
fn test_extracts_logo_by_class() {
    let html = r#"<html><body>
        <header>
            <img class="site-logo" src="/logo.svg" alt="Brand">
        </header>
    </body></html>"#;

    let brand = extract_brand_from_html(html, Some("https://example.com"));
    assert_eq!(
        brand.logo_url.as_deref(),
        Some("https://example.com/logo.svg")
    );
}

#[test]
fn test_extracts_brand_name_from_og_site_name() {
    let html = r#"<html><head>
        <meta property="og:site_name" content="Acme Corp">
    </head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    assert_eq!(brand.name.as_deref(), Some("Acme Corp"));
}

#[test]
fn test_css_custom_properties() {
    let html = r#"<html><head><style>
        :root {
            --primary: #3b82f6;
            --spacing: 1rem;
        }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(
        hexes.contains(&"#3B82F6"),
        "should find --primary CSS variable"
    );
}

#[test]
fn test_empty_html_returns_empty_result() {
    let brand = extract_brand_from_html("", None);
    assert!(brand.colors.is_empty());
    assert!(brand.fonts.is_empty());
    assert!(brand.logo_url.is_none());
    assert!(brand.favicon_url.is_none());
}

#[test]
fn test_max_10_colors() {
    let colors: Vec<String> = (0..15u8)
        .map(|i| {
            format!(
                ".c{i} {{ color: #{:02X}{:02X}{:02X}; }}",
                10 + i * 15,
                20 + i * 10,
                30 + i * 5
            )
        })
        .collect();
    let html = format!(
        "<html><head><style>{}</style></head><body></body></html>",
        colors.join("\n")
    );
    let brand = extract_brand_from_html(&html, None);
    assert!(brand.colors.len() <= 10, "should cap at 10 colors");
}

#[test]
fn test_rgb_color_parsing() {
    let html = r#"<html><head><style>
        .btn { background-color: rgb(52, 152, 219); }
    </style></head><body></body></html>"#;

    let brand = extract_brand_from_html(html, None);
    let hexes: Vec<&str> = brand.colors.iter().map(|c| c.hex.as_str()).collect();
    assert!(hexes.contains(&"#3498DB"), "rgb(52,152,219) -> #3498DB");
}
