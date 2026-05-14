//! Aurora palette — ANSI 256 constants matching lab's aurora palette exactly.
//!
//! Source of truth: `lab/crates/lab/src/output/theme.rs`
//! Cross-reference: `aurora-design-system/registry/aurora/styles/aurora.css`
//!
//! | Const          | ANSI 256 | TrueColor RGB   | CSS token               | CSS hex  |
//! |----------------|----------|-----------------|-------------------------|----------|
//! | SERVICE_NAME   | 211      | (255, 175, 215) | --aurora-accent-pink    | #f9a8c4  |
//! | ACCENT_PRIMARY | 39       | (41, 182, 246)  | --aurora-accent-primary | #29b6f6  |
//! | TEXT_MUTED     | 250      | (167, 188, 201) | --aurora-text-muted     | #a7bcc9  |
//! | SUCCESS        | 115      | (125, 211, 199) | --aurora-success        | #7dd3c7  |
//! | WARN           | 180      | (198, 163, 107) | --aurora-warn           | #c6a36b  |
//! | ERROR          | 174      | (199, 132, 144) | --aurora-error          | #c78490  |

#![allow(dead_code)]

/// Pink — service names and first token of log messages. RGB (255, 175, 215).
pub const SERVICE_NAME: u8 = 211;

/// Bright blue — primary action/route/tool identifiers. RGB (41, 182, 246).
pub const ACCENT_PRIMARY: u8 = 39;

/// Light grey — secondary metadata and muted text. RGB (167, 188, 201).
pub const TEXT_MUTED: u8 = 250;

/// Teal — success states and HTTP 2xx. RGB (125, 211, 199).
pub const SUCCESS: u8 = 115;

/// Amber — warnings and HTTP 3xx/4xx. RGB (198, 163, 107).
pub const WARN: u8 = 180;

/// Muted red — errors and HTTP 5xx. RGB (199, 132, 144).
pub const ERROR: u8 = 174;
