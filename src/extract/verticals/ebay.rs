//! eBay listing vertical extractor (antibot-gated).
//!
//! Matches ebay.com/itm/{id} and /sch/* URLs.
//! auto_dispatch: false — eBay deploys antibot; explicit opt-in only.
//!
//! Attempts to fetch JSON-LD structured data from listing pages.
//! Returns VerticalBlockedAntibot when the response signals a challenge.

use crate::core::http::http_client;
use crate::extract::context::VerticalContext;
use crate::extract::error::VerticalError;
use crate::extract::types::{ExtractorInfo, ScrapedDoc};
use crate::services::error::ServiceTaxonomyError;

pub const INFO: ExtractorInfo = ExtractorInfo {
    name: "ebay",
    label: "eBay Listing",
    description: "Attempts eBay listing extraction. Antibot-gated — explicit-only via --vertical ebay.",
    url_patterns: &[
        "https://ebay.com/itm/{id}",
        "https://ebay.com/sch/*",
    ],
    auto_dispatch: false,
};

pub fn matches(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let host = parsed.host_str().unwrap_or_default().to_lowercase();
    // Match ebay.com, www.ebay.com, ebay.co.uk, ebay.de, etc.
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

pub async fn extract(url: &str, _ctx: &VerticalContext) -> Result<ScrapedDoc, VerticalError> {
    let client = http_client().map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status: 0,
    })?;

    let resp = client
        .get(url)
        .header("User-Agent", "Mozilla/5.0 (compatible; axon-bot/1.0)")
        .header("Accept", "text/html,application/xhtml+xml")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send()
        .await
        .map_err(|_| VerticalError::VerticalTargetUnavailable { vertical: INFO.name, status: 0 })?;

    let status = resp.status().as_u16();
    match status {
        403 | 503 => return Err(ServiceTaxonomyError::VerticalBlockedAntibot {
            vertical: INFO.name,
            vendor: crate::services::error::ChallengeVendor::Other("ebay-bot"),
        }),
        404 => return Err(VerticalError::VerticalTargetNotFound {
            vertical: INFO.name,
            url: url.to_string(),
        }),
        429 => return Err(VerticalError::VerticalRateLimited {
            vertical: INFO.name,
            retry_after: None,
        }),
        200 => {}
        _ => return Err(VerticalError::VerticalTargetUnavailable {
            vertical: INFO.name,
            status,
        }),
    }

    let body = resp.text().await.map_err(|_| VerticalError::VerticalTargetUnavailable {
        vertical: INFO.name,
        status,
    })?;

    // Heuristic antibot detection
    let is_blocked = body.contains("Robot or human?")
        || body.contains("Access Denied")
        || body.contains("captcha")
        || (body.len() < 5000 && body.contains("automated"));

    if is_blocked {
        return Err(ServiceTaxonomyError::VerticalBlockedAntibot {
            vertical: INFO.name,
            vendor: crate::services::error::ChallengeVendor::Other("ebay-bot"),
        });
    }

    let jsonld = extract_jsonld(&body);
    let title = jsonld
        .as_ref()
        .and_then(|j| j["name"].as_str())
        .map(str::to_string);

    let mut md = format!("# eBay Listing\n\n");
    if let Some(ref t) = title {
        md = format!("# {t}\n\n");
    }
    if let Some(ref j) = jsonld {
        if let Some(price) = j["offers"]["price"].as_str() {
            md.push_str(&format!("**Price:** {price}\n"));
        }
        if let Some(condition) = j["offers"]["itemCondition"].as_str() {
            let condition_short = condition.split('/').last().unwrap_or(condition);
            md.push_str(&format!("**Condition:** {condition_short}\n"));
        }
        if let Some(brand) = j["brand"]["name"].as_str() {
            md.push_str(&format!("**Brand:** {brand}\n"));
        }
        if let Some(desc) = j["description"].as_str() {
            let excerpt: String = desc.chars().take(400).collect();
            md.push_str(&format!("\n{excerpt}\n"));
        }
    }
    md.push_str(&format!("\n**eBay:** {url}\n"));

    Ok(ScrapedDoc {
        url: url.to_string(),
        markdown: md,
        title,
        extractor_name: INFO.name,
        extractor_version: 1,
        structured: jsonld,
    })
}

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
