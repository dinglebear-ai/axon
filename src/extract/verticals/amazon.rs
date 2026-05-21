//! Amazon product vertical extractor (antibot-gated).
//!
//! Matches amazon.com/dp/{asin} and /gp/product/{asin} URLs.
//! auto_dispatch: false — Amazon deploys aggressive antibot; explicit opt-in only.
//!
//! Uses Chrome rendering when configured, falls back to plain reqwest.
//! Returns VerticalBlockedAntibot when the response signals an antibot challenge.

use crate::core::config::RenderMode;
use crate::core::http::http_client;
use crate::crawl::scrape::{build_scrape_website, fetch_single_page};
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};
use crate::services::error::ServiceTaxonomyError;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "amazon",
    label: "Amazon Product",
    description: "Attempts Amazon product page extraction. Antibot-gated — explicit-only via --vertical amazon.",
    url_patterns: &[
        "https://amazon.com/dp/{asin}",
        "https://amazon.com/gp/product/{asin}",
    ],
    auto_dispatch: false,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if !host.contains("amazon.") {
        return false;
    }
    let path = parsed.path();
    let segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if segs.len() >= 2 && segs[0] == "dp" {
        return true;
    }
    if segs.len() >= 3 && segs[0] == "gp" && segs[1] == "product" {
        return true;
    }
    if let Some(pos) = segs.iter().position(|&s| s == "dp")
        && pos + 1 < segs.len()
    {
        return true;
    }
    false
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let body = fetch_page_body(url, ctx).await?;

    let is_blocked = body.contains("Type the characters you see in this image")
        || body.contains("Enter the characters you see below")
        || body.contains("Robot Check")
        || (body.len() < 5000 && body.contains("automated"));

    if is_blocked {
        return Err(ServiceTaxonomyError::VerticalBlockedAntibot {
            vertical: INFO.name,
            vendor: crate::services::error::ChallengeVendor::Other("amazon-bot"),
        });
    }

    let jsonld = extract_jsonld(&body);
    let asin = extract_asin(url);
    build_scraped_doc(url, jsonld, asin)
}

async fn fetch_page_body(url: &str, ctx: &VerticalContext) -> Result<String, VerticalError> {
    // Use Chrome path when Chrome is configured and render mode supports it
    // Only use Chrome when explicitly configured — AutoSwitch means "try HTTP
    // first" which is what the reqwest fallback already does for structured APIs.
    let use_chrome =
        ctx.cfg.chrome_remote_url.is_some() && ctx.cfg.render_mode == RenderMode::Chrome;

    if use_chrome {
        return fetch_via_chrome(url, ctx).await;
    }
    fetch_via_reqwest(url, ctx).await
}

async fn fetch_via_chrome(url: &str, ctx: &VerticalContext) -> Result<String, VerticalError> {
    let mut website = build_scrape_website(ctx.cfg.as_ref(), url).map_err(|_| {
        VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        }
    })?;
    let page = fetch_single_page(ctx.cfg.as_ref(), &mut website, url)
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;

    match page.status_code {
        403 | 503 => Err(ServiceTaxonomyError::VerticalBlockedAntibot {
            vertical: INFO.name,
            vendor: crate::services::error::ChallengeVendor::Other("amazon-bot"),
        }),
        404 => Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        }),
        429 => Err(VerticalError::VerticalRateLimited {
            vertical: INFO.name,
            retry_after: None,
        }),
        200 | 0 => Ok(page.html),
        s => Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: s,
        }),
    }
}

async fn fetch_via_reqwest(url: &str, ctx: &VerticalContext) -> Result<String, VerticalError> {
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(url)
        .header("User-Agent", ctx.ua())
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status: 0,
        })?;

    let status = resp.status().as_u16();
    match status {
        403 | 503 => {
            return Err(ServiceTaxonomyError::VerticalBlockedAntibot {
                vertical: INFO.name,
                vendor: crate::services::error::ChallengeVendor::Other("amazon-bot"),
            });
        }
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

    resp.text()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        })
}

fn extract_asin(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let path = parsed.path();
    let segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if let Some(pos) = segs.iter().position(|&s| s == "dp") {
        return segs.get(pos + 1).map(|s| s.to_string());
    }
    if segs.len() >= 3 && segs[0] == "gp" && segs[1] == "product" {
        return segs.get(2).map(|s| s.to_string());
    }
    None
}

