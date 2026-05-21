//! Brand identity extraction from a URL.
//!
//! The pure computation (`extract_brand_from_html`) takes raw HTML and performs
//! no network calls, making it fully testable without a running server.
//!
//! Submodules hold the heavy extraction logic to stay within the 500-line policy:
//! - `colors` — hex/rgb/hsl color extraction and classification
//! - `fonts`  — font-family extraction and filtering

mod colors;
mod fonts;

use std::error::Error;

use once_cell::sync::Lazy;
use regex::Regex;
use scraper::{Html, Selector};
use tokio::sync::mpsc;
use url::Url;

use crate::core::config::Config;
use crate::core::http::http_client;
use crate::services::events::{LogLevel, ServiceEvent, emit};
use crate::services::types::{BrandResult, LogoVariant};

// ── Regex patterns (compiled once, shared with submodules) ───────────────────

pub(super) static CSS_DECL: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)([\w-]+)\s*:\s*([^;}{]+)").unwrap());
pub(super) static CSS_VAR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)--([\w-]+)\s*:\s*([^;}{]+)").unwrap());
pub(super) static HEX_COLOR: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"#([0-9a-fA-F]{3})\b|#([0-9a-fA-F]{6})\b").unwrap());
pub(super) static RGB_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)rgb\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*\)").unwrap()
});
pub(super) static RGBA_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)rgba\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*[\d.]+\s*\)").unwrap()
});
pub(super) static HSL_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)hsla?\(\s*(\d{1,3})\s*,\s*(\d{1,3})%\s*,\s*(\d{1,3})%\s*(?:,\s*[\d.]+\s*)?\)")
        .unwrap()
});
static TW_COLOR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?:bg|text|border|ring|outline|shadow|accent|fill|stroke)-\[([^\]]+)\]").unwrap()
});
pub(super) static FONT_SHORTHAND_FAMILY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?ix)(?:^|\s)(?:xx-small|x-small|small|medium|large|x-large|xx-large|larger|smaller|\d*\.?\d+(?:px|rem|em|pt|pc|in|cm|mm|%|vw|vh|vmin|vmax))(?:\s*/\s*[^\s,]+)?\s+(.+)$"#,
    )
    .unwrap()
});

macro_rules! sel {
    ($s:expr) => {{
        static S: Lazy<Selector> = Lazy::new(|| Selector::parse($s).unwrap());
        &*S
    }};
}

// ── CSS declaration (shared type for submodules) ─────────────────────────────

pub(super) struct CssDecl {
    pub(super) property: String,
    pub(super) value: String,
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Fetch `url` and extract brand identity.
pub async fn brand(
    cfg: &Config,
    url: &str,
    tx: Option<mpsc::Sender<ServiceEvent>>,
) -> Result<BrandResult, Box<dyn Error>> {
    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: format!("brand: fetching {url}"),
        },
    )
    .await;

    let client = http_client()?;
    let mut req = client.get(url);
    for header_str in &cfg.custom_headers {
        if let Some((k, v)) = header_str.split_once(": ") {
            req = req.header(k, v);
        }
    }
    let response = req.send().await?;
    let html = response.text().await?;

    emit(
        &tx,
        ServiceEvent::Log {
            level: LogLevel::Info,
            message: "brand: analyzing".to_string(),
        },
    )
    .await;

    let mut result = extract_brand_from_html(&html, Some(url));
    result.url = url.to_string();
    Ok(result)
}

// ── Pure extraction (no I/O) ─────────────────────────────────────────────────

/// Extract brand identity from raw HTML.
/// `page_url` is used only for resolving relative paths.
pub(crate) fn extract_brand_from_html(html: &str, page_url: Option<&str>) -> BrandResult {
    let doc = Html::parse_document(html);
    let base_url = page_url.and_then(|u| Url::parse(u).ok());

    let name = extract_brand_name(&doc);
    let css_sources = collect_css(&doc);
    let colors_out = colors::extract_colors(&css_sources, name.as_deref());
    let fonts_out = fonts::extract_fonts(&css_sources, name.as_deref());
    let logo_url = find_logo(&doc, base_url.as_ref());
    let favicon_url = find_favicon(&doc, base_url.as_ref());
    let logos = find_all_logos(&doc, base_url.as_ref());
    let og_image = find_og_image(&doc, base_url.as_ref());

    BrandResult {
        url: page_url.unwrap_or("").to_string(),
        name,
        colors: colors_out,
        fonts: fonts_out,
        logos,
        logo_url,
        favicon_url,
        og_image,
    }
}

