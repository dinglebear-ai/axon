//! CSS custom-property emitter shared by web, Palette, and the Chrome
//! extension — all three consume the same axon-tokens.css shape and are
//! expected to load Aurora's own `aurora.css` alongside it.

use super::header::css_header;
use super::model::TokenSource;

pub fn render(src: &TokenSource) -> String {
    let mut out = css_header(src);
    out.push_str(":root,\n.dark {\n");
    for c in &src.colors {
        out.push_str(&format!("  --axon-color-{}: {};\n", dash(&c.name), c.dark));
    }
    push_shared_vars(&mut out, src);
    out.push_str("}\n\n.light {\n");
    for c in &src.colors {
        out.push_str(&format!("  --axon-color-{}: {};\n", dash(&c.name), c.light));
    }
    out.push_str("}\n");
    out
}

fn push_shared_vars(out: &mut String, src: &TokenSource) {
    let t = &src.typography;
    out.push_str(&format!("  --axon-font-sans: {};\n", t.font_family_sans));
    out.push_str(&format!("  --axon-font-mono: {};\n", t.font_family_mono));
    out.push_str(&format!(
        "  --axon-font-display: {};\n",
        t.font_family_display
    ));
    out.push_str(&format!("  --axon-font-size-xs: {}px;\n", t.font_size_xs));
    out.push_str(&format!("  --axon-font-size-sm: {}px;\n", t.font_size_sm));
    out.push_str(&format!("  --axon-font-size-md: {}px;\n", t.font_size_md));
    out.push_str(&format!("  --axon-font-size-lg: {}px;\n", t.font_size_lg));
    out.push_str(&format!("  --axon-font-size-xl: {}px;\n", t.font_size_xl));
    out.push_str(&format!(
        "  --axon-font-weight-regular: {};\n",
        t.font_weight_regular
    ));
    out.push_str(&format!(
        "  --axon-font-weight-medium: {};\n",
        t.font_weight_medium
    ));
    out.push_str(&format!(
        "  --axon-font-weight-semibold: {};\n",
        t.font_weight_semibold
    ));
    out.push_str(&format!(
        "  --axon-line-height-tight: {};\n",
        t.line_height_tight
    ));
    out.push_str(&format!(
        "  --axon-line-height-normal: {};\n",
        t.line_height_normal
    ));
    out.push_str(&format!(
        "  --axon-line-height-relaxed: {};\n",
        t.line_height_relaxed
    ));
    for (k, v) in &src.spacing {
        out.push_str(&format!("  --axon-{}: {}px;\n", dash(k), v));
    }
    for (k, v) in &src.radius {
        out.push_str(&format!("  --axon-radius-{}: {}px;\n", dash(k), v));
    }
}

fn dash(name: &str) -> String {
    name.replace('_', "-")
}
