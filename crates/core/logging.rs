use console::Style;
use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tracing::field::{Field, Visit};
use tracing::{debug, error, info, warn};
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::fmt::{
    FmtContext, FormatEvent, FormatFields, FormattedFields, format::Writer,
};
use tracing_subscriber::registry::LookupSpan;

#[derive(Debug)]
struct SizeRotatingFile {
    path: PathBuf,
    file: std::fs::File,
    current_size: u64,
    max_bytes: u64,
    max_files_total: usize,
}

impl SizeRotatingFile {
    fn new(path: PathBuf, max_bytes: u64, max_files_total: usize) -> io::Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            path,
            file,
            current_size,
            max_bytes,
            max_files_total: max_files_total.max(1),
        })
    }

    fn indexed_path(&self, idx: usize) -> PathBuf {
        let mut s = self.path.as_os_str().to_owned();
        s.push(format!(".{idx}"));
        PathBuf::from(s)
    }

    // TODO(PERF-MED-4): rotate_if_needed performs blocking fs::rename and File::create.
    // These run inside Mutex<SizeRotatingFile> via tracing's sync Write trait — NOT in an
    // async task. If tracing ever moves to an async writer, wrap these in spawn_blocking.
    // Risk: on slow NFS mounts, rotation could briefly block the async runtime thread that
    // happens to be logging. Mitigation: keep log files on local disk (the default).
    fn rotate_if_needed(&mut self, incoming_len: usize) -> io::Result<()> {
        if self.max_bytes == 0 {
            return Ok(());
        }
        if self.current_size.saturating_add(incoming_len as u64) <= self.max_bytes {
            return Ok(());
        }

        self.file.flush()?;
        let rotated_slots = self.max_files_total.saturating_sub(1);
        if rotated_slots == 0 {
            self.file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.path)?;
            self.current_size = 0;
            return Ok(());
        }

        let oldest = self.indexed_path(rotated_slots);
        if oldest.exists() {
            let _ = std::fs::remove_file(oldest);
        }

        for idx in (1..rotated_slots).rev() {
            let src = self.indexed_path(idx);
            let dst = self.indexed_path(idx + 1);
            if src.exists() {
                std::fs::rename(&src, &dst)?;
            }
        }

        if self.path.exists() {
            std::fs::rename(&self.path, self.indexed_path(1))?;
        }

        self.file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.current_size = 0;
        Ok(())
    }
}

#[derive(Clone)]
struct SizeRotateMakeWriter {
    inner: Arc<Mutex<SizeRotatingFile>>,
}

struct SizeRotateWriterGuard {
    inner: Arc<Mutex<SizeRotatingFile>>,
}

impl<'a> MakeWriter<'a> for SizeRotateMakeWriter {
    type Writer = SizeRotateWriterGuard;

    fn make_writer(&'a self) -> Self::Writer {
        SizeRotateWriterGuard {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl Write for SizeRotateWriterGuard {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("log writer mutex poisoned"))?;
        inner.rotate_if_needed(buf.len())?;
        let written = inner.file.write(buf)?;
        inner.current_size = inner.current_size.saturating_add(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| io::Error::other("log writer mutex poisoned"))?;
        inner.file.flush()
    }
}

/// Outcome of attempting to open the rotating log file with optional fallback.
enum RotatingOutcome {
    Opened(SizeRotatingFile),
    BothFailed {
        primary_path: PathBuf,
        primary_err: io::Error,
        fallback_path: PathBuf,
        fallback_err: io::Error,
    },
    SamePathFailed {
        path: PathBuf,
        err: io::Error,
    },
}

/// Try to open `log_path` for rotating writes; fall back to `logs/axon.log`
/// if that fails with a different path. Reports which errors came from which
/// paths so both failures are visible when debugging.
fn open_rotating_file(
    log_path: &PathBuf,
    max_bytes: u64,
    max_files_total: usize,
) -> RotatingOutcome {
    match SizeRotatingFile::new(log_path.clone(), max_bytes, max_files_total) {
        Ok(r) => RotatingOutcome::Opened(r),
        Err(primary_err) => {
            let fallback = PathBuf::from("logs/axon.log");
            if &fallback == log_path {
                return RotatingOutcome::SamePathFailed {
                    path: log_path.clone(),
                    err: primary_err,
                };
            }
            eprintln!(
                "warning: cannot open log file {}: {} — falling back to {}",
                log_path.display(),
                primary_err,
                fallback.display(),
            );
            match SizeRotatingFile::new(fallback.clone(), max_bytes, max_files_total) {
                Ok(r) => RotatingOutcome::Opened(r),
                Err(fallback_err) => RotatingOutcome::BothFailed {
                    primary_path: log_path.clone(),
                    primary_err,
                    fallback_path: fallback,
                    fallback_err,
                },
            }
        }
    }
}

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

