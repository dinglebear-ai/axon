//! Color extraction helpers for the brand service.
//!
//! Extracts dominant hex colors from CSS declarations, filters boring/common
//! colors, and classifies each color by usage (primary/secondary/background/text/accent).

use std::collections::HashMap;

use super::CssDecl;
use crate::services::types::{BrandColor, ColorUsage};

pub(super) const BORING_COLORS: &[&str] = &[
    "#FFFFFF", "#000000", "#F8F8F8", "#F5F5F5", "#EEEEEE", "#E5E5E5", "#DDDDDD", "#D4D4D4",
    "#CCCCCC", "#BBBBBB", "#AAAAAA", "#999999", "#888888", "#777777", "#666666", "#555555",
    "#444444", "#333333", "#222222", "#111111", "#F0F0F0", "#E0E0E0", "#D0D0D0", "#C0C0C0",
    "#B0B0B0", "#A0A0A0", "#909090", "#808080", "#FAFAFA", "#F9F9F9", "#F7F7F7", "#F4F4F4",
    "#EFEFEF",
];

const GOOGLE_OAUTH_COLORS: &[&str] = &[
    "#1A73E8", "#4285F4", "#34A853", "#FBBC05", "#EA4335", "#5F6368", "#202124",
];

pub(super) fn extract_colors(decls: &[CssDecl], brand_name: Option<&str>) -> Vec<BrandColor> {
    let mut counts: HashMap<String, HashMap<ColorUsage, usize>> = HashMap::new();

    for decl in decls {
        let usage = classify_property(decl.property.as_str());
        for hex in parse_colors_from_value(&decl.value) {
            if BORING_COLORS.contains(&hex.as_str()) {
                continue;
            }
            *counts
                .entry(hex)
                .or_default()
                .entry(usage.clone())
                .or_insert(0) += 1;
        }
    }

    let mut colors: Vec<BrandColor> = counts
        .into_iter()
        .map(|(hex, usage_map)| {
            let total: usize = usage_map.values().sum();
            let usage = usage_map
                .into_iter()
                .max_by_key(|(_, c)| *c)
                .map(|(u, _)| u)
                .unwrap_or(ColorUsage::Unknown);
            BrandColor {
                hex,
                usage,
                count: total,
            }
        })
        .collect();
    colors.sort_by_key(|c| std::cmp::Reverse(c.count));

    // Remove Google OAuth palette from non-Google brands
    let brand = brand_name.unwrap_or("").to_ascii_lowercase();
    if !brand.contains("google") {
        let google_hits = colors
            .iter()
            .filter(|c| GOOGLE_OAUTH_COLORS.contains(&c.hex.as_str()))
            .count();
        if google_hits >= 3 {
            colors.retain(|c| !GOOGLE_OAUTH_COLORS.contains(&c.hex.as_str()));
        }
    }

    // Assign Primary/Secondary to top Unknown colors
    let mut primary_assigned = colors.iter().any(|c| c.usage == ColorUsage::Primary);
    let mut secondary_assigned = colors.iter().any(|c| c.usage == ColorUsage::Secondary);
    for color in &mut colors {
        if color.usage != ColorUsage::Unknown {
            continue;
        }
        if !primary_assigned {
            color.usage = ColorUsage::Primary;
            primary_assigned = true;
        } else if !secondary_assigned {
            color.usage = ColorUsage::Secondary;
            secondary_assigned = true;
        }
    }

    colors.truncate(10);
    colors
}

pub(super) fn classify_property(property: &str) -> ColorUsage {
    match property {
        "background-color" | "background" => ColorUsage::Background,
        "color" => ColorUsage::Text,
        "border-color" | "border" | "outline-color" => ColorUsage::Accent,
        _ => ColorUsage::Unknown,
    }
}

pub(super) fn parse_colors_from_value(value: &str) -> Vec<String> {
    use super::{HEX_COLOR, HSL_COLOR, RGB_COLOR, RGBA_COLOR};

    let mut colors = Vec::new();

    for cap in HEX_COLOR.captures_iter(value) {
        if let Some(short) = cap.get(1) {
            colors.push(expand_short_hex(short.as_str()));
        } else if let Some(full) = cap.get(2) {
            colors.push(format!("#{}", full.as_str().to_ascii_uppercase()));
        }
    }

    for cap in RGB_COLOR.captures_iter(value) {
        let r = parse_u8_clamp(&cap[1]);
        let g = parse_u8_clamp(&cap[2]);
        let b = parse_u8_clamp(&cap[3]);
        colors.push(format!("#{r:02X}{g:02X}{b:02X}"));
    }

    for cap in RGBA_COLOR.captures_iter(value) {
        let r = parse_u8_clamp(&cap[1]);
        let g = parse_u8_clamp(&cap[2]);
        let b = parse_u8_clamp(&cap[3]);
        colors.push(format!("#{r:02X}{g:02X}{b:02X}"));
    }

    for cap in HSL_COLOR.captures_iter(value) {
        // Hue wraps modulo 360 per CSS spec; saturation and lightness clamp to [0,1].
        let h = cap[1].parse::<f64>().unwrap_or(0.0).rem_euclid(360.0);
        let s = (cap[2].parse::<f64>().unwrap_or(0.0) / 100.0).clamp(0.0, 1.0);
        let l = (cap[3].parse::<f64>().unwrap_or(0.0) / 100.0).clamp(0.0, 1.0);
        let (r, g, b) = hsl_to_rgb(h, s, l);
        colors.push(format!("#{r:02X}{g:02X}{b:02X}"));
    }

    colors
}

/// Parse a CSS component string as a u8, clamping out-of-range values to 255
/// instead of silently wrapping to 0 via `parse::<u8>().unwrap_or(0)`.
fn parse_u8_clamp(s: &str) -> u8 {
    s.parse::<u16>().map(|v| v.min(255) as u8).unwrap_or(0)
}

fn expand_short_hex(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    format!("#{0}{0}{1}{1}{2}{2}", chars[0], chars[1], chars[2]).to_ascii_uppercase()
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let h = h / 360.0;
    let r = hue_to_rgb(p, q, h + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h);
    let b = hue_to_rgb(p, q, h - 1.0 / 3.0);
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 0.5 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}
