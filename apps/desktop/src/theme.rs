use std::borrow::Cow;

use gpui::App;

pub(crate) const AURORA_PAGE_BG: u32 = 0x07131c;
pub(crate) const AURORA_NAV_BG: u32 = 0x07111a;
pub(crate) const AURORA_PANEL_STRONG: u32 = 0x13293a;
pub(crate) const AURORA_CONTROL_SURFACE: u32 = 0x0c1a24;
pub(crate) const AURORA_HOVER_BG: u32 = 0x17364b;
pub(crate) const AURORA_ROW_HOVER_BG: u32 = 0x14283a;
pub(crate) const AURORA_PRESSED_BG: u32 = 0x1f4763;
pub(crate) const AURORA_BORDER_DEFAULT: u32 = 0x1d3d4e;
pub(crate) const AURORA_BORDER_STRONG: u32 = 0x24536c;
pub(crate) const AURORA_TEXT_PRIMARY: u32 = 0xe6f4fb;
pub(crate) const AURORA_TEXT_MUTED: u32 = 0xa7bcc9;
pub(crate) const AURORA_ACCENT_PRIMARY: u32 = 0x29b6f6;
pub(crate) const AURORA_ACCENT_STRONG: u32 = 0x67cbfa;
pub(crate) const AURORA_PANEL_MEDIUM: u32 = 0x102330;
pub(crate) const AURORA_ACCENT_PINK: u32 = 0xf9a8c4;
pub(crate) const AURORA_OUTPUT_TEXT: u32 = 0xd7e7ef;
pub(crate) const AURORA_OUTPUT_MUTED: u32 = 0x8aa3b2;
pub(crate) const AURORA_SUCCESS: u32 = 0x7dd3c7;
pub(crate) const AURORA_WARN: u32 = 0xc6a36b;
pub(crate) const AURORA_ERROR: u32 = 0xc78490;
pub(crate) const AURORA_WARNING: u32 = AURORA_WARN;

pub(crate) const AURORA_FONT_DISPLAY: &str = "Manrope";
pub(crate) const AURORA_FONT_SANS: &str = "Inter";
pub(crate) const AURORA_FONT_MONO: &str = "JetBrains Mono";

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
