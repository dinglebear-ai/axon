//! eBay listing vertical extractor (antibot-gated).
//!
//! Matches ebay.com/itm/{id} and /sch/* URLs.
//! auto_dispatch: false — eBay deploys antibot; explicit opt-in only.
//!
//! Uses Chrome rendering when configured, falls back to plain reqwest.
//! Returns VerticalBlockedAntibot when the response signals a challenge.

use crate::context::VerticalContext;
use crate::error::VerticalError;
use crate::types::{ExtractorInfo, ScrapedDoc};
use axon_core::error::ServiceTaxonomyError;
use axon_core::http::http_client;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "ebay",
    label: "eBay Listing",
    description: "Attempts eBay listing extraction. Antibot-gated — explicit-only via --vertical ebay.",
    url_patterns: &["https://ebay.com/itm/{id}", "https://ebay.com/sch/*"],
    auto_dispatch: false,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    if !host.contains("ebay.") {
        return false;
    }
    let path = parsed.path().trim_matches('/');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segs.is_empty() {
        return false;
    }
    matches!(segs[0], "itm" | "sch" | "p")
}

pub async fn extract(url: &str, ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let body = fetch_page_body(url, ctx).await?;

    let is_blocked = body.contains("Robot or human?")
        || body.contains("Access Denied")
        || body.contains("captcha")
        || (body.len() < 5000 && body.contains("automated"));

    if is_blocked {
        return Err(ServiceTaxonomyError::VerticalBlockedAntibot {
            vertical: INFO.name,
            vendor: axon_core::error::ChallengeVendor::Other("ebay-bot"),
        });
    }

    let jsonld = extract_jsonld(&body);
    let item_id = extract_item_id(url);
    build_scraped_doc(url, jsonld, item_id)
}

async fn fetch_page_body(url: &str, ctx: &VerticalContext) -> Result<String, VerticalError> {
    // The old explicit-only extractor optionally used the removed axon-crawl
    // crate for Chrome. The restored extractor keeps the structured JSON-LD
    // path through the SSRF-guarded reqwest client; Chrome rendering now belongs
    // to the web adapter render provider.
    fetch_via_reqwest(url, ctx).await
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
                vendor: axon_core::error::ChallengeVendor::Other("ebay-bot"),
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

fn extract_item_id(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let path = parsed.path();
    let segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    // /itm/{id} — the most common form
    if segs.first() == Some(&"itm")
        && let Some(id) = segs.get(1)
    {
        // Strip any trailing slug suffix (e.g. /itm/12345678/some-title)
        let numeric: String = id.chars().take_while(|c| c.is_ascii_digit()).collect();
        if !numeric.is_empty() {
            return Some(numeric);
        }
    }
    // /p/{id} (product page alternate form)
    if segs.first() == Some(&"p") {
        return segs.get(1).map(|s| s.to_string());
    }
    None
}

fn build_extra(jsonld: Option<&serde_json::Value>, item_id: Option<&str>) -> serde_json::Value {
    let mut obj = serde_json::json!({});
    if let Some(j) = jsonld {
        if let Some(brand) = j["brand"]["name"].as_str() {
            obj["ebay_brand"] = serde_json::Value::String(brand.to_string());
        }
        let price_val = &j["offers"]["price"];
        let price_str = price_val
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| price_val.as_f64().map(|n| n.to_string()));
        if let Some(price) = price_str {
            obj["ebay_price"] = serde_json::Value::String(price);
        }
        if let Some(condition) = j["offers"]["itemCondition"].as_str() {
            let clean = condition
                .split('/')
                .next_back()
                .unwrap_or(condition)
                .trim_end_matches("Condition");
            obj["ebay_condition"] = serde_json::Value::String(clean.to_string());
        }
        if let Some(avail) = j["offers"]["availability"].as_str() {
            let short = avail.split('/').next_back().unwrap_or(avail);
            obj["ebay_availability"] = serde_json::Value::String(short.to_string());
        }
        if let Some(r) = j["aggregateRating"]["ratingValue"].as_f64() {
            obj["ebay_rating"] = serde_json::json!(r);
        }
        if let Some(rc) = j["aggregateRating"]["reviewCount"].as_u64() {
            obj["ebay_review_count"] = serde_json::json!(rc);
        }
    }
    if let Some(id) = item_id {
        obj["ebay_item_id"] = serde_json::Value::String(id.to_string());
    }
    obj
}

fn build_scraped_doc(
    url: &str,
    jsonld: Option<serde_json::Value>,
    item_id: Option<String>,
) -> Result<ScrapedDoc, VerticalError> {
    let title = jsonld
        .as_ref()
        .and_then(|j| j["name"].as_str())
        .map(str::to_string);

    let mut md = if let Some(ref t) = title {
        format!("# {t}\n\n")
    } else {
        "# eBay Listing\n\n".to_string()
    };

    if let Some(ref j) = jsonld {
        let price = j["offers"]["price"].as_str();
        let currency = j["offers"]["priceCurrency"].as_str().unwrap_or("");
        if let Some(p) = price {
            if currency.is_empty() {
                md.push_str(&format!("**Price:** {p}\n"));
            } else {
                md.push_str(&format!("**Price:** {p} {currency}\n"));
            }
        }
        let condition = j["offers"]["itemCondition"].as_str().unwrap_or("");
        if !condition.is_empty() {
            let condition_short = condition.split('/').next_back().unwrap_or(condition);
            // Strip trailing "Condition" suffix e.g. "NewCondition" → "New"
            let cond_clean = condition_short.trim_end_matches("Condition");
            md.push_str(&format!("**Condition:** {cond_clean}\n"));
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
            let excerpt: String = desc.chars().take(800).collect();
            md.push_str(&format!("\n{excerpt}\n"));
        }
    }

    if let Some(ref id) = item_id {
        md.push_str(&format!("\n**Item ID:** {id}\n"));
    }
    md.push_str(&format!("\n**eBay:** {url}\n"));

    let extra = build_extra(jsonld.as_ref(), item_id.as_deref());

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
#[path = "ebay_tests.rs"]
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