        // HH:MM:SS (UTC) ─────────────────────────────────────────────────────
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let ts = format!(
            "{:02}:{:02}:{:02}",
            (secs % 86400) / 3600,
            (secs % 3600) / 60,
            secs % 60
        );
        if ansi {
            write!(writer, "{}  ", Style::new().dim().apply_to(&ts))?;
        } else {
            write!(writer, "{ts}  ")?;
        }

        // LEVEL ───────────────────────────────────────────────────────────────
        let level = *event.metadata().level();
        if ansi {
            let s = match level {
                tracing::Level::ERROR => Style::new().red().bold().apply_to("ERROR").to_string(),
                tracing::Level::WARN => Style::new().yellow().bold().apply_to(" WARN").to_string(),
                tracing::Level::INFO => Style::new().green().apply_to(" INFO").to_string(),
                tracing::Level::DEBUG => Style::new().dim().apply_to("DEBUG").to_string(),
                tracing::Level::TRACE => Style::new().dim().apply_to("TRACE").to_string(),
            };
            write!(writer, "{s}  ")?;
        } else {
            write!(writer, "{level:5}  ")?;
        }

        // MESSAGE ─────────────────────────────────────────────────────────────
        let mut v = EventVisitor::default();
        event.record(&mut v);

        if ansi && !v.message.is_empty() {
            let tokens: Vec<&str> = v.message.split_whitespace().collect();
            for (i, token) in tokens.iter().enumerate() {
                if i > 0 {
                    write!(writer, " ")?;
                }
                if i == 0 {
                    // event name — bold
                    write!(writer, "{}", Style::new().bold().apply_to(*token))?;
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

        // extra structured fields (e.g. status="done" from log_done) ─────────
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

        // Span context fields (job_id, source, target, etc.) ─────────────────
        // Performance note: this span walk runs on every console-emitted event. At the
        // default WARN filter this is negligible. If the console filter is ever lowered
        // to INFO (e.g. via RUST_LOG=info), high-throughput paths (embed batches, crawl
        // pages) will walk spans on every emit. Consider gating on Level if observed.
        let mut span_fields: Vec<String> = Vec::new();
        let mut current = ctx.lookup_current();
        while let Some(span) = current {
            if let Some(fields) = span.extensions().get::<FormattedFields<N>>()
                && !fields.fields.is_empty()
            {
                span_fields.push(fields.fields.clone());
            }
            current = span.parent();
        }
        span_fields.reverse(); // root → leaf order
        for fields_str in &span_fields {
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

fn resolve_json_log_file() -> String {
    std::env::var("AXON_LOG_FILE")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("AXON_DATA_DIR")
                .ok()
                .map(|d| d.trim().to_string())
                .filter(|d| !d.is_empty())
                .map(|d| format!("{d}/axon/logs/axon.log"))
        })
        .unwrap_or_else(|| "logs/axon.log".to_string())
}

fn resolve_rotation_limits() -> (u64, usize) {
    let max_bytes = std::env::var("AXON_LOG_MAX_BYTES")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10 * 1024 * 1024)
        .max(1024);
    let max_files_total = std::env::var("AXON_LOG_MAX_FILES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(3)
        .max(1);
    (max_bytes, max_files_total)
}

fn build_filter_with_noise(
    default_level: &str,
    noise_directives: &[&str],
) -> tracing_subscriber::EnvFilter {
    use tracing_subscriber::EnvFilter;
    EnvFilter::try_from_default_env()
        .map(|f| {
            noise_directives.iter().fold(f, |acc, d| {
                acc.add_directive(d.parse().expect("hard-coded directive is valid"))
            })
        })
        .unwrap_or_else(|_| {
            let extras = noise_directives.join(",");
            EnvFilter::new(format!("{default_level},{extras}"))
        })
}

pub fn init_tracing() {
    use tracing_subscriber::prelude::*;

    let json_log_file = resolve_json_log_file();
    let (max_bytes, max_files_total) = resolve_rotation_limits();

    // Browserless (CDP proxy) sends non-standard session management frames over
    // the same WebSocket as standard CDP traffic. chromiumoxide's Message<T> is
    // an untagged enum with only two variants — Response (needs `id`) and Event
    // (needs `method`) — so any Browserless-specific frame fails serde and the
    // chromey library logs it at ERROR. The frames are gracefully dropped and
    // crawling succeeds; the error level is a library misclassification.
    // Suppress by default. Because this directive is added in code, same-target
    // RUST_LOG overrides are not guaranteed to win under tracing-subscriber 0.3.
    // To force-enable this target, change/remove this directive in code.
    const SUPPRESS_CDP_NOISE: &str = "chromiumoxide::conn::raw_ws::parse_errors=off";

    // agent-client-protocol 0.10.0 doesn't recognise `usage_update` session
    // updates that Claude Code sends. The deserialization error is logged at ERROR
    // but is non-fatal — the session continues normally. Suppress until the crate
    // adds the variant upstream.
    const SUPPRESS_ACP_DECODE_NOISE: &str = "agent_client_protocol::rpc=off";

    let noise_directives = [SUPPRESS_CDP_NOISE, SUPPRESS_ACP_DECODE_NOISE];

    let console_filter = build_filter_with_noise("warn", &noise_directives);
    let file_filter = build_filter_with_noise("info", &noise_directives);

    let log_path = PathBuf::from(&json_log_file);

    let console_layer = tracing_subscriber::fmt::layer()
        .event_format(CliFormat)
        .with_writer(io::stderr)
        .with_filter(console_filter);

    match open_rotating_file(&log_path, max_bytes, max_files_total) {
        RotatingOutcome::Opened(rotating) => {
            let file_writer = SizeRotateMakeWriter {
                inner: Arc::new(Mutex::new(rotating)),
            };
            let json_layer = tracing_subscriber::fmt::layer()
                .json()
                .with_ansi(false)
                .with_writer(file_writer)
                .with_filter(file_filter);
            tracing_subscriber::registry()
                .with(console_layer)
                .with(json_layer)
                .init();
        }
        RotatingOutcome::BothFailed {
            primary_path,
            primary_err,
            fallback_path,
            fallback_err,
        } => {
            eprintln!(
                "warning: failed to initialize file logging — \
                 primary path '{}' failed: {}; \
                 fallback path '{}' also failed: {} \
                 (continuing with console logs only)",
                primary_path.display(),
                primary_err,
                fallback_path.display(),
                fallback_err,
            );
            tracing_subscriber::registry().with(console_layer).init();
        }
        RotatingOutcome::SamePathFailed { path, err } => {
            eprintln!(
                "warning: failed to initialize file logging at {}: {} (continuing with console logs only)",
                path.display(),
                err
            );
            tracing_subscriber::registry().with(console_layer).init();
        }
    }
}

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
    use super::SizeRotatingFile;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_event(rotating: &mut SizeRotatingFile, event: &str) {
        let bytes = event.as_bytes();
        rotating
            .rotate_if_needed(bytes.len())
            .expect("rotate check should succeed");
        rotating
            .file
            .write_all(bytes)
            .expect("write should succeed");
        rotating.current_size = rotating.current_size.saturating_add(bytes.len() as u64);
        rotating.file.flush().expect("flush should succeed");
    }

    #[test]
    fn size_rotation_keeps_max_three_files() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("axon.log");
        let mut rotating = SizeRotatingFile::new(path.clone(), 32, 3).expect("init");

        for i in 0..20 {
            write_event(&mut rotating, &format!("event-{i:02}-payload\n"));
        }

        let entries = std::fs::read_dir(dir.path()).expect("read_dir").count();
        assert_eq!(entries, 3);
        assert!(path.exists());
        assert!(dir.path().join("axon.log.1").exists());
        assert!(dir.path().join("axon.log.2").exists());
        assert!(!dir.path().join("axon.log.3").exists());
    }

    #[test]
    fn writer_guard_write_all_updates_size_counter() {
        use super::{SizeRotateMakeWriter, SizeRotatingFile};
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("axon.log");
        let rotating = SizeRotatingFile::new(path.clone(), 1024, 3).expect("init");
        let make_writer = SizeRotateMakeWriter {
            inner: std::sync::Arc::new(std::sync::Mutex::new(rotating)),
        };
        use tracing_subscriber::fmt::writer::MakeWriter;
        let mut guard = make_writer.make_writer();
        let payload = b"hello-guard\n";
        guard.write_all(payload).expect("write_all should succeed");
        guard.flush().expect("flush should succeed");
        // Verify write reached the file.
        let content = std::fs::read_to_string(&path).expect("read file");
        assert_eq!(content, "hello-guard\n");
        // Verify the size counter was incremented — drives the rotation trigger.
        drop(guard);
        let inner = make_writer.inner.lock().unwrap();
        assert_eq!(inner.current_size, payload.len() as u64);
    }

    #[test]
    fn size_rotation_preserves_newest_in_primary_file() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("axon.log");
        let mut rotating = SizeRotatingFile::new(path.clone(), 10, 3).expect("init");

        write_event(&mut rotating, "A12345\n");
        write_event(&mut rotating, "B12345\n");
        write_event(&mut rotating, "C12345\n");
        write_event(&mut rotating, "D12345\n");

        let primary = std::fs::read_to_string(&path).expect("read primary");
        let rotated_1 = std::fs::read_to_string(dir.path().join("axon.log.1")).expect("read .1");
        let rotated_2 = std::fs::read_to_string(dir.path().join("axon.log.2")).expect("read .2");

        assert_eq!(primary, "D12345\n");
        assert_eq!(rotated_1, "C12345\n");
        assert_eq!(rotated_2, "B12345\n");
    }
}