// ── CSS collection ───────────────────────────────────────────────────────────

fn collect_css(doc: &Html) -> Vec<CssDecl> {
    let mut decls = Vec::new();

    for el in doc.select(sel!("style")) {
        let text: String = el.text().collect();
        parse_declarations(&text, &mut decls);
        parse_css_variables(&text, &mut decls);
    }

    for el in doc.select(sel!("[style]")) {
        if let Some(style) = el.value().attr("style") {
            parse_declarations(style, &mut decls);
        }
    }

    for el in doc.select(sel!("[class]")) {
        if let Some(class) = el.value().attr("class") {
            parse_tailwind_colors(class, &mut decls);
        }
    }

    for el in doc.select(sel!("meta[name='theme-color']")) {
        if let Some(content) = el.value().attr("content") {
            decls.push(CssDecl {
                property: "background-color".to_string(),
                value: content.to_string(),
            });
        }
    }

    for el in doc.select(sel!("link[rel='preload'][as='font']")) {
        if let Some(href) = el.value().attr("href")
            && let Some(name) = fonts::extract_font_name_from_url(href)
        {
            decls.push(CssDecl {
                property: "font-family".to_string(),
                value: format!("\"{name}\""),
            });
        }
    }

    #[allow(clippy::collapsible_if)]
    for el in doc.select(sel!("link[rel='stylesheet']")) {
        if let Some(href) = el.value().attr("href") {
            if href.contains("fonts.googleapis.com") || href.contains("fonts.bunny.net") {
                for font in fonts::extract_google_fonts_from_url(href) {
                    decls.push(CssDecl {
                        property: "font-family".to_string(),
                        value: format!("\"{font}\""),
                    });
                }
            }
        }
    }

    decls
}

fn parse_declarations(css_text: &str, out: &mut Vec<CssDecl>) {
    for cap in CSS_DECL.captures_iter(css_text) {
        let property = cap[1].to_ascii_lowercase();
        let value = cap[2].trim().to_string();
        out.push(CssDecl { property, value });
    }
}

fn parse_css_variables(css_text: &str, out: &mut Vec<CssDecl>) {
    for cap in CSS_VAR.captures_iter(css_text) {
        let var_name = cap[1].to_ascii_lowercase();
        let value = cap[2].trim().to_string();
        if is_color_value(&value) {
            let property = if var_name.contains("background") || var_name.contains("bg") {
                "background-color"
            } else if var_name.contains("text")
                || var_name.contains("foreground")
                || var_name.contains("fg")
            {
                "color"
            } else if var_name.contains("border") || var_name.contains("accent") {
                "border-color"
            } else {
                "color"
            };
            out.push(CssDecl {
                property: property.to_string(),
                value,
            });
        }
    }
}

fn is_color_value(v: &str) -> bool {
    HEX_COLOR.is_match(v)
        || RGB_COLOR.is_match(v)
        || RGBA_COLOR.is_match(v)
        || HSL_COLOR.is_match(v)
}

fn parse_tailwind_colors(class: &str, out: &mut Vec<CssDecl>) {
    for cap in TW_COLOR.captures_iter(class) {
        let value = &cap[1];
        if is_color_value(value) {
            let full = cap.get(0).unwrap().as_str();
            let property = if full.starts_with("bg-") {
                "background-color"
            } else if full.starts_with("text-") {
                "color"
            } else if full.starts_with("border-") {
                "border-color"
            } else {
                "color"
            };
            out.push(CssDecl {
                property: property.to_string(),
                value: value.to_string(),
            });
        }
    }
}

// ── Logo detection ────────────────────────────────────────────────────────────

