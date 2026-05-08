use crate::core::config::Config;
use console::{Style, style};
use dialoguer::{Confirm, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use std::error::Error;
use std::time::Duration;

pub struct Spinner {
    bar: ProgressBar,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.enable_steady_tick(Duration::from_millis(100));
        bar.set_style(
            ProgressStyle::with_template("{spinner:.cyan} {msg}")
                .unwrap_or_else(|_| ProgressStyle::default_spinner()),
        );
        bar.set_message(message.to_string());
        Self { bar }
    }

    pub fn finish(&self, message: &str) {
        self.bar.finish_with_message(message.to_string());
    }
}

pub fn confirm_destructive(cfg: &Config, prompt: &str) -> Result<bool, Box<dyn Error>> {
    if cfg.yes || !console::Term::stderr().is_term() {
        return Ok(true);
    }

    let proceed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("{} {}", style("[confirm]").yellow().bold(), prompt))
        .default(false)
        .interact()?;
    Ok(proceed)
}

pub fn primary(text: &str) -> String {
    Style::new().color256(211).bold().apply_to(text).to_string()
}

pub fn accent(text: &str) -> String {
    // #87afff — web UI primary blue
    Style::new().color256(111).apply_to(text).to_string()
}

pub fn success(text: &str) -> String {
    Style::new().green().bold().apply_to(text).to_string()
}

pub fn warning(text: &str) -> String {
    Style::new().yellow().bold().apply_to(text).to_string()
}

pub fn muted(text: &str) -> String {
    Style::new().dim().apply_to(text).to_string()
}

/// Soft blue for secondary info (UUIDs, ages, separators) — visible but not dominant.
pub fn subtle(text: &str) -> String {
    // #87afd7 — muted blue, more vibrant than the prior grayish periwinkle
    Style::new().color256(110).apply_to(text).to_string()
}

pub fn symbol_for_status(status: &str) -> String {
    match status {
        "completed" => Style::new().green().apply_to("✓").to_string(),
        "failed" | "error" => Style::new().red().apply_to("✗").to_string(),
        "pending" | "running" | "processing" | "scraping" => {
            Style::new().yellow().apply_to("◐").to_string()
        }
        "canceled" => Style::new().yellow().apply_to("⚠").to_string(),
        _ => Style::new().cyan().apply_to("•").to_string(),
    }
}

pub fn status_text(status: &str) -> String {
    match status {
        "completed" => Style::new().green().apply_to(status).to_string(),
        "failed" | "error" => Style::new().red().apply_to(status).to_string(),
        "pending" | "running" | "processing" | "scraping" => {
            Style::new().yellow().apply_to(status).to_string()
        }
        "canceled" => Style::new().yellow().apply_to(status).to_string(),
        _ => Style::new().cyan().apply_to(status).to_string(),
    }
}

/// Like `status_text` but returns an empty string for terminal states —
/// ✓ and ✗ already communicate the outcome without words.
pub fn status_label(status: &str) -> String {
    match status {
        "completed" | "failed" | "error" => String::new(),
        _ => status_text(status),
    }
}

/// Blue number + blue label: "42 docs"
pub fn metric(value: impl std::fmt::Display, label: &str) -> String {
    format!("{} {}", accent(&value.to_string()), accent(label))
}

/// Red text for errors.
pub fn error(text: &str) -> String {
    Style::new().red().apply_to(text).to_string()
}

/// "error: <msg>" in red/bold on stderr — for CLI user-facing errors.
pub fn report_error(msg: &str) {
    eprintln!("{} {}", Style::new().red().bold().apply_to("error:"), msg);
}

/// "hint: <msg>" in cyan/dim on stderr — companion to report_error.
pub fn report_hint(msg: &str) {
    eprintln!("{} {}", Style::new().cyan().dim().apply_to("hint:"), msg);
}

pub fn print_phase(symbol: &str, action: &str, subject: &str) {
    println!("  {} {} {}", primary(symbol), action, muted(subject));
}

pub fn print_option(label: &str, value: &str) {
    println!("    {} {}", muted(&format!("{label}:")), value);
}

pub fn print_kv(label: &str, value: &str) {
    println!("{} {}", primary(label), value);
}
