//! Canonical presentation-token source model: parses `source.json` (the
//! single canonical token source per docs/pipeline-unification/surfaces/
//! presentation-contract.md) and computes the contract-version/source-hash
//! pair every generated artifact must carry.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The raw canonical source, embedded at compile time so `cargo xtask
/// presentation generate` never depends on a runtime-relative path.
pub const SOURCE_JSON: &str = include_str!("source.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorToken {
    pub name: String,
    pub dark: String,
    pub light: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Typography {
    pub font_family_sans: String,
    pub font_family_mono: String,
    pub font_family_display: String,
    pub font_size_xs: u32,
    pub font_size_sm: u32,
    pub font_size_md: u32,
    pub font_size_lg: u32,
    pub font_size_xl: u32,
    pub font_weight_regular: u32,
    pub font_weight_medium: u32,
    pub font_weight_semibold: u32,
    pub line_height_tight: f64,
    pub line_height_normal: f64,
    pub line_height_relaxed: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusMapping {
    pub status: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IconSlot {
    pub intent: String,
    pub slot: String,
    pub cli_symbol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSource {
    pub contract_version: String,
    pub source_doc: String,
    pub colors: Vec<ColorToken>,
    pub typography: Typography,
    pub spacing: std::collections::BTreeMap<String, u32>,
    pub radius: std::collections::BTreeMap<String, u32>,
    pub density: std::collections::BTreeMap<String, String>,
    pub status_mapping: Vec<StatusMapping>,
    pub icons: Vec<IconSlot>,
}

impl TokenSource {
    pub fn load() -> Result<Self> {
        serde_json::from_str(SOURCE_JSON).context("parsing xtask/src/presentation/source.json")
    }

    /// Short (first 12 hex chars) sha256 of the raw source bytes, embedded in
    /// every generated file's header per the contract's "must include a
    /// contract version and source hash" requirement.
    pub fn source_hash(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(SOURCE_JSON.as_bytes());
        let digest = hasher.finalize();
        hex_prefix(&digest, 12)
    }
}

fn hex_prefix(bytes: &[u8], chars: usize) -> String {
    let mut out = String::with_capacity(chars);
    for byte in bytes {
        if out.len() >= chars {
            break;
        }
        out.push_str(&format!("{byte:02x}"));
    }
    out.truncate(chars);
    out
}
