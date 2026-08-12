//! Aurora CLI/editor palette — brightened for dark terminal contrast.
//!
//! Source of truth: `aurora/themes/editors/claude-code/TOKENS.md`
//!
//! | Const          | ANSI 256 | TrueColor RGB   | CSS token               | CSS hex  |
//! |----------------|----------|-----------------|-------------------------|----------|
//! | SERVICE_NAME   | 212      | (255, 126, 182) | CLI rose                | #ff7eb6  |
//! | ACCENT_PRIMARY | 45       | (54, 201, 255)  | CLI cyan                | #36c9ff  |
//! | TEXT_MUTED     | 252      | (207, 224, 236) | CLI inactive text       | #cfe0ec  |
//! | SUCCESS        | 115      | (125, 211, 199) | --aurora-success        | #7dd3c7  |
//! | WARN           | 180      | (198, 163, 107) | --aurora-warn           | #c6a36b  |
//! | ERROR          | 174      | (199, 132, 144) | --aurora-error          | #c78490  |

#![allow(dead_code)]

/// Pink — service names and first token of log messages. RGB (255, 175, 215).
pub const SERVICE_NAME: u8 = 212;

/// Bright blue — primary action/route/tool identifiers. RGB (41, 182, 246).
pub const ACCENT_PRIMARY: u8 = 45;

/// Light grey — secondary metadata and muted text. RGB (167, 188, 201).
pub const TEXT_MUTED: u8 = 252;

/// Teal — success states and HTTP 2xx. RGB (125, 211, 199).
pub const SUCCESS: u8 = 115;

/// Amber — warnings and HTTP 3xx/4xx. RGB (198, 163, 107).
pub const WARN: u8 = 180;

/// Muted red — errors and HTTP 5xx. RGB (199, 132, 144).
pub const ERROR: u8 = 174;

/// Truecolor (24-bit) RGB triples for the same Aurora tokens. Preferred when
/// `COLORTERM=truecolor|24bit` is set; falls back to the ANSI-256 constants.
pub mod rgb {
    pub const SERVICE_NAME: (u8, u8, u8) = (255, 126, 182); // #FF7EB6
    pub const ACCENT_PRIMARY: (u8, u8, u8) = (54, 201, 255); // #36C9FF
    pub const SUCCESS: (u8, u8, u8) = (125, 211, 199); // #7DD3C7
    pub const WARN: (u8, u8, u8) = (198, 163, 107); // #C6A36B
    pub const ERROR: (u8, u8, u8) = (199, 132, 144); // #C78490
    pub const INFO: (u8, u8, u8) = (114, 200, 245); // #72C8F5
}