fn find_logo(doc: &Html, base_url: Option<&Url>) -> Option<String> {
    for el in doc.select(sel!("header img, nav img")) {
        let class = el.value().attr("class").unwrap_or("");
        let id = el.value().attr("id").unwrap_or("");
        let alt = el.value().attr("alt").unwrap_or("");
        let src = el.value().attr("src")?;
        if ci_contains(class, "logo") || ci_contains(id, "logo") || ci_contains(alt, "logo") {
            return Some(resolve_url(src, base_url));
        }
    }

    for el in doc.select(sel!("a[href='/'] img, a[href] img")) {
        if let Some(parent) = el.parent().and_then(|p| p.value().as_element()) {
            let href = parent.attr("href").unwrap_or("");
            if (href == "/" || href.ends_with(".com") || href.ends_with(".com/"))
                && let Some(src) = el.value().attr("src")
            {
                return Some(resolve_url(src, base_url));
            }
        }
    }

    None
}

fn find_favicon(doc: &Html, base_url: Option<&Url>) -> Option<String> {
    doc.select(sel!("link[rel]"))
        .find(|el| {
            el.value()
                .attr("rel")
                .is_some_and(|r| r.to_lowercase().contains("icon"))
        })
        .and_then(|el| el.value().attr("href"))
        .map(|href| resolve_url(href, base_url))
}

fn find_og_image(doc: &Html, base_url: Option<&Url>) -> Option<String> {
    doc.select(sel!("meta[property='og:image']"))
        .find_map(|el| el.value().attr("content").filter(|c| !c.is_empty()))
        .or_else(|| {
            doc.select(sel!("meta[name='twitter:image']"))
                .find_map(|el| el.value().attr("content").filter(|c| !c.is_empty()))
        })
        .map(|src| resolve_url(src, base_url))
}

fn find_all_logos(doc: &Html, base_url: Option<&Url>) -> Vec<LogoVariant> {
    let mut logos = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    let mut add = |url: String, kind: &str| {
        if !url.is_empty() && seen.insert(url.clone()) {
            logos.push(LogoVariant {
                url,
                kind: kind.to_string(),
            });
        }
    };

    #[allow(clippy::collapsible_if)]
    for el in doc.select(sel!("link[rel]")) {
        let rel = el.value().attr("rel").unwrap_or("").to_lowercase();
        if let Some(href) = el.value().attr("href") {
            if rel.contains("icon") && !rel.contains("apple") {
                add(resolve_url(href, base_url), "favicon");
            }
        }
    }

    for el in doc.select(sel!("link[rel='apple-touch-icon']")) {
        if let Some(href) = el.value().attr("href") {
            add(resolve_url(href, base_url), "apple-touch-icon");
        }
    }

    for el in doc.select(sel!("header img, nav img")) {
        let class = el.value().attr("class").unwrap_or("");
        let id = el.value().attr("id").unwrap_or("");
        let alt = el.value().attr("alt").unwrap_or("");
        if (ci_contains(class, "logo") || ci_contains(id, "logo") || ci_contains(alt, "logo"))
            && let Some(src) = el.value().attr("src")
        {
            add(resolve_url(src, base_url), "logo");
        }
    }

    logos
}

// ── Brand name ───────────────────────────────────────────────────────────────

fn extract_brand_name(doc: &Html) -> Option<String> {
    for el in doc.select(sel!("meta[property='og:site_name']")) {
        if let Some(c) = el.value().attr("content") {
            let n = c.trim();
            if !n.is_empty() {
                return Some(n.to_string());
            }
        }
    }

    for el in doc.select(sel!("meta[name='application-name']")) {
        if let Some(c) = el.value().attr("content") {
            let n = c.trim();
            if !n.is_empty() {
                return Some(n.to_string());
            }
        }
    }

    for el in doc.select(sel!("title")) {
        let title: String = el.text().collect();
        let t = title.trim();
        if !t.is_empty() {
            return Some(clean_title(t));
        }
    }

    None
}

fn clean_title(title: &str) -> String {
    for sep in [" | ", " - ", " — ", " · "] {
        if let Some(pos) = title.find(sep) {
            let left = title[..pos].trim();
            let right = title[pos + sep.len()..].trim();
            if right.len() < left.len() && right.len() >= 2 {
                return right.to_string();
            }
            return left.to_string();
        }
    }
    title.to_string()
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn ci_contains(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

fn resolve_url(src: &str, base_url: Option<&Url>) -> String {
    match base_url {
        Some(base) => base
            .join(src)
            .map(|u| u.to_string())
            .unwrap_or_else(|_| src.to_string()),
        None => src.to_string(),
    }
}

#[cfg(test)]
#[path = "brand_tests.rs"]
mod tests;
