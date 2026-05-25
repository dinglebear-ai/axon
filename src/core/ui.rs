use crate::core::config::Config;
use dialoguer::{Confirm, theme::ColorfulTheme};
use indicatif::{ProgressBar, ProgressStyle};
use std::env;
use std::error::Error;
use std::time::Duration;

// Aurora design tokens — keep in sync with
// aurora-design-system/registry/aurora/styles/aurora.css
pub const PRIMARY_ANSI: &str = "\x1b[38;2;249;168;196m"; // --aurora-accent-pink        #F9A8C4
pub const ACCENT_ANSI: &str = "\x1b[38;2;41;182;246m"; // --aurora-accent-primary     #29B6F6
const SUCCESS_ANSI: &str = "\x1b[38;2;125;211;199m"; // --aurora-success            #7DD3C7
const WARN_ANSI: &str = "\x1b[38;2;198;163;107m"; // --aurora-warn               #C6A36B
const ERROR_ANSI: &str = "\x1b[38;2;199;132;144m"; // --aurora-error              #C78490
const INFO_ANSI: &str = "\x1b[38;2;114;200;245m"; // --aurora-info               #72C8F5
const MUTED_ANSI: &str = "\x1b[38;2;167;188;201m"; // --aurora-text-muted         #A7BCC9
const SUBTLE_ANSI: &str = "\x1b[38;2;196;107;136m"; // --aurora-accent-pink-deep   #C46B88

fn color_enabled() -> bool {
    env::var_os("NO_COLOR").is_none()
}

pub fn ansi_colorize(code: &str, text: &str) -> String {
    if color_enabled() {
        format!("{code}{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

pub fn ansi_bold(text: &str) -> String {
    ansi_colorize("\x1b[1m", text)
}

pub fn ansi_dim(text: &str) -> String {
    ansi_colorize("\x1b[2m", text)
}

pub struct Spinner {
    bar: ProgressBar,
}

impl Spinner {
    pub fn new(message: &str) -> Self {
        let bar = ProgressBar::new_spinner();
        bar.enable_steady_tick(Duration::from_millis(100));
        // indicatif's template DSL only supports named/256 colors; cyan is the
        // closest stand-in for Aurora's --aurora-accent-primary (#29B6F6).
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
        .with_prompt(format!("{} {}", warning("[confirm]"), prompt))
        .default(false)
        .interact()?;
    Ok(proceed)
}

pub fn primary(text: &str) -> String {
    ansi_bold(&ansi_colorize(PRIMARY_ANSI, text))
}

pub fn accent(text: &str) -> String {
    ansi_colorize(ACCENT_ANSI, text)
}

pub fn success(text: &str) -> String {
    ansi_bold(&ansi_colorize(SUCCESS_ANSI, text))
}

pub fn warning(text: &str) -> String {
    ansi_bold(&ansi_colorize(WARN_ANSI, text))
}

pub fn muted(text: &str) -> String {
    ansi_colorize(MUTED_ANSI, text)
}

/// Aurora rose-deep — secondary info (UUIDs, ages, separators).
pub fn subtle(text: &str) -> String {
    ansi_colorize(SUBTLE_ANSI, text)
}

pub fn symbol_for_status(status: &str) -> String {
    match status {
        "completed" => ansi_colorize(SUCCESS_ANSI, "✓"),
        "failed" | "error" => ansi_colorize(ERROR_ANSI, "✗"),
        "pending" | "running" | "processing" | "scraping" => ansi_colorize(INFO_ANSI, "◐"),
        "canceled" => ansi_colorize(WARN_ANSI, "⚠"),
        _ => ansi_colorize(ACCENT_ANSI, "•"),
    }
}

pub fn status_text(status: &str) -> String {
    match status {
        "completed" => ansi_colorize(SUCCESS_ANSI, status),
        "failed" | "error" => ansi_colorize(ERROR_ANSI, status),
        "pending" | "running" | "processing" | "scraping" => ansi_colorize(INFO_ANSI, status),
        "canceled" => ansi_colorize(WARN_ANSI, status),
        _ => ansi_colorize(ACCENT_ANSI, status),
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
    ansi_colorize(ERROR_ANSI, text)
}

/// "error: <msg>" in Aurora rose-red on stderr — for CLI user-facing errors.
pub fn report_error(msg: &str) {
    eprintln!(
        "{} {}",
        ansi_bold(&ansi_colorize(ERROR_ANSI, "error:")),
        msg
    );
}

/// "hint: <msg>" in Aurora cyan/dim on stderr — companion to report_error.
pub fn report_hint(msg: &str) {
    let label = ansi_dim(&ansi_colorize(ACCENT_ANSI, "hint:"));
    eprintln!("{label} {msg}");
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
