use std::io::{self, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::thread;

use gpui::SharedString;

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;

use crate::actions::{ACTIONS, CommandAction};
use crate::markdown::MarkdownDocument;
use crate::theme::{
    AURORA_ACCENT_PINK, AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_WARNING,
};

const OUTPUT_LIMIT: usize = 12_000;
const TRUNCATED_MESSAGE: &str = "\n... output truncated ...";
/// Maximum number of lines rendered for a raw (non-markdown) output
/// section. Mirrors the `take(MAX_RENDER_LINES)` cap in `render.rs`.
pub(crate) const MAX_RENDER_LINES: usize = 220;

#[derive(Clone)]
pub(crate) struct CommandOutput {
    pub(crate) kind: OutputKind,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) stdout: Option<OutputSection>,
    pub(crate) stderr: Option<OutputSection>,
    pub(crate) use_markdown: bool,
    pub(crate) compact_stdout: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum OutputKind {
    Running,
    Success,
    Warning,
    Error,
}

#[derive(Clone)]
pub(crate) struct OutputSection {
    pub(crate) label: &'static str,
    pub(crate) text: String,
    pub(crate) line_count: usize,
    /// Pre-computed visible raw lines, capped at `MAX_RENDER_LINES`, so render
    /// frames can clone cheap `SharedString`s instead of reallocating lines.
    pub(crate) rendered_lines: Vec<SharedString>,
    pub(crate) markdown: Option<MarkdownDocument>,
}

pub(crate) struct BoundedProcessOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
}

impl CommandOutput {
    pub(crate) fn running(command_line: &str, action: CommandAction) -> Self {
        Self {
            kind: OutputKind::Running,
            title: format!("Running {}", action.label),
            subtitle: command_line.to_string(),
            stdout: None,
            stderr: None,
            use_markdown: false,
            compact_stdout: false,
        }
    }

    pub(crate) fn notice(
        kind: OutputKind,
        title: impl Into<String>,
        subtitle: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            subtitle: subtitle.into(),
            stdout: None,
            stderr: None,
            use_markdown: false,
            compact_stdout: false,
        }
    }

    pub(crate) fn spawn_error(command_line: &str, error: String) -> Self {
        Self {
            kind: OutputKind::Error,
            title: "Could not start axon".to_string(),
            subtitle: command_line.to_string(),
            stdout: None,
            stderr: Some(OutputSection::new("spawn error", error)),
            use_markdown: false,
            compact_stdout: false,
        }
    }

    pub(crate) fn from_process(
        command_line: &str,
        subcommand: &str,
        output: BoundedProcessOutput,
    ) -> Self {
        let use_markdown = matches!(subcommand, "scrape" | "ask" | "research");
        let stdout = OutputSection::from_bytes_for_command(
            "stdout",
            subcommand,
            &output.stdout,
            use_markdown,
        );
        let success = output.status.success();
        let stderr = OutputSection::from_bytes("stderr", &output.stderr).map(|section| {
            if success {
                section
            } else {
                section.with_text(actionable_error_text(&section.text), false)
            }
        });
        let kind = if success {
            OutputKind::Success
        } else {
            OutputKind::Error
        };
        let title = if success {
            format!("{} completed", command_title(subcommand))
        } else {
            format!("{} failed", command_title(subcommand))
        };
        let subtitle = format!("{command_line} · {}", format_exit_status(&output.status));
        let compact_stdout = success && stderr.is_none();

        Self {
            kind,
            title,
            subtitle,
            stdout,
            stderr,
            use_markdown,
            compact_stdout,
        }
    }

    pub(crate) fn has_body(&self) -> bool {
        self.stdout.is_some() || self.stderr.is_some()
    }
}

impl OutputSection {
    fn from_bytes_for_command(
        label: &'static str,
        subcommand: &str,
        bytes: &[u8],
        use_markdown: bool,
    ) -> Option<Self> {
        Self::from_bytes(label, bytes).map(|section| {
            let section = if subcommand == "map" {
                section.with_text(map_url_listing(&section.text), use_markdown)
            } else {
                section
            };
            if use_markdown {
                section.with_markdown()
            } else {
                section
            }
        })
    }

