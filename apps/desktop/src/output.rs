use std::process::Output;

use gpui::SharedString;

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;

use crate::actions::{ACTIONS, CommandAction};
use crate::theme::{
    AURORA_ACCENT_PINK, AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_WARNING,
};

const OUTPUT_LIMIT: usize = 12_000;
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
    /// Pre-computed `SharedString` per visible line, capped at
    /// `MAX_RENDER_LINES`. Built once when the section is created and
    /// cloned cheaply (`Arc`-backed) on every render frame — avoids the
    /// ~220 allocations/frame the raw renderer used to incur.
    /// Only populated for raw (non-markdown) sections; markdown sections
    /// render directly from `text`.
    pub(crate) rendered_lines: Vec<SharedString>,
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

    pub(crate) fn from_process(command_line: &str, subcommand: &str, output: Output) -> Self {
        let stdout = OutputSection::from_bytes_for_command("stdout", subcommand, &output.stdout);
        let success = output.status.success();
        let stderr = OutputSection::from_bytes("stderr", &output.stderr).map(|section| {
            if success {
                section
            } else {
                section.with_text(actionable_error_text(&section.text))
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
        let use_markdown = matches!(subcommand, "scrape" | "ask" | "research");
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
    fn from_bytes_for_command(label: &'static str, subcommand: &str, bytes: &[u8]) -> Option<Self> {
        Self::from_bytes(label, bytes).map(|section| {
            if subcommand == "map" {
                section.with_text(map_url_listing(&section.text))
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
        Self::build(label, truncate_output(text.into()))
    }

    fn with_text(&self, text: impl Into<String>) -> Self {
        Self::build(self.label, truncate_output(text.into()))
    }

    fn build(label: &'static str, text: String) -> Self {
        let line_count = text.lines().count().max(1);
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
        }
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

/// Strip ANSI / VT escape sequences.
///
/// Covers:
/// - **CSI** (`ESC [` … final byte in `0x40..=0x7E`) — colour/format codes.
/// - **OSC** (`ESC ]` … terminated by `BEL` (`0x07`) or `ST` (`ESC \`)) —
///   title-setting and similar OS commands. Per ECMA-48 / xterm convention,
///   OSC accepts BEL as a shortcut terminator for legacy compatibility.
/// - **DCS** (`ESC P` … terminated by `ST` only) — device control strings.
/// - **APC** (`ESC _` … terminated by `ST` only) — application program commands.
/// - **PM**  (`ESC ^` … terminated by `ST` only) — privacy messages.
/// - **SOS** (`ESC X` … terminated by `ST` only) — start of string.
///
/// Per ECMA-48, DCS/APC/PM/SOS are NOT terminated by BEL — only OSC accepts
/// BEL as a terminator (xterm legacy convention). Embedded BEL bytes inside
/// DCS/APC/PM/SOS payloads must be passed through as content.
///
/// Anything malformed (lone `ESC`, EOF mid-sequence) is silently dropped.
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
                // DCS / APC / PM / SOS — terminate ONLY on ST (ESC \).
                // Embedded BEL bytes are part of the payload and must not
                // short-circuit the sequence (ECMA-48 §8.3).
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

/// Consume characters until a String Terminator is seen.
///
/// The terminator itself is consumed. EOF mid-sequence is silently accepted.
///
/// `allow_bel = true` accepts `BEL` (`0x07`) as a shortcut terminator (OSC
/// convention); `false` treats BEL as ordinary payload and waits for the
/// canonical ST = `ESC \` (DCS/APC/PM/SOS semantics).
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
