//! Font extraction helpers for the brand service.
//!
//! Extracts brand-specific font families from CSS declarations,
//! filtering out generic/common system fonts.

use std::collections::HashMap;

use super::{CssDecl, FONT_SHORTHAND_FAMILY};

const GENERIC_FONTS: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
    "emoji",
    "math",
    "fangsong",
    "inherit",
    "initial",
    "unset",
    "revert",
    "arial",
    "times",
    "times new roman",
    "courier new",
    "georgia",
    "menlo",
    "monaco",
    "consolas",
    "liberation mono",
    "sf mono",
    "sfmono-regular",
    "source code pro",
    "apple color emoji",
    "segoe ui",
    "segoe ui emoji",
    "segoe ui symbol",
    "noto color emoji",
    "blinkmacsystemfont",
    "-apple-system",
];

pub(super) fn extract_fonts(decls: &[CssDecl], brand_name: Option<&str>) -> Vec<String> {
    let mut freq: HashMap<String, usize> = HashMap::new();
    let brand = brand_name.unwrap_or("").to_ascii_lowercase();

    for decl in decls {
        if decl.property != "font-family" && decl.property != "font" {
            continue;
        }

        let family_str = if decl.property == "font" {
            match parse_font_shorthand_family(&decl.value) {
                Some(f) => f,
                None => continue,
            }
        } else {
            decl.value.clone()
        };

        for font in split_font_families(&family_str) {
            let lower = font.to_lowercase();
            if !GENERIC_FONTS.contains(&lower.as_str())
                && !is_junk_font(&lower)
                && (!brand.contains("google") || !lower.contains("google sans"))
            {
                *freq.entry(font).or_insert(0) += 1;
            }
        }
    }

    let mut fonts: Vec<(String, usize)> = freq.into_iter().collect();
    fonts.sort_by_key(|f| std::cmp::Reverse(f.1));
    fonts.into_iter().map(|(name, _)| name).collect()
}

fn is_junk_font(name: &str) -> bool {
    if name.starts_with("var(") {
        return true;
    }
    if name.len() >= 8 && name.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    if name.len() < 3 {
        return true;
    }
    if name.contains("katex")
        || name.contains("icon")
        || name.contains("emoji")
        || name.contains("symbol")
    {
        return true;
    }
    if name.contains(')') || name.contains('!') || name.contains("px ") || name.contains("rem ") {
        return true;
    }
    if name.starts_with('_') || name.starts_with("--") {
        return true;
    }
    false
}

fn parse_font_shorthand_family(value: &str) -> Option<String> {
    let caps = FONT_SHORTHAND_FAMILY.captures(value)?;
    let family = caps.get(1)?.as_str().trim().to_string();
    if family.is_empty() {
        None
    } else {
        Some(family)
    }
}

fn split_font_families(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|s| {
            s.trim()
                .trim_matches('"')
                .trim_matches('\'')
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .collect()
}

pub(super) fn extract_font_name_from_url(url: &str) -> Option<String> {
    let filename = url.rsplit('/').next()?;
    let stem = filename.split('.').next()?;
    let clean = stem
        .split('-')
        .take_while(|p| {
            !matches!(
                p.to_lowercase().as_str(),
                "regular"
                    | "bold"
                    | "italic"
                    | "light"
                    | "medium"
                    | "semibold"
                    | "variable"
                    | "subset"
                    | "latin"
            )
        })
        .collect::<Vec<_>>()
        .join(" ");
    if clean.len() < 2 { None } else { Some(clean) }
}

pub(super) fn extract_google_fonts_from_url(url: &str) -> Vec<String> {
    let mut fonts = Vec::new();
    for part in url.split('&') {
        let family = if let Some(rest) = part.strip_prefix("family=") {
            rest
        } else if let Some(rest) = part.split("family=").nth(1) {
            rest
        } else {
            continue;
        };
        let name = family.split(':').next().unwrap_or(family);
        let clean = name.replace('+', " ");
        if !clean.is_empty() {
            fonts.push(clean);
        }
    }
    fonts
}
