mod size_rotating;

use chrono::Local;
use console::Style;
use size_rotating::SizeRotatingFile;
use std::fmt;
use std::io;
use std::path::PathBuf;
use tracing::field::{Field, Visit};
use tracing::{debug, error, info, warn};
use tracing_subscriber::fmt::{
    FmtContext, FormatEvent, FormatFields, FormattedFields, format::Writer,
};
use tracing_subscriber::registry::LookupSpan;

// ── Console event formatter ──────────────────────────────────────────────────
//
// Renders log lines on stderr as:
//   HH:MM:SS   LEVEL  event_name  key=value  key=value
//
// Colors (when ANSI is supported):
//   timestamp — dim
//   LEVEL     — green (INFO), yellow (WARN), red (ERROR), dim (DEBUG/TRACE)
//   event     — bold white (first whitespace-delimited token of the message)
//   key=      — dim
//   value     — normal (inherits terminal default)
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

/// Write the log level to `writer`, with ANSI colour when `ansi` is true.
fn write_level(writer: &mut Writer<'_>, level: tracing::Level, ansi: bool) -> fmt::Result {
    if ansi {
        match level {
            tracing::Level::ERROR => {
                write!(writer, "{}  ", Style::new().red().bold().apply_to("ERROR"))
            }
            tracing::Level::WARN => write!(
                writer,
                "{}  ",
                Style::new().yellow().bold().apply_to(" WARN")
            ),
            tracing::Level::INFO => write!(writer, "{}  ", Style::new().green().apply_to(" INFO")),
            tracing::Level::DEBUG => write!(writer, "{}  ", Style::new().dim().apply_to("DEBUG")),
            tracing::Level::TRACE => write!(writer, "{}  ", Style::new().dim().apply_to("TRACE")),
        }
    } else {
        write!(writer, "{level:5}  ")
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
        let ansi = writer.has_ansi_escapes();

        // HH:MM:SS (local time)
        let ts = Local::now().format("%H:%M:%S").to_string();
        if ansi {
            write!(writer, "{}  ", Style::new().dim().apply_to(&ts))?;
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
                    write!(writer, "{}", Style::new().bold().apply_to(token))?;
                } else if let Some(eq) = token.find('=') {
                    // key=value — dim key, normal value
                    write!(
                        writer,
                        "{}{}{}",
                        Style::new().dim().apply_to(&token[..eq]),
                        Style::new().dim().apply_to("="),
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
                    Style::new().dim().apply_to(key.as_str()),
                    Style::new().dim().apply_to("="),
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
                write!(
                    writer,
                    "  {}",
                    Style::new().dim().apply_to(fields_str.as_str())
                )?;
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
            let extras = noise_directives.join(",");
            EnvFilter::new(format!("{default_level},{extras}"))
        })
}

pub fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::prelude::*;

    // CDP proxy sends non-standard frames that chromiumoxide logs at ERROR;
    // they are gracefully dropped, suppress the noise.
    const SUPPRESS_CDP_NOISE: &str = "chromiumoxide::conn::raw_ws::parse_errors=off";
    // agent-client-protocol does not recognise usage_update frames from
    // Claude Code; non-fatal, suppress until upstream adds the variant.
    const SUPPRESS_ACP_DECODE_NOISE: &str = "agent_client_protocol::rpc=off";

    let noise_directives = [SUPPRESS_CDP_NOISE, SUPPRESS_ACP_DECODE_NOISE];

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
    let log_dir: PathBuf = std::env::var("AXON_LOG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            super::paths::axon_data_dir()
                .map(|d| d.join("logs"))
                .unwrap_or_else(|| PathBuf::from("logs"))
        });
    // Use ensure_private_dir (0o700) so axon.log + rotated archives —
    // which include redacted Config dumps and may include service URLs
    // in error messages — are not world-readable on multi-user hosts.
    super::paths::ensure_private_dir(&log_dir).ok();

    let log_file_name = std::env::var("AXON_LOG_FILE").unwrap_or_else(|_| "axon.log".to_string());

    let max_bytes = std::env::var("AXON_LOG_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10 * 1024 * 1024); // 10 MB

    let max_files = std::env::var("AXON_LOG_MAX_FILES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3);

    let file_appender = SizeRotatingFile::new(log_dir, log_file_name, max_bytes, max_files)
        .expect("failed to create axon log file appender");

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
// this module (`crates::core::logging`) as the target instead of the actual call site. This makes
// filtering by target (e.g. `RUST_LOG=crates::jobs::crawl=debug`) miss log lines emitted through
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
