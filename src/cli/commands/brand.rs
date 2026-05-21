use crate::core::config::Config;
use crate::core::logging::{log_done, log_info};
use crate::core::ui::{accent, muted, primary, print_option, print_phase};
use crate::services::brand as brand_svc;
use crate::services::types::{BrandResult, ColorUsage};
use std::error::Error;

pub async fn run_brand(cfg: &Config) -> Result<(), Box<dyn Error>> {
    let url = parse_brand_url(cfg)?;

    log_info(&format!("command=brand url={url}"));
    let result = brand_svc::brand(cfg, &url, None).await?;

    emit_brand_result(cfg, &result)?;

    log_done(&format!(
        "command=brand url={url} colors={} fonts={} logos={}",
        result.colors.len(),
        result.fonts.len(),
        result.logos.len(),
    ));
    Ok(())
}

fn parse_brand_url(cfg: &Config) -> Result<String, Box<dyn Error>> {
    if let Some(url) = cfg.positional.first().cloned() {
        return Ok(url);
    }
    if !cfg.start_url.is_empty() {
        return Ok(cfg.start_url.clone());
    }
    Err("brand requires a URL: axon brand <url>".into())
}

pub(crate) fn emit_brand_result(cfg: &Config, result: &BrandResult) -> Result<(), Box<dyn Error>> {
    if cfg.json_output {
        println!("{}", serde_json::to_string_pretty(result)?);
        return Ok(());
    }

    print_phase("◐", "Brand", &result.url);
    if let Some(ref name) = result.name {
        print_option("name", name);
    }
    print_option("colors", &result.colors.len().to_string());
    print_option("fonts", &result.fonts.len().to_string());
    print_option("logos", &result.logos.len().to_string());

    if !result.colors.is_empty() {
        println!("\n{}", primary("Brand Colors"));
        for color in &result.colors {
            let usage_label = match color.usage {
                ColorUsage::Primary => "primary",
                ColorUsage::Secondary => "secondary",
                ColorUsage::Background => "background",
                ColorUsage::Text => "text",
                ColorUsage::Accent => "accent",
                ColorUsage::Unknown => "unknown",
            };
            println!(
                "  {} {} {} ({})",
                muted("•"),
                accent(&color.hex),
                muted(usage_label),
                color.count
            );
        }
    }

    if !result.fonts.is_empty() {
        println!("\n{}", primary("Fonts"));
        for font in &result.fonts {
            println!("  {} {}", muted("•"), font);
        }
    }

    if let Some(ref logo) = result.logo_url {
        println!("\n{}", primary("Logo"));
        println!("  {logo}");
    }

    if let Some(ref favicon) = result.favicon_url {
        println!("\n{}", primary("Favicon"));
        println!("  {favicon}");
    }

    if !result.logos.is_empty() {
        println!("\n{}", primary("All Logo Variants"));
        for logo in &result.logos {
            println!("  {} {} ({})", muted("•"), logo.url, logo.kind);
        }
    }

    Ok(())
}

/// Pure formatting helper exposed for testing.
#[cfg(test)]
pub(crate) fn format_brand_summary(result: &BrandResult) -> String {
    let name = result.name.as_deref().unwrap_or("(unknown)");
    let fonts = result.fonts.join(", ");
    format!(
        "name={name} colors={} fonts=[{fonts}] logos={}",
        result.colors.len(),
        result.logos.len(),
    )
}

#[cfg(test)]
#[path = "brand_tests.rs"]
mod tests;
