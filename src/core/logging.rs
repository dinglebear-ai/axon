mod aurora;
mod size_rotating;

use chrono::Local;
use size_rotating::SizeRotatingFile;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use tracing::field::{Field, Visit};
use tracing::{debug, error, info, warn};
use tracing_subscriber::fmt::{
    FmtContext, FormatEvent, FormatFields, FormattedFields, format::Writer,
};
use tracing_subscriber::registry::LookupSpan;

// ── Raw ANSI helpers ─────────────────────────────────────────────────────────

fn ansi256_bold(n: u8, text: &str) -> String {
    format!("\x1b[1;38;5;{n}m{text}\x1b[0m")
}

fn ansi_dim(text: &str) -> String {
    format!("\x1b[2m{text}\x1b[0m")
}

fn truecolor_bold(rgb: (u8, u8, u8), text: &str) -> String {
    let (r, g, b) = rgb;
    format!("\x1b[1;38;2;{r};{g};{b}m{text}\x1b[0m")
}

/// True iff the terminal advertises 24-bit color via COLORTERM.
fn supports_truecolor() -> bool {
    matches!(
        std::env::var("COLORTERM").as_deref(),
        Ok("truecolor") | Ok("24bit")
    )
}

fn read_trimmed_env(var: &str) -> Option<String> {
    std::env::var(var)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

// ── Console event formatter ──────────────────────────────────────────────────
//
// Renders log lines on stderr as:
//   HH:MM:SS   LEVEL  event_name  key=value  key=value
//
// Colors (when ANSI is supported) — Aurora palette:
//   timestamp — dim
//   LEVEL     — aurora::ERROR bold (ERROR), aurora::WARN bold (WARN), plain (INFO), dim (DEBUG/TRACE)
//   first token of message — aurora::SERVICE_NAME bold (pink)
//   key=      — dim
//   value     — plain (inherits terminal default)
//
// The JSON file layer uses tracing-subscriber's built-in JSON formatter with
// `with_ansi(false)`, so it never receives ANSI escape codes.

#[derive(Default)]
struct EventVisitor {
    message: String,
    extra: Vec<(String, String)>,
}

impl Visit for EventVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_owned();
        } else {
            self.extra.push((field.name().to_owned(), value.to_owned()));
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let s = format!("{value:?}");
        if field.name() == "message" {
            self.message = s;
        } else {
            self.extra.push((field.name().to_owned(), s));
        }
    }
}

/// Write the log level to `writer`, with Aurora ANSI 256 colour when `ansi` is true.
fn write_level(writer: &mut Writer<'_>, level: tracing::Level, ansi: bool) -> fmt::Result {
    if ansi {
        let tc = supports_truecolor();
        match level {
            tracing::Level::ERROR => {
                if tc {
                    write!(writer, "{}  ", truecolor_bold(aurora::rgb::ERROR, "ERROR"))
                } else {
                    write!(writer, "{}  ", ansi256_bold(aurora::ERROR, "ERROR"))
                }
            }
            tracing::Level::WARN => {
                if tc {
                    write!(writer, "{}  ", truecolor_bold(aurora::rgb::WARN, " WARN"))
                } else {
                    write!(writer, "{}  ", ansi256_bold(aurora::WARN, " WARN"))
                }
            }
            tracing::Level::INFO => {
                if tc {
                    write!(writer, "{}  ", truecolor_bold(aurora::rgb::INFO, " INFO"))
                } else {
                    write!(writer, " INFO  ")
                }
            }
            tracing::Level::DEBUG => write!(writer, "{}  ", ansi_dim("DEBUG")),
            tracing::Level::TRACE => write!(writer, "{}  ", ansi_dim("TRACE")),
        }
    } else {
        match level {
            tracing::Level::ERROR => write!(writer, "ERROR  "),
            tracing::Level::WARN => write!(writer, " WARN  "),
            tracing::Level::INFO => write!(writer, " INFO  "),
            tracing::Level::DEBUG => write!(writer, "DEBUG  "),
            tracing::Level::TRACE => write!(writer, "TRACE  "),
        }
    }
}

/// Collect formatted span fields from leaf → root, then reverse to root → leaf order.
///
/// clone() on fields.fields is required — extensions() returns a temporary guard that
/// drops at the end of each loop iteration, so we cannot borrow &str across iterations.
fn collect_span_fields<S, N>(ctx: &FmtContext<'_, S, N>) -> Vec<String>
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    let mut fields: Vec<String> = Vec::new();
    let mut current = ctx.lookup_current();
    while let Some(span) = current {
        if let Some(f) = span.extensions().get::<FormattedFields<N>>()
            && !f.fields.is_empty()
        {
            fields.push(f.fields.clone());
        }
        current = span.parent();
    }
    fields.reverse();
    fields
}

