//! Reference-doc emitter: `tokens.md` (human-readable registry) and
//! `tokens.schema.json` (JSON Schema validating `source.json`'s shape).

use super::header::markdown_header;
use super::model::TokenSource;

pub fn render_markdown(src: &TokenSource) -> String {
    let mut out = markdown_header(src);
    out.push_str("# Presentation Tokens\n\n");
    out.push_str(&format!(
        "Contract version `{}`, source hash `{}`. See [presentation-contract.md](../../pipeline-unification/surfaces/presentation-contract.md).\n\n",
        src.contract_version,
        src.source_hash()
    ));

    out.push_str("## Color Tokens\n\n| Token | Dark | Light | Note |\n|---|---|---|---|\n");
    for c in &src.colors {
        out.push_str(&format!(
            "| `color.{}` | `{}` | `{}` | {} |\n",
            c.name, c.dark, c.light, c.note
        ));
    }

    out.push_str("\n## Status Mapping\n\n| Status | Token |\n|---|---|\n");
    for m in &src.status_mapping {
        out.push_str(&format!("| `{}` | `color.{}` |\n", m.status, m.token));
    }

    let t = &src.typography;
    out.push_str("\n## Typography Tokens\n\n| Token | Value |\n|---|---|\n");
    out.push_str(&format!(
        "| `font.family.sans` | {} |\n",
        t.font_family_sans
    ));
    out.push_str(&format!(
        "| `font.family.mono` | {} |\n",
        t.font_family_mono
    ));
    out.push_str(&format!(
        "| `font.family.display` | {} |\n",
        t.font_family_display
    ));
    out.push_str(&format!("| `font.size.xs` | {}px |\n", t.font_size_xs));
    out.push_str(&format!("| `font.size.sm` | {}px |\n", t.font_size_sm));
    out.push_str(&format!("| `font.size.md` | {}px |\n", t.font_size_md));
    out.push_str(&format!("| `font.size.lg` | {}px |\n", t.font_size_lg));
    out.push_str(&format!("| `font.size.xl` | {}px |\n", t.font_size_xl));
    out.push_str(&format!(
        "| `font.weight.regular` | {} |\n",
        t.font_weight_regular
    ));
    out.push_str(&format!(
        "| `font.weight.medium` | {} |\n",
        t.font_weight_medium
    ));
    out.push_str(&format!(
        "| `font.weight.semibold` | {} |\n",
        t.font_weight_semibold
    ));
    out.push_str(&format!(
        "| `line_height.tight` | {} |\n",
        t.line_height_tight
    ));
    out.push_str(&format!(
        "| `line_height.normal` | {} |\n",
        t.line_height_normal
    ));
    out.push_str(&format!(
        "| `line_height.relaxed` | {} |\n",
        t.line_height_relaxed
    ));

    out.push_str("\n## Spacing / Radius / Density\n\n| Token | Value |\n|---|---|\n");
    for (k, v) in &src.spacing {
        out.push_str(&format!(
            "| `space.{}` | {}px |\n",
            k.trim_start_matches("space_"),
            v
        ));
    }
    for (k, v) in &src.radius {
        out.push_str(&format!("| `radius.{k}` | {v}px |\n"));
    }
    for (k, v) in &src.density {
        out.push_str(&format!("| `density.{k}` | {v} |\n"));
    }

    out.push_str("\n## Icon Slots\n\n| Intent | Slot | CLI Symbol |\n|---|---|---|\n");
    for icon in &src.icons {
        out.push_str(&format!(
            "| {} | `{}` | `{}` |\n",
            icon.intent, icon.slot, icon.cli_symbol
        ));
    }

    out.push_str("\n## Generated Artifacts\n\nSee `docs/reference/presentation/README.md` for consumption guidance and the full artifact list.\n");
    out
}

pub fn render_schema(_src: &TokenSource) -> String {
    // Static hand-maintained JSON Schema for source.json's shape (not derived
    // via schemars — the presentation source is data, not a Rust type used
    // elsewhere in the wire contract).
    serde_json::to_string_pretty(&schema_value()).expect("schema literal is valid json")
}

fn schema_value() -> serde_json::Value {
    serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "Axon Presentation Token Source",
        "type": "object",
        "required": ["contract_version", "source_doc", "colors", "typography", "spacing", "radius", "density", "status_mapping", "icons"],
        "properties": {
            "contract_version": { "type": "string" },
            "source_doc": { "type": "string" },
            "colors": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["name", "dark", "light", "note"],
                    "properties": {
                        "name": { "type": "string" },
                        "dark": { "type": "string", "pattern": "^#[0-9a-fA-F]{6}$" },
                        "light": { "type": "string", "pattern": "^#[0-9a-fA-F]{6}$" },
                        "note": { "type": "string" }
                    }
                }
            },
            "typography": { "type": "object" },
            "spacing": { "type": "object" },
            "radius": { "type": "object" },
            "density": { "type": "object" },
            "status_mapping": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["status", "token"],
                    "properties": {
                        "status": { "type": "string" },
                        "token": { "type": "string" }
                    }
                }
            },
            "icons": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["intent", "slot", "cli_symbol"],
                    "properties": {
                        "intent": { "type": "string" },
                        "slot": { "type": "string" },
                        "cli_symbol": { "type": "string" }
                    }
                }
            }
        }
    })
}