    fn from_bytes(label: &'static str, bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let raw = String::from_utf8_lossy(bytes);
        Some(Self::new(label, strip_ansi(raw.trim_end())))
    }

    fn new(label: &'static str, text: impl Into<String>) -> Self {
        Self::build(label, truncate_output(text.into()), false)
    }

    fn with_text(&self, text: impl Into<String>, use_markdown: bool) -> Self {
        Self::build(self.label, truncate_output(text.into()), use_markdown)
    }

    fn with_markdown(&self) -> Self {
        Self::build(self.label, self.text.clone(), true)
    }

    fn build(label: &'static str, text: String, use_markdown: bool) -> Self {
        let line_count = text.lines().count().max(1);
        let markdown = use_markdown.then(|| MarkdownDocument::parse(&text));
        let rendered_lines = text
            .lines()
            .take(MAX_RENDER_LINES)
            .map(|line| {
                if line.is_empty() {
                    SharedString::from(" ")
                } else {
                    SharedString::from(line.to_string())
                }
            })
            .collect();
        Self {
            label,
            text,
            line_count,
            rendered_lines,
            markdown,
        }
    }
}

pub(crate) fn run_command_bounded(mut command: Command) -> io::Result<BoundedProcessOutput> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("child stdout was not piped"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("child stderr was not piped"))?;

    let stdout_reader = thread::spawn(move || read_bounded(stdout));
    let stderr_reader = thread::spawn(move || read_bounded(stderr));
    let status = child.wait()?;
    let stdout = stdout_reader
        .join()
        .map_err(|_| io::Error::other("stdout reader panicked"))??;
    let stderr = stderr_reader
        .join()
        .map_err(|_| io::Error::other("stderr reader panicked"))??;

    Ok(BoundedProcessOutput {
        status,
        stdout,
        stderr,
    })
}

fn read_bounded(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut buffer = BoundedByteBuffer::new(OUTPUT_LIMIT);
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.push(&chunk[..read]);
    }
    Ok(buffer.into_bytes())
}

struct BoundedByteBuffer {
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
}

impl BoundedByteBuffer {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(limit.min(8192)),
            limit,
            truncated: false,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        let remaining = self.limit.saturating_sub(self.bytes.len());
        if self.bytes.len() < self.limit {
            self.bytes
                .extend_from_slice(&chunk[..chunk.len().min(remaining)]);
        }
        if chunk.len() > remaining {
            self.truncated = true;
        }
    }

    fn into_bytes(mut self) -> Vec<u8> {
        if !self.truncated {
            return self.bytes;
        }

        let boundary = valid_utf8_boundary(&self.bytes);
        self.bytes.truncate(boundary);
        self.bytes.extend_from_slice(TRUNCATED_MESSAGE.as_bytes());
        self.bytes
    }
}

fn valid_utf8_boundary(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error) => error.valid_up_to(),
    }
}

fn map_url_listing(text: &str) -> String {
    let urls: Vec<&str> = text
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            trimmed
                .strip_prefix('•')
                .or_else(|| trimmed.strip_prefix("- "))
                .map(str::trim)
                .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        })
        .collect();

    if urls.is_empty() {
        text.to_string()
    } else {
        urls.join("\n")
    }
}

fn actionable_error_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if let Some(index) = lines
        .iter()
        .position(|line| line.trim_start().starts_with("Error:"))
    {
        return lines[index..].join("\n");
    }

    let non_log_lines: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.contains(" WARN ")
                || trimmed.contains(" INFO ")
                || trimmed.contains(" DEBUG ")
                || trimmed.contains(" TRACE "))
        })
        .collect();

    if non_log_lines.is_empty() {
        text.to_string()
    } else {
        non_log_lines.join("\n")
    }
}