/// Determine whether to emit ANSI escape codes. Explicit `--color` choices win
/// first, then standard env vars are applied for auto mode:
/// - `NO_COLOR` (https://no-color.org) — disables auto-mode colors
/// - `FORCE_COLOR` / `CLICOLOR_FORCE` — enables colors even without a TTY (Docker, CI)
/// - Falls back to the writer's own TTY detection
fn should_use_ansi(writer: &Writer<'_>) -> bool {
    should_use_ansi_for_writer(writer.has_ansi_escapes())
}

fn should_use_ansi_for_writer(writer_supports_ansi: bool) -> bool {
    // Honor --color={always,never} from Config (installed at startup).
    if crate::core::ui::color_forced_never() {
        return false;
    }
    // --color=always must produce ANSI even in non-TTY contexts (CI, redirected
    // logs). It bypasses the writer-side TTY detection completely.
    if crate::core::ui::color_forced_always() {
        return true;
    }
    if crate::core::ui::color_env_disabled() {
        return false;
    }
    // --color=auto: FORCE_COLOR/CLICOLOR_FORCE still wins over TTY detection.
    if crate::core::ui::color_env_forced() {
        return true;
    }
    writer_supports_ansi
}

struct CliFormat;

impl<S, N> FormatEvent<S, N> for CliFormat
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> fmt::Result {
        let ansi = should_use_ansi(&writer);

        // HH:MM:SS (local time)
        let ts = Local::now().format("%H:%M:%S").to_string();
        if ansi {
            write!(writer, "{}  ", ansi_dim(&ts))?;
        } else {
            write!(writer, "{ts}  ")?;
        }

        // LEVEL
        write_level(&mut writer, *event.metadata().level(), ansi)?;

        // MESSAGE
        let mut v = EventVisitor::default();
        event.record(&mut v);

        if ansi && !v.message.is_empty() {
            // Iterate directly instead of collecting into an intermediate Vec.
            for (i, token) in v.message.split_whitespace().enumerate() {
                if i > 0 {
                    write!(writer, " ")?;
                }
                if i == 0 {
                    let painted = if supports_truecolor() {
                        truecolor_bold(aurora::rgb::SERVICE_NAME, token)
                    } else {
                        ansi256_bold(aurora::SERVICE_NAME, token)
                    };
                    write!(writer, "{painted}")?;
                } else if let Some(eq) = token.find('=') {
                    // key=value — dim key, normal value
                    write!(
                        writer,
                        "{}{}{}",
                        ansi_dim(&token[..eq]),
                        ansi_dim("="),
                        &token[eq + 1..]
                    )?;
                } else {
                    write!(writer, "{token}")?;
                }
            }
        } else {
            write!(writer, "{}", v.message)?;
        }

        // Extra structured fields (e.g. status="done" from log_done)
        for (key, val) in &v.extra {
            if ansi {
                write!(
                    writer,
                    "  {}{}{}",
                    ansi_dim(key.as_str()),
                    ansi_dim("="),
                    val
                )?;
            } else {
                write!(writer, "  {key}={val}")?;
            }
        }

        // Span context fields (job_id, source, target, etc.)
        // Performance note: span walk runs on every console-emitted event. At the default
        // WARN filter this is negligible; consider gating on Level if ever lowered to INFO.
        for fields_str in &collect_span_fields(ctx) {
            if ansi {
                write!(writer, "  {}", ansi_dim(fields_str.as_str()))?;
            } else {
                write!(writer, "  {fields_str}")?;
            }
        }

        writeln!(writer)
    }
}

fn build_filter_with_noise(
    default_level: &str,
    noise_directives: &[&str],
) -> tracing_subscriber::EnvFilter {
    use tracing_subscriber::EnvFilter;
    EnvFilter::try_from_default_env()
        .map(|f| {
            // Logger is not yet initialized — use eprintln! for warnings.
            noise_directives.iter().fold(f, |acc, d| match d.parse() {
                Ok(directive) => acc.add_directive(directive),
                Err(e) => {
                    eprintln!("warning: failed to parse log directive '{d}': {e} -- skipping");
                    acc
                }
            })
        })
        .unwrap_or_else(|_| {
            let default_level = read_trimmed_env("AXON_LOG_LEVEL")
                .filter(|level| !level.is_empty())
                .unwrap_or_else(|| default_level.to_string());
            let extras = noise_directives.join(",");
            EnvFilter::new(format!("{default_level},{extras}"))
        })
}