fn build_extra(jsonld: Option<&serde_json::Value>, asin: Option<&str>) -> serde_json::Value {
    let mut obj = serde_json::json!({});
    if let Some(j) = jsonld {
        if let Some(brand) = j["brand"]["name"].as_str() {
            obj["amz_brand"] = serde_json::Value::String(brand.to_string());
        }
        if let Some(price) = j["offers"]["price"].as_str() {
            obj["amz_price"] = serde_json::Value::String(price.to_string());
        }
        if let Some(currency) = j["offers"]["priceCurrency"].as_str() {
            obj["amz_currency"] = serde_json::Value::String(currency.to_string());
        }
        if let Some(avail) = j["offers"]["availability"].as_str() {
            let short = avail.split('/').next_back().unwrap_or(avail);
            obj["amz_availability"] = serde_json::Value::String(short.to_string());
        }
        if let Some(r) = j["aggregateRating"]["ratingValue"].as_f64() {
            obj["amz_rating"] = serde_json::json!(r);
        }
        if let Some(rc) = j["aggregateRating"]["reviewCount"].as_u64() {
            obj["amz_review_count"] = serde_json::json!(rc);
        }
    }
    if let Some(a) = asin {
        obj["amz_asin"] = serde_json::Value::String(a.to_string());
    }
    obj
}

fn build_scraped_doc(
    url: &str,
    jsonld: Option<serde_json::Value>,
    asin: Option<String>,
) -> Result<ScrapedDoc, VerticalError> {
    let title = jsonld
        .as_ref()
        .and_then(|j| j["name"].as_str())
        .map(str::to_string);

    let mut md = if let Some(ref t) = title {
        format!("# {t}\n\n")
    } else {
        "# Amazon Product\n\n".to_string()
    };

    if let Some(ref j) = jsonld {
        let price = j["offers"]["price"].as_str();
        let currency = j["offers"]["priceCurrency"].as_str().unwrap_or("USD");
        if let Some(p) = price {
            md.push_str(&format!("**Price:** {p} {currency}\n"));
        }
        let avail = j["offers"]["availability"].as_str().unwrap_or("");
        if !avail.is_empty() {
            let avail_short = avail.split('/').next_back().unwrap_or(avail);
            md.push_str(&format!("**Availability:** {avail_short}\n"));
        }
        if let Some(brand) = j["brand"]["name"].as_str() {
            md.push_str(&format!("**Brand:** {brand}\n"));
        }
        let rating = j["aggregateRating"]["ratingValue"].as_f64();
        let review_count = j["aggregateRating"]["reviewCount"].as_u64();
        if let Some(r) = rating {
            let rc = review_count
                .map(|n| format!(" ({n} reviews)"))
                .unwrap_or_default();
            md.push_str(&format!("**Rating:** {r:.1}{rc}\n"));
        }
        if let Some(img) = j["image"].as_str().or_else(|| {
            j["image"]
                .as_array()
                .and_then(|a| a.first().and_then(|v| v.as_str()))
        }) {
            md.push_str(&format!("**Image:** {img}\n"));
        }
        if let Some(desc) = j["description"].as_str() {
            let excerpt: String = desc.chars().take(1000).collect();
            md.push_str(&format!("\n{excerpt}\n"));
        }
    }

    if let Some(ref a) = asin {
        md.push_str(&format!("\n**ASIN:** {a}\n"));
    }
    md.push_str(&format!("\n**Amazon:** {url}\n"));

    let extra = build_extra(jsonld.as_ref(), asin.as_deref());

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 2,
        structured: jsonld,
        follow_crawl_urls: vec![],
        extra: Some(extra),
    })
}

#[cfg(test)]
#[path = "amazon_tests.rs"]
mod tests;

fn extract_jsonld(html: &str) -> Option<serde_json::Value> {
    let mut remaining = html;
    while let Some(start) = remaining.find(r#"application/ld+json">"#) {
        let after = &remaining[start + r#"application/ld+json">"#.len()..];
        if let Some(end) = after.find("</script>") {
            let json_str = &after[..end];
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                let type_str = v["@type"].as_str().unwrap_or("");
                if type_str == "Product" {
                    return Some(v);
                }
                if let Some(arr) = v.as_array() {
                    for item in arr {
                        if item["@type"].as_str() == Some("Product") {
                            return Some(item.clone());
                        }
                    }
                }
            }
        }
        remaining = &remaining[start + 1..];
    }
    None
}
