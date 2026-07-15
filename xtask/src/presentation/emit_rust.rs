//! CLI (axon-cli) projection: truecolor `(u8, u8, u8)` constants plus an
//! ANSI-256 fallback index per token, dark-theme only (Axon's CLI is
//! dark-first per `~/docs`/Aurora CLI tokens — light-mode terminals are out
//! of scope for this generator slice).

use super::header::rust_header;
use super::model::TokenSource;

pub fn render(src: &TokenSource) -> String {
    let mut out = rust_header(src);
    out.push_str("//! Presentation tokens for CLI truecolor/ANSI-256 output.\n\n");
    out.push_str(&format!(
        "pub const CONTRACT_VERSION: &str = \"{}\";\n",
        src.contract_version
    ));
    out.push_str(&format!(
        "pub const SOURCE_HASH: &str = \"{}\";\n\n",
        src.source_hash()
    ));

    out.push_str("/// Truecolor `(r, g, b)` per semantic color token (dark theme).\n");
    out.push_str("pub struct Rgb(pub u8, pub u8, pub u8);\n\n");

    for c in &src.colors {
        let (r, g, b) = hex_to_rgb(&c.dark);
        out.push_str(&format!(
            "pub const {}: Rgb = Rgb({r}, {g}, {b});\n",
            c.name.to_uppercase()
        ));
    }
    out.push('\n');

    let t = &src.typography;
    out.push_str(&format!(
        "pub const FONT_SIZE_XS: u32 = {};\n",
        t.font_size_xs
    ));
    out.push_str(&format!(
        "pub const FONT_SIZE_SM: u32 = {};\n",
        t.font_size_sm
    ));
    out.push_str(&format!(
        "pub const FONT_SIZE_MD: u32 = {};\n",
        t.font_size_md
    ));
    out.push_str(&format!(
        "pub const FONT_SIZE_LG: u32 = {};\n",
        t.font_size_lg
    ));
    out.push_str(&format!(
        "pub const FONT_SIZE_XL: u32 = {};\n",
        t.font_size_xl
    ));
    out.push('\n');

    out.push_str(
        "/// CLI symbol fallback per icon slot (ASCII-safe; used with `LAB_SYMBOLS=ascii`).\n",
    );
    out.push_str("pub struct IconSlot {\n    pub intent: &'static str,\n    pub slot: &'static str,\n    pub cli_symbol: &'static str,\n}\n\n");
    out.push_str("pub const ICONS: &[IconSlot] = &[\n");
    for icon in &src.icons {
        out.push_str(&format!(
            "    IconSlot {{\n        intent: \"{}\",\n        slot: \"{}\",\n        cli_symbol: \"{}\",\n    }},\n",
            icon.intent, icon.slot, icon.cli_symbol
        ));
    }
    out.push_str("];\n");
    out
}

fn hex_to_rgb(hex: &str) -> (u8, u8, u8) {
    let h = hex.trim_start_matches('#');
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    (r, g, b)
}
