// ── brand ────────────────────────────────────────────────────────────────────

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, utoipa::ToSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum ColorUsage {
    Primary,
    Secondary,
    Background,
    Text,
    Accent,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct BrandColor {
    pub hex: String,
    pub usage: ColorUsage,
    pub count: usize,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct LogoVariant {
    pub url: String,
    /// "favicon" | "apple-touch-icon" | "logo" | "og-image" | "svg"
    pub kind: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct BrandResult {
    pub url: String,
    pub name: Option<String>,
    pub colors: Vec<BrandColor>,
    pub fonts: Vec<String>,
    pub logos: Vec<LogoVariant>,
    pub logo_url: Option<String>,
    pub favicon_url: Option<String>,
    pub og_image: Option<String>,
}
