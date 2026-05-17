use std::borrow::Cow;

use gpui::App;

/// Aurora design token palette.
///
/// Grouped colour and typography tokens used across the palette UI. Kept as a
/// `const` struct so the values are still inlined by the compiler — there is
/// no runtime indirection compared to free constants. Existing call sites use
/// the `AURORA_*` re-exports below; new code should prefer `AURORA.foo`.
#[allow(dead_code)]
pub(crate) struct AuroraTokens {
    // Surfaces
    pub page_bg: u32,
    pub nav_bg: u32,
    pub panel_strong: u32,
    pub panel_medium: u32,
    pub control_surface: u32,
    pub hover_bg: u32,
    // Borders
    pub border_default: u32,
    pub border_strong: u32,
    // Text
    pub text_primary: u32,
    pub text_muted: u32,
    // Accents
    pub accent_primary: u32,
    pub accent_strong: u32,
    pub accent_pink: u32,
    // Output panel
    pub output_text: u32,
    pub output_muted: u32,
    // Status
    pub success: u32,
    pub warn: u32,
    pub error: u32,
    // Typography families
    pub font_display: &'static str,
    pub font_sans: &'static str,
    pub font_mono: &'static str,
}

pub(crate) const AURORA: AuroraTokens = AuroraTokens {
    page_bg: 0x07131c,
    nav_bg: 0x07111a,
    panel_strong: 0x13293a,
    panel_medium: 0x102330,
    control_surface: 0x0c1a24,
    hover_bg: 0x17364b,
    border_default: 0x1d3d4e,
    border_strong: 0x24536c,
    text_primary: 0xe6f4fb,
    text_muted: 0xa7bcc9,
    accent_primary: 0x29b6f6,
    accent_strong: 0x67cbfa,
    accent_pink: 0xf9a8c4,
    output_text: 0xd7e7ef,
    output_muted: 0x8aa3b2,
    success: 0x7dd3c7,
    warn: 0xc6a36b,
    error: 0xc78490,
    font_display: "Manrope",
    font_sans: "Inter",
    font_mono: "JetBrains Mono",
};

// Re-exports kept so the 77 existing call sites continue to compile. Prefer
// `AURORA.foo` in new code.
pub(crate) const AURORA_PAGE_BG: u32 = AURORA.page_bg;
pub(crate) const AURORA_NAV_BG: u32 = AURORA.nav_bg;
pub(crate) const AURORA_PANEL_STRONG: u32 = AURORA.panel_strong;
pub(crate) const AURORA_PANEL_MEDIUM: u32 = AURORA.panel_medium;
pub(crate) const AURORA_CONTROL_SURFACE: u32 = AURORA.control_surface;
pub(crate) const AURORA_HOVER_BG: u32 = AURORA.hover_bg;
pub(crate) const AURORA_BORDER_DEFAULT: u32 = AURORA.border_default;
pub(crate) const AURORA_BORDER_STRONG: u32 = AURORA.border_strong;
pub(crate) const AURORA_TEXT_PRIMARY: u32 = AURORA.text_primary;
pub(crate) const AURORA_TEXT_MUTED: u32 = AURORA.text_muted;
pub(crate) const AURORA_ACCENT_PRIMARY: u32 = AURORA.accent_primary;
pub(crate) const AURORA_ACCENT_STRONG: u32 = AURORA.accent_strong;
pub(crate) const AURORA_ACCENT_PINK: u32 = AURORA.accent_pink;
pub(crate) const AURORA_OUTPUT_TEXT: u32 = AURORA.output_text;
pub(crate) const AURORA_OUTPUT_MUTED: u32 = AURORA.output_muted;
pub(crate) const AURORA_SUCCESS: u32 = AURORA.success;
pub(crate) const AURORA_WARN: u32 = AURORA.warn;
pub(crate) const AURORA_ERROR: u32 = AURORA.error;
pub(crate) const AURORA_WARNING: u32 = AURORA.warn;

pub(crate) const AURORA_FONT_DISPLAY: &str = AURORA.font_display;
pub(crate) const AURORA_FONT_SANS: &str = AURORA.font_sans;
pub(crate) const AURORA_FONT_MONO: &str = AURORA.font_mono;

const AURORA_MANROPE_LATIN: &[u8] = include_bytes!("../assets/fonts/Manrope-latin.woff2");
const AURORA_INTER_LATIN: &[u8] = include_bytes!("../assets/fonts/Inter-latin.woff2");
const AURORA_JETBRAINS_MONO_LATIN: &[u8] =
    include_bytes!("../assets/fonts/JetBrainsMono-latin.woff2");

pub(crate) fn register_bundled_fonts(cx: &mut App) {
    let fonts = [
        AURORA_MANROPE_LATIN,
        AURORA_INTER_LATIN,
        AURORA_JETBRAINS_MONO_LATIN,
    ]
    .iter()
    .map(|font| Cow::Borrowed(*font))
    .collect();

    if let Err(err) = cx.text_system().add_fonts(fonts) {
        tracing::warn!("failed to register bundled Aurora fonts: {err}");
    }
}
