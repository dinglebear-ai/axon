use std::process::Output;

use crate::actions::{ACTIONS, CommandAction};
use crate::theme::{
    AURORA_ACCENT_PINK, AURORA_ACCENT_PRIMARY, AURORA_ACCENT_STRONG, AURORA_WARNING,
};

const OUTPUT_LIMIT: usize = 12_000;

#[derive(Clone)]
pub(crate) struct CommandOutput {
    pub(crate) kind: OutputKind,
    pub(crate) title: String,
    pub(crate) subtitle: String,
    pub(crate) stdout: Option<OutputSection>,
    pub(crate) stderr: Option<OutputSection>,
    pub(crate) use_markdown: bool,
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
        }
    }

    pub(crate) fn from_process(command_line: &str, subcommand: &str, output: Output) -> Self {
        let stdout = OutputSection::from_bytes("stdout", &output.stdout);
        let stderr = OutputSection::from_bytes("stderr", &output.stderr);
        let kind = if output.status.success() {
            OutputKind::Success
        } else {
            OutputKind::Error
        };
        let title = if output.status.success() {
            format!("{} completed", command_title(subcommand))
        } else {
            format!("{} failed", command_title(subcommand))
        };
        let subtitle = format!("{command_line} · {}", format_exit_status(&output.status));
        let use_markdown = matches!(subcommand, "scrape" | "ask" | "research");

        Self {
            kind,
            title,
            subtitle,
            stdout,
            stderr,
            use_markdown,
        }
    }

    pub(crate) fn has_body(&self) -> bool {
        self.stdout.is_some() || self.stderr.is_some()
    }
}

impl OutputSection {
    fn from_bytes(label: &'static str, bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }
        let raw = String::from_utf8_lossy(bytes);
        Some(Self::new(label, strip_ansi(raw.trim_end())))
    }

    fn new(label: &'static str, text: impl Into<String>) -> Self {
        let text = truncate_output(text.into());
        let line_count = text.lines().count().max(1);
        Self {
            label,
            text,
            line_count,
        }
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

/// Strip ANSI CSI escape sequences (e.g. `\x1b[1;31m`, `\x1b[0m`).
fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Consume until ANSI final byte: 0x40–0x7E (includes letters
                // AND punctuation such as `~`, `|`, `@`, etc.).
                for ch in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&ch) {
                        break;
                    }
                }
            } else {
                chars.next(); // skip the single non-CSI escape char
            }
        } else {
            out.push(c);
        }
    }
    out
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
