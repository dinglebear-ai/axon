//! Central operator-console policy for CLI transports.

use std::io::IsTerminal;

use crate::config::{Config, MotionChoice};

/// Console tracing threshold selected from operator intent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsoleLogLevel {
    Error,
    Warn,
    Info,
    Debug,
}

/// One decision point for progress, diagnostics, and terminal motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConsolePolicy {
    json: bool,
    quiet: bool,
    verbosity: u8,
    stderr_is_tty: bool,
    motion_choice: MotionChoice,
}

impl ConsolePolicy {
    pub fn for_config(config: &Config) -> Self {
        Self::for_stream(config, std::io::stderr().is_terminal())
    }

    pub fn for_stream(config: &Config, stderr_is_tty: bool) -> Self {
        Self {
            json: config.json_output,
            quiet: config.quiet,
            verbosity: config.verbosity,
            stderr_is_tty,
            motion_choice: config.motion_choice,
        }
    }

    /// Live progress is exclusively an interactive-terminal affordance.
    pub fn progress_enabled(self) -> bool {
        !self.json && !self.quiet && self.stderr_is_tty
    }

    /// Animation follows progress visibility and respects explicit/static and
    /// common non-interactive environment signals.
    pub fn motion_enabled(self) -> bool {
        if !self.progress_enabled() {
            return false;
        }
        match self.motion_choice {
            MotionChoice::Always => true,
            MotionChoice::Never => false,
            MotionChoice::Auto => {
                std::env::var_os("CI").is_none() && std::env::var("TERM").as_deref() != Ok("dumb")
            }
        }
    }

    pub fn verbosity(self) -> u8 {
        self.verbosity
    }

    pub fn diagnostic_enabled(self, level: u8) -> bool {
        self.verbosity >= level
    }

    pub fn console_log_level(self) -> ConsoleLogLevel {
        if self.json || self.quiet {
            ConsoleLogLevel::Error
        } else {
            match self.verbosity {
                0 => ConsoleLogLevel::Warn,
                1 => ConsoleLogLevel::Info,
                _ => ConsoleLogLevel::Debug,
            }
        }
    }
}

#[cfg(test)]
#[path = "console_tests.rs"]
mod tests;
