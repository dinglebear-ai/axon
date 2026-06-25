use sha2::{Digest, Sha256};
use spider::url::Url;

pub fn url_to_domain(url: &str) -> String {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "unknown".to_string())
        .replace(['[', ']', ':'], "_")
}

pub fn url_to_filename(url: &str, idx: u32) -> String {
    let parsed = Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .unwrap_or("unknown-host");
    let path = parsed.as_ref().map(|u| u.path()).unwrap_or("/unknown-path");

    let stem_raw = format!("{host}{path}");
    let stem: String = stem_raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(80)
        .collect();

    format!("{:04}-{stem}.md", idx)
}

pub fn url_to_stable_filename(url: &str) -> String {
    let parsed = Url::parse(url).ok();
    let host = parsed
        .as_ref()
        .and_then(|u| u.host_str())
        .unwrap_or("unknown-host");
    let path = parsed.as_ref().map(|u| u.path()).unwrap_or("/unknown-path");

    let stem_raw = format!("{host}{path}");
    let stem = sanitized_filename_stem(&stem_raw);
    let identity = canonical_url_identity(url, parsed);
    let hash = Sha256::digest(identity.as_bytes());
    let short_hash = &hex::encode(hash)[..12];

    format!("{stem}-{short_hash}.md")
}

fn canonical_url_identity(url: &str, parsed: Option<Url>) -> String {
    parsed
        .map(|mut parsed| {
            parsed.set_fragment(None);
            parsed.to_string()
        })
        .unwrap_or_else(|| url.to_string())
}

fn sanitized_filename_stem(raw: &str) -> String {
    let mut stem = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(80)
        .collect::<String>();

    while stem.ends_with('-') {
        stem.pop();
    }

    if stem.is_empty() {
        "unknown".to_string()
    } else {
        stem
    }
}
