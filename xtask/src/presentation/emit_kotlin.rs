//! Android Kotlin projection: an `AxonTokens` object exposing dark/light
//! `Color` maps plus typography/spacing/radius constants. Consumption is not
//! wired in this slice (see the presentation README) — this is emitted
//! alongside the hand-written `AxonTheme.kt` for a follow-up migration.

use super::header::kotlin_header;
use super::model::TokenSource;

pub const PACKAGE: &str = "com.axon.app.ui.theme.generated";

pub fn render(src: &TokenSource) -> String {
    let mut out = kotlin_header(src);
    out.push_str(&format!("package {PACKAGE}\n\n"));
    out.push_str("import androidx.compose.runtime.Immutable\n");
    out.push_str("import androidx.compose.ui.graphics.Color\n\n");

    out.push_str("@Immutable\ndata class AxonTokenColors(\n");
    for c in &src.colors {
        out.push_str(&format!("    val {}: Color,\n", camel(&c.name)));
    }
    out.push_str(")\n\n");

    render_palette(&mut out, src, "AxonTokenColorsDark", true);
    render_palette(&mut out, src, "AxonTokenColorsLight", false);

    out.push_str("object AxonTokens {\n");
    out.push_str(&format!(
        "    const val CONTRACT_VERSION: String = \"{}\"\n",
        src.contract_version
    ));
    out.push_str(&format!(
        "    const val SOURCE_HASH: String = \"{}\"\n",
        src.source_hash()
    ));
    out.push_str("    val dark: AxonTokenColors = AxonTokenColorsDark\n");
    out.push_str("    val light: AxonTokenColors = AxonTokenColorsLight\n\n");

    let t = &src.typography;
    out.push_str(&format!(
        "    const val FONT_SIZE_XS_SP: Float = {}f\n",
        t.font_size_xs
    ));
    out.push_str(&format!(
        "    const val FONT_SIZE_SM_SP: Float = {}f\n",
        t.font_size_sm
    ));
    out.push_str(&format!(
        "    const val FONT_SIZE_MD_SP: Float = {}f\n",
        t.font_size_md
    ));
    out.push_str(&format!(
        "    const val FONT_SIZE_LG_SP: Float = {}f\n",
        t.font_size_lg
    ));
    out.push_str(&format!(
        "    const val FONT_SIZE_XL_SP: Float = {}f\n",
        t.font_size_xl
    ));
    out.push_str(&format!(
        "    const val FONT_WEIGHT_REGULAR: Int = {}\n",
        t.font_weight_regular
    ));
    out.push_str(&format!(
        "    const val FONT_WEIGHT_MEDIUM: Int = {}\n",
        t.font_weight_medium
    ));
    out.push_str(&format!(
        "    const val FONT_WEIGHT_SEMIBOLD: Int = {}\n",
        t.font_weight_semibold
    ));
    out.push_str(&format!(
        "    const val LINE_HEIGHT_TIGHT: Float = {}f\n",
        t.line_height_tight
    ));
    out.push_str(&format!(
        "    const val LINE_HEIGHT_NORMAL: Float = {}f\n",
        t.line_height_normal
    ));
    out.push_str(&format!(
        "    const val LINE_HEIGHT_RELAXED: Float = {}f\n",
        t.line_height_relaxed
    ));

    for (k, v) in &src.spacing {
        out.push_str(&format!(
            "    const val {}_DP: Int = {}\n",
            k.to_uppercase(),
            v
        ));
    }
    for (k, v) in &src.radius {
        out.push_str(&format!(
            "    const val RADIUS_{}_DP: Int = {}\n",
            k.to_uppercase(),
            v
        ));
    }
    out.push_str("}\n");
    out
}

fn render_palette(out: &mut String, src: &TokenSource, name: &str, dark: bool) {
    out.push_str(&format!("internal val {name} = AxonTokenColors(\n"));
    for c in &src.colors {
        let hex = if dark { &c.dark } else { &c.light };
        out.push_str(&format!(
            "    {} = Color(0xFF{}),\n",
            camel(&c.name),
            hex.trim_start_matches('#').to_uppercase()
        ));
    }
    out.push_str(")\n\n");
}

fn camel(name: &str) -> String {
    let mut result = String::new();
    let mut upper_next = false;
    for ch in name.chars() {
        if ch == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            result.extend(ch.to_uppercase());
            upper_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}