pub fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::prelude::*;

    // CDP proxy sends non-standard frames that chromiumoxide logs at ERROR;
    // they are gracefully dropped, suppress the noise.
    const SUPPRESS_CDP_NOISE: &str = "chromiumoxide::conn::raw_ws::parse_errors=off";
    let noise_directives = [SUPPRESS_CDP_NOISE];

    let console_filter = build_filter_with_noise("warn", &noise_directives);
    let file_filter = build_filter_with_noise("info", &noise_directives);

    let console_layer = tracing_subscriber::fmt::layer()
        .event_format(CliFormat)
        .with_writer(io::stderr)
        .with_filter(console_filter);

    // ── Size-rotating file appender ──────────────────────────────────────────
    //
    // Active log file lives at `<dir>/<file_name>`. When it exceeds
    // `AXON_LOG_MAX_BYTES` the writer renames `.{N-1}` → `.N` from the top
    // down (oldest pruned), `<file>` → `<file>.1`, then reopens fresh.
    //
    // tracing_appender::non_blocking serialises writes through one worker
    // thread, so the guard MUST be held for the process lifetime.
    // Full path to active log file. Rotated siblings (.1, .2, ...) live in the same dir.
    // Default: $AXON_DATA_DIR/logs/axon.log
    let log_path: PathBuf = read_trimmed_env("AXON_LOG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            super::paths::axon_data_base_dir()
                .join("logs")
                .join("axon.log")
        });
    let log_dir: PathBuf = log_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| super::paths::axon_data_base_dir().join("logs"));
    let log_file_name = log_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "axon.log".to_string());

    let max_bytes = std::env::var("AXON_LOG_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10 * 1024 * 1024); // 10 MB

    let max_files = std::env::var("AXON_LOG_MAX_FILES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3);

    let Ok(file_appender) = SizeRotatingFile::new(log_dir, log_file_name, max_bytes, max_files)
    else {
        eprintln!("warn: failed to create axon log file appender; continuing with stderr logging");
        let (_sink, guard) = tracing_appender::non_blocking(io::sink());
        tracing_subscriber::registry().with(console_layer).init();
        return guard;
    };

    let (non_blocking_file, guard) = tracing_appender::non_blocking(file_appender);

    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_ansi(false)
        .with_writer(non_blocking_file)
        .with_filter(file_filter);

    tracing_subscriber::registry()
        .with(console_layer)
        .with(json_layer)
        .init();

    guard
}

// TODO(QUALITY): These wrapper functions lose the caller's `target` metadata -- tracing records
// this module (`axon::core::logging`) as the target instead of the actual call site. This makes
// filtering by target (e.g. `RUST_LOG=axon::jobs::crawl=debug`) miss log lines emitted through
// these wrappers. The proper fix is to replace them with macros (which expand at the call site and
// preserve target) or remove them entirely in favor of direct `tracing::info!()` / `tracing::warn!()`
// calls. Left as-is to avoid a large cross-crate refactor.
pub fn log_info(msg: &str) {
    info!("{}", msg);
}

pub fn log_warn(msg: &str) {
    warn!("{}", msg);
}

pub fn log_done(msg: &str) {
    info!(status = "done", "{}", msg);
}

pub fn log_error(msg: &str) {
    error!("{}", msg);
}

pub fn log_debug(msg: &str) {
    debug!("{}", msg);
}

#[cfg(test)]
mod tests {
    use super::should_use_ansi_for_writer;
    use crate::core::config::ColorChoice;
    use crate::core::ui::{COLOR_OVERRIDE, COLOR_TEST_GUARD, install_color_choice};

    #[test]
    fn color_override_controls_logging_ansi() {
        let _g = COLOR_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let prev = COLOR_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);

        install_color_choice(ColorChoice::Never);
        assert!(!should_use_ansi_for_writer(true));

        install_color_choice(ColorChoice::Always);
        assert!(should_use_ansi_for_writer(false));

        install_color_choice(ColorChoice::Auto);
        assert!(
            !crate::core::ui::color_forced_always(),
            "auto must clear the forced-always override"
        );

        COLOR_OVERRIDE.store(prev, std::sync::atomic::Ordering::Relaxed);
    }
}