impl OutputKind {
    pub(crate) fn label(self) -> &'static str {
        match self {
            OutputKind::Running => "running",
            OutputKind::Success => "done",
            OutputKind::Warning => "notice",
            OutputKind::Error => "error",
        }
    }

    pub(crate) fn accent_color(self) -> u32 {
        match self {
            OutputKind::Running => AURORA_ACCENT_PRIMARY,
            OutputKind::Success => AURORA_ACCENT_STRONG,
            OutputKind::Warning => AURORA_WARNING,
            OutputKind::Error => AURORA_ACCENT_PINK,
        }
    }
}

/// Strip ANSI / VT escape sequences. CSI, OSC, DCS, APC, PM, and SOS are
/// covered; malformed sequences are silently dropped.
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        // Look at the byte immediately after ESC to pick the sequence kind.
        let Some(&next) = chars.peek() else {
            // lone trailing ESC — drop it
            continue;
        };
        match next {
            '[' => {
                chars.next();
                // CSI: consume until a final byte in 0x40..=0x7E.
                for ch in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&ch) {
                        break;
                    }
                }
            }
            ']' => {
                // OSC — terminates on BEL (0x07) or ST (ESC \).
                chars.next();
                consume_until_string_terminator(&mut chars, /* allow_bel = */ true);
            }
            'P' | '_' | '^' | 'X' => {
                // DCS/APC/PM/SOS terminate only on ST; embedded BEL is payload.
                chars.next();
                consume_until_string_terminator(&mut chars, /* allow_bel = */ false);
            }
            _ => {
                // Some other Fp/Fe/Fs/two-char escape (e.g. ESC =, ESC c).
                // Drop the single follow-up byte and move on.
                chars.next();
            }
        }
    }
    out
}

/// Consume until ST (`ESC \`), optionally accepting BEL for OSC.
fn consume_until_string_terminator(
    chars: &mut std::iter::Peekable<std::str::Chars<'_>>,
    allow_bel: bool,
) {
    while let Some(ch) = chars.next() {
        if allow_bel && ch == '\x07' {
            return;
        }
        if ch == '\x1b' {
            // ST = ESC '\\'. Only a well-formed ST terminates the
            // sequence. A bare ESC inside a string-type payload is
            // malformed input; swallow it and keep stripping rather
            // than leaking the remainder of the payload to output.
            if chars.peek() == Some(&'\\') {
                chars.next();
                return;
            }
            continue;
        }
    }
}

/// Render an `ExitStatus` as a short human-readable string.
///
/// Replaces `std`'s `"exit status: 58"` / `"signal: 9 (SIGKILL)"` (and on some
/// platforms a raw hex code) with `"exit 58"` / `"killed by SIGKILL"` / `"ok"`.
fn format_exit_status(status: &std::process::ExitStatus) -> String {
    if status.success() {
        return "ok".to_string();
    }
    if let Some(code) = status.code() {
        return format!("exit {code}");
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            let name = signal_name(sig).unwrap_or("signal");
            return format!("killed by {name} ({sig})");
        }
    }
    // Fallback for non-Unix or unknown termination.
    format!("{status}")
}

#[cfg(unix)]
fn signal_name(sig: i32) -> Option<&'static str> {
    match sig {
        1 => Some("SIGHUP"),
        2 => Some("SIGINT"),
        3 => Some("SIGQUIT"),
        6 => Some("SIGABRT"),
        9 => Some("SIGKILL"),
        11 => Some("SIGSEGV"),
        13 => Some("SIGPIPE"),
        14 => Some("SIGALRM"),
        15 => Some("SIGTERM"),
        _ => None,
    }
}

fn command_title(subcommand: &str) -> &'static str {
    ACTIONS
        .iter()
        .find(|action| action.subcommand == subcommand)
        .map(|action| action.label)
        .unwrap_or("Command")
}

fn truncate_output(mut text: String) -> String {
    if text.len() <= OUTPUT_LIMIT {
        return text;
    }

    // floor_char_boundary finds the largest char boundary <= OUTPUT_LIMIT,
    // preventing a panic when a multibyte character straddles the limit.
    let boundary = text.floor_char_boundary(OUTPUT_LIMIT);
    text.truncate(boundary);
    text.push_str("\n... output truncated ...");
    text
}
