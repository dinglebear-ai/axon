#[cfg(test)]
use std::process::ExitStatus;

use gpui::SharedString;

#[cfg(test)]
#[path = "output_tests.rs"]
mod tests;

mod formatting;

#[cfg(test)]
use formatting::{
    actionable_error_text, ask_answer, crawl_summary, drop_cli_scaffolding, format_exit_status,
    palette_output_text, scrape_body, strip_ansi,
};
use formatting::{command_title, map_url_listing, rest_output_text, truncate_output};

use crate::actions::CommandAction;
use crate::markdown::MarkdownDocument;
use crate::rest_client::RestOutput;
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
    /// Optional pre-fetched PNG image (e.g. screenshot artifact).
    pub(crate) image: Option<std::sync::Arc<gpui::Image>>,
}

#[cfg(test)]
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

    #[cfg(test)]
    pub(crate) fn from_process(
        command_line: &str,
        subcommand: &str,
        output: BoundedProcessOutput,
    ) -> Self {
        let use_markdown = matches!(
            subcommand,
            "scrape" | "ask" | "research" | "summarize" | "retrieve"
        );
        let stdout = OutputSection::from_bytes_for_command(
            "stdout",
            subcommand,
            &output.stdout,
            use_markdown,
        );
        let success = output.status.success();
        let stderr = if success {
            None
        } else {
            OutputSection::from_bytes("stderr", &output.stderr)
                .map(|section| section.with_text(actionable_error_text(&section.text), false))
        };
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

    pub(crate) fn from_rest(
        command_line: &str,
        subcommand: &str,
        output: RestOutput,
        image_bytes: Option<Vec<u8>>,
    ) -> Self {
        let use_markdown = matches!(
            subcommand,
            "scrape" | "ask" | "research" | "summarize" | "retrieve"
        );
        let stdout = output.stdout.as_deref().and_then(|text| {
            let text = rest_output_text(subcommand, text);
            let section =
                OutputSection::from_text_for_command("stdout", subcommand, &text, use_markdown)?;
            Some(match image_bytes {
                Some(bytes) if subcommand == "screenshot" => section.with_image(bytes),
                _ => section,
            })
        });
        let stderr = output.stderr.as_deref().and_then(|text| {
            let text = rest_output_text(subcommand, text);
            OutputSection::from_text("errors", &text, false)
        });
        let title = if output.ok {
            format!("{} completed", command_title(subcommand))
        } else {
            format!("{} failed", command_title(subcommand))
        };
        Self {
            kind: if output.ok {
                OutputKind::Success
            } else {
                OutputKind::Error
            },
            title,
            subtitle: format!("{command_line} · HTTP {}", output.status),
            stdout,
            stderr,
            use_markdown,
            compact_stdout: output.ok,
        }
    }

    pub(crate) fn has_body(&self) -> bool {
        self.stdout.is_some() || self.stderr.is_some()
    }
}

impl OutputSection {
    fn from_text_for_command(
        label: &'static str,
        subcommand: &str,
        text: &str,
        use_markdown: bool,
    ) -> Option<Self> {
        let section = Self::from_text(label, text, use_markdown)?;
        let section = if subcommand == "map" {
            section.with_text(map_url_listing(&section.text), use_markdown)
        } else {
            section
        };
        Some(if use_markdown {
            section.with_markdown()
        } else {
            section
        })
    }

    fn from_text(label: &'static str, text: &str, use_markdown: bool) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            Some(Self::build(
                label,
                truncate_output(text.to_string()),
                use_markdown,
            ))
        }
    }

    #[cfg(test)]
    fn from_bytes_for_command(
        label: &'static str,
        subcommand: &str,
        bytes: &[u8],
        use_markdown: bool,
    ) -> Option<Self> {
        Self::from_bytes(label, bytes).map(|section| {
            let section = if subcommand == "map" {
                section.with_text(map_url_listing(&section.text), use_markdown)
            } else if matches!(
                subcommand,
                "ask" | "crawl" | "embed" | "extract" | "ingest" | "scrape" | "search"
            ) {
                section.with_text(palette_output_text(subcommand, &section.text), use_markdown)
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

    #[cfg(test)]
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
            image: None,
        }
    }

    /// Attach a pre-fetched PNG image to this section (e.g. from a screenshot artifact).
    pub(super) fn with_image(mut self, bytes: Vec<u8>) -> Self {
        self.image = Some(std::sync::Arc::new(gpui::Image::from_bytes(
            gpui::ImageFormat::Png,
            bytes,
        )));
        self
    }
}

#[cfg(test)]
struct BoundedByteBuffer {
    bytes: Vec<u8>,
    limit: usize,
    truncated: bool,
}

#[cfg(test)]
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

#[cfg(test)]
fn valid_utf8_boundary(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(error) => error.valid_up_to(),
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

#[cfg(test)]
fn success_status() -> std::process::ExitStatus {
    #[cfg(windows)]
    {
        std::process::Command::new("cmd")
            .args(["/C", "exit", "0"])
            .status()
            .expect("success status")
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("true")
            .status()
            .expect("success status")
    }
}
