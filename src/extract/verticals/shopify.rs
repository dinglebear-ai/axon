//! Shopify product vertical extractor.
//!
//! Matches any URL with /products/{handle} on a Shopify-looking host.
//! Uses the public /products/{handle}.json endpoint — no authentication needed.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "shopify",
    label: "Shopify Product",
    description: "Fetches product data from Shopify's public /products/{handle}.json endpoint including variants and pricing.",
    url_patterns: &["https://{shop}.myshopify.com/products/{handle}"],
    auto_dispatch: true,
};

/// Hosts that have /products/ paths but are NOT Shopify stores.
const NON_SHOPIFY_HOSTS: &[&str] = &[
    "github.com",
    "gitlab.com",
    "npmjs.com",
    "pypi.org",
    "crates.io",
    "arxiv.org",
    "stackoverflow.com",
    "reddit.com",
    "youtube.com",
    "twitter.com",
    "x.com",
    "amazon.com",
    "ebay.com",
];

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    // Reject known non-Shopify hosts
    for blocked in NON_SHOPIFY_HOSTS {
        if host == *blocked || host.ends_with(&format!(".{blocked}")) {
            return false;
        }
    }
    let path = parsed.path();
    let segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // Must contain /products/{handle} (handle must be non-empty, no dots)
    if let Some(pos) = segs.iter().position(|&s| s == "products") {
        let handle_idx = pos + 1;
        if handle_idx < segs.len() {
            let handle = segs[handle_idx];
            return !handle.is_empty() && !handle.contains('.');
        }
    }
    false
}

fn extract_handle(url: &str) -> Option<(String, String)> {
    let parsed = url::Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_string();
    let path = parsed.path().to_string();
    let segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let pos = segs.iter().position(|&s| s == "products")?;
    let handle = segs.get(pos + 1)?.to_string();
    Some((host, handle))
}

/// Strip simple HTML tags for plain text extraction.
fn strip_html(html: &str) -> String {
    html.chars()
        .scan(false, |in_tag, c| {
            if c == '<' {
                *in_tag = true;
                Some(None)
            } else if c == '>' {
                *in_tag = false;
                Some(None)
            } else if *in_tag {
                Some(None)
            } else {
                Some(Some(c))
            }
        })
        .flatten()
        .collect()
}

/// Build variant listing (capped at 20).
fn build_variant_section(variants: &serde_json::Value) -> String {
    let Some(arr) = variants.as_array() else {
        return String::new();
    };
    if arr.is_empty() {
        return String::new();
    }
    let mut section = String::from("\n## Variants\n\n");
    for v in arr.iter().take(20) {
        let vtitle = v["title"].as_str().unwrap_or("Default");
        let price = v["price"].as_str().unwrap_or("N/A");
        let available = v["available"].as_bool().unwrap_or(false);
        let availability = if available {
            "in stock"
        } else {
            "out of stock"
        };
        // Shopify products.json does not expose currency; omit the symbol
        section.push_str(&format!("- {vtitle}: {price} ({availability})\n"));
    }
    section
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let (host, handle) =
        extract_handle(url).ok_or_else(|| VerticalError::VerticalUnsupportedUrl {
            vertical: INFO.name,
            url: url.to_string(),
        })?;

    let api_url = format!("https://{host}/products/{handle}.json");

    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(&api_url)
        .header("User-Agent", ctx.api_ua())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;

    let status = resp.status().as_u16();
    match status {
        404 => {
            return Err(VerticalError::VerticalTargetNotFound {
                vertical: INFO.name,
                url: url.to_string(),
            });
        }
        429 => {
            return Err(VerticalError::VerticalRateLimited {
                vertical: INFO.name,
                retry_after: None,
            });
        }
        200 => {}
        _ => {
            return Err(VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status,
            });
        }
    }

    let data: serde_json::Value =
        resp.json()
            .await
            .map_err(|_| VerticalError::VerticalTargetUnavailable {
                vertical: INFO.name,
                status,
            })?;

    let product = &data["product"];
    let title_str = product["title"].as_str().unwrap_or(&handle);
    let vendor = product["vendor"].as_str().unwrap_or("");
    let product_type = product["product_type"].as_str().unwrap_or("");
    let body_html = product["body_html"].as_str().unwrap_or("");
    let body_text = strip_html(body_html);

    let title = Some(title_str.to_string());
    let mut md = format!("# {title_str}\n\n");
    if !vendor.is_empty() {
        md.push_str(&format!("**Vendor:** {vendor}\n"));
    }
    if !product_type.is_empty() {
        md.push_str(&format!("**Type:** {product_type}\n"));
    }
    if !body_text.trim().is_empty() {
        md.push('\n');
        md.push_str(body_text.trim());
        md.push('\n');
    }
    let variant_section = build_variant_section(&product["variants"]);
    if !variant_section.is_empty() {
        md.push_str(&variant_section);
    }
    md.push_str(&format!("\n**Product:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: Some(data),
        follow_crawl_urls: vec![],
    })
}
