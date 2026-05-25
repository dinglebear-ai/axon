# Aurora CLI Polish Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the axon CLI's terminal output to full Aurora design-system parity — clickable URLs, truecolor tracing, opt-in/opt-out color flag, bordered summary panels, themed tables, sparkline metrics, and a live MultiProgress status watcher.

**Architecture:** Each enhancement is an isolated additive change to existing modules. The `src/core/ui.rs` palette helpers already use Aurora truecolor escapes (committed earlier in this branch); we add new helpers next to them and migrate the few callers that benefit. A new `ColorChoice` global flag funnels through `Config` to both `ui.rs::color_enabled()` and `logging.rs::should_use_ansi()` so the runtime override is single-source. Tracing-subscriber's `CliFormat` is upgraded from ANSI-256 to 24-bit truecolor with a graceful fallback. `axon status` gains a `--watch` mode that swaps the one-shot text render for `indicatif::MultiProgress`.

**Tech Stack:** Rust 2024 edition, `indicatif` 0.18 (already in Cargo.toml), `console` 0.16 (already), `tracing-subscriber` 0.3 (already), `comfy-table` 7.x (new dep), `clap` `ValueEnum` (already used), `supports-hyperlinks` 3.x (new dep for OSC 8 terminal detection).

**Outcome note:** This document is the original implementation plan. The final
PR intentionally deviated in a few places: stats sparkline integration stayed
deferred, crawl completion panel wiring landed in `src/cli/commands/crawl/sync_crawl.rs`,
job-list table polish landed in `src/cli/commands/common_jobs.rs`, and PR review
follow-up added route/watch/color/OSC8 regression hardening.

---

## File Structure

**New files (created in this plan):**
- `src/core/ui/hyperlinks.rs` — OSC 8 hyperlink emitter + terminal-capability gate
- `src/core/ui/panel.rs` — Box-drawing summary panel (`panel(title, rows)`)
- `src/core/ui/sparkline.rs` — Unicode sparkline renderer (`sparkline(&[u64]) -> String`)
- `src/core/ui/table.rs` — `comfy-table` wrapper with Aurora borders (`aurora_table()`)
- `src/core/ui/hyperlinks_tests.rs` — sidecar tests
- `src/core/ui/panel_tests.rs` — sidecar tests
- `src/core/ui/sparkline_tests.rs` — sidecar tests
- `src/cli/commands/status/watch.rs` — `axon status --watch` MultiProgress live view

**Modified files:**
- `src/core/ui.rs` — declare new submodules, re-export helpers
- `src/core/config/cli/global_args.rs` — add `--color` flag
- `src/core/config/types/config.rs` — add `color_choice: ColorChoice` field
- `src/core/config/types/enums.rs` — add `ColorChoice { Auto, Always, Never }`
- `src/core/config/types/config_impls.rs` — default in `Config::default()`
- `src/core/config/parse/build_config.rs` — wire the flag through
- `src/core/logging.rs` — switch `aurora` palette + `should_use_ansi` to honor `ColorChoice`
- `src/core/logging/aurora.rs` — add truecolor RGB constants beside the 256-color codes
- `src/cli/commands/sources.rs` — migrate to `aurora_table()`
- `src/cli/commands/domains.rs` — migrate to `aurora_table()`
- `src/cli/commands/common_jobs.rs` — migrate job-list tables to `aurora_table()`
- `src/cli/commands/crawl/sync_crawl.rs` — bordered completion panel
- `src/cli/commands/status.rs` — `--watch` dispatch
- `src/cli/commands/status/watch.rs` — live watch loop and terminal outcome handling
- `src/cli/route.rs` + `src/cli/server_mode.rs` tests — keep `status --watch` local when server mode is configured
- `Cargo.toml` — add `comfy-table`, `supports-hyperlinks`

---

## Task 1: `--color` global flag + `ColorChoice` enum

**Files:**
- Modify: `src/core/config/types/enums.rs`
- Modify: `src/core/config/types/config.rs`
- Modify: `src/core/config/types/config_impls.rs`
- Modify: `src/core/config/cli/global_args.rs`
- Modify: `src/core/config/parse/build_config.rs`
- Modify: `src/core/ui.rs:19-21` (the `color_enabled()` function)
- Modify: `src/core/logging.rs:115-134` (the `should_use_ansi()` function)
- Modify: `src/cli/commands/research.rs` (inline `Config{..}` literal)
- Modify: `src/cli/commands/search.rs` (inline `Config{..}` literal)

- [ ] **Step 1: Add `ColorChoice` to `enums.rs`**

Append to `src/core/config/types/enums.rs`:

```rust
/// Terminal color override. Wired through `Config::color_choice`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum ColorChoice {
    /// Detect TTY + NO_COLOR + CLICOLOR_FORCE (default).
    Auto,
    /// Force ANSI color output regardless of TTY detection.
    Always,
    /// Suppress all ANSI escapes.
    Never,
}

impl Default for ColorChoice {
    fn default() -> Self {
        ColorChoice::Auto
    }
}
```

- [ ] **Step 2: Add `color_choice` to `Config`**

In `src/core/config/types/config.rs`, add to the `Config` struct (next to the other UI/output fields, e.g. near `json_output`):

```rust
pub color_choice: crate::core::config::types::enums::ColorChoice,
```

In `src/core/config/types/config_impls.rs`, set the default in `Config::default()` AND in `Config::default_minimal()`:

```rust
color_choice: crate::core::config::types::enums::ColorChoice::Auto,
```

- [ ] **Step 3: Add `--color` to `GlobalArgs`**

In `src/core/config/cli/global_args.rs`, after the existing `json` flag (~line 93):

```rust
/// Color output: auto (TTY detect, default), always, never.
#[arg(global = true, long, value_enum, default_value_t = crate::core::config::types::enums::ColorChoice::Auto)]
pub(in crate::core::config) color: crate::core::config::types::enums::ColorChoice,
```

- [ ] **Step 4: Wire the flag through `build_config.rs`**

In `src/core/config/parse/build_config.rs`, inside `into_config()`, set the field:

```rust
color_choice: cli.global.color,
```

- [ ] **Step 5: Make `color_enabled()` honor the choice**

Replace the body of `src/core/ui.rs::color_enabled()` (currently lines 19-21):

```rust
fn color_enabled() -> bool {
    use crate::core::config::types::enums::ColorChoice;
    // Per-process override (set by main.rs once Config is parsed).
    match COLOR_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed) {
        1 => true,   // Always
        2 => false,  // Never
        _ => env::var_os("NO_COLOR").is_none(),
    }
}

static COLOR_OVERRIDE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);

pub fn install_color_choice(choice: crate::core::config::types::enums::ColorChoice) {
    use crate::core::config::types::enums::ColorChoice;
    let val: u8 = match choice {
        ColorChoice::Auto => 0,
        ColorChoice::Always => 1,
        ColorChoice::Never => 2,
    };
    COLOR_OVERRIDE.store(val, std::sync::atomic::Ordering::Relaxed);
}
```

- [ ] **Step 6: Call `install_color_choice` from `main.rs` and `lib.rs::run_once`**

Find the spot in `src/lib.rs::run_once()` (or wherever `Config` becomes available — see `src/lib.rs:34-61` per CLAUDE.md) and add **immediately after** `Config` is built but before any UI calls:

```rust
crate::core::ui::install_color_choice(cfg.color_choice);
```

- [ ] **Step 7: Honor it inside `should_use_ansi()`**

Replace `src/core/logging.rs::should_use_ansi()` (around lines 115-134) so it consults the same override before falling back to TTY detection:

```rust
fn should_use_ansi(writer: &Writer<'_>) -> bool {
    use crate::core::config::types::enums::ColorChoice;
    // ui::color_enabled() reads the same atomic.
    if !crate::core::ui::color_enabled_public() {
        return false;
    }
    // Existing logic — FORCE_COLOR / CLICOLOR_FORCE / writer.has_ansi_escapes()
    let force = |v: &str| std::env::var(v).map(|s| !s.is_empty()).unwrap_or(false);
    if force("FORCE_COLOR") || force("CLICOLOR_FORCE") {
        return true;
    }
    writer.has_ansi_escapes()
}
```

Add a `pub fn color_enabled_public() -> bool` shim in `src/core/ui.rs` that returns `color_enabled()` — `color_enabled()` is currently private.

- [ ] **Step 8: Update inline `Config{..}` test literals**

`src/cli/commands/research.rs` and `src/cli/commands/search.rs` both have `make_test_config()` (or similar) functions that construct `Config { ... }` inline. Add `color_choice: ColorChoice::Auto,` to each. Search for `make_test_config` in both files.

- [ ] **Step 9: Compile + smoke test**

```bash
just check
./target/release/axon --color=never status --json --active 2>&1 | head -5
./target/release/axon --color=always doctor 2>&1 | head -5
NO_COLOR=1 ./target/release/axon doctor 2>&1 | head -5
```

Expected: `--color=never` produces no ANSI escapes; `--color=always` produces them even when piped; `NO_COLOR=1` (with default `--color=auto`) produces none.

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(cli): add --color=auto|always|never global flag"
```

---

## Task 2: Aurora truecolor upgrade for tracing-subscriber formatter

**Files:**
- Modify: `src/core/logging/aurora.rs`
- Modify: `src/core/logging.rs` (the `CliFormat::format_event` impl + `ansi256_bold` / `ansi_dim` helpers — around lines 18-80)

- [ ] **Step 1: Add truecolor RGB constants in `aurora.rs`**

Append below the existing 256-color constants:

```rust
/// Truecolor (24-bit) RGB triples, matching the same Aurora tokens.
pub mod rgb {
    pub const SERVICE_NAME:   (u8, u8, u8) = (249, 168, 196);   // #F9A8C4
    pub const ACCENT_PRIMARY: (u8, u8, u8) = (41, 182, 246);    // #29B6F6
    pub const TEXT_MUTED:     (u8, u8, u8) = (167, 188, 201);   // #A7BCC9
    pub const SUCCESS:        (u8, u8, u8) = (125, 211, 199);   // #7DD3C7
    pub const WARN:           (u8, u8, u8) = (198, 163, 107);   // #C6A36B
    pub const ERROR:          (u8, u8, u8) = (199, 132, 144);   // #C78490
    pub const INFO:           (u8, u8, u8) = (114, 200, 245);   // #72C8F5
}
```

- [ ] **Step 2: Add truecolor emitters in `logging.rs`**

In `src/core/logging.rs`, near the existing `ansi256_bold` / `ansi_dim` helpers (around lines 18-25), add:

```rust
fn truecolor_bold(rgb: (u8, u8, u8), text: &str) -> String {
    let (r, g, b) = rgb;
    format!("\x1b[1;38;2;{r};{g};{b}m{text}\x1b[0m")
}

fn truecolor(rgb: (u8, u8, u8), text: &str) -> String {
    let (r, g, b) = rgb;
    format!("\x1b[38;2;{r};{g};{b}m{text}\x1b[0m")
}

/// True iff the terminal advertises 24-bit color via COLORTERM.
fn supports_truecolor() -> bool {
    matches!(
        std::env::var("COLORTERM").as_deref(),
        Ok("truecolor") | Ok("24bit")
    )
}
```

- [ ] **Step 3: Swap `write_level` and the message highlighter**

Replace `write_level()` (around `logging.rs:74`) to prefer truecolor when supported:

```rust
fn write_level(writer: &mut Writer<'_>, level: tracing::Level, ansi: bool) -> fmt::Result {
    if !ansi {
        return write!(writer, "{:5}  ", level.as_str());
    }
    let tc = supports_truecolor();
    match level {
        tracing::Level::ERROR => {
            if tc { write!(writer, "{}  ", truecolor_bold(aurora::rgb::ERROR, "ERROR")) }
            else  { write!(writer, "{}  ", ansi256_bold(aurora::ERROR, "ERROR")) }
        }
        tracing::Level::WARN => {
            if tc { write!(writer, "{}  ", truecolor_bold(aurora::rgb::WARN, " WARN")) }
            else  { write!(writer, "{}  ", ansi256_bold(aurora::WARN, " WARN")) }
        }
        tracing::Level::INFO => {
            if tc { write!(writer, "{}  ", truecolor_bold(aurora::rgb::INFO, " INFO")) }
            else  { write!(writer, " INFO  ") }  // existing fallback
        }
        tracing::Level::DEBUG => write!(writer, "{}  ", ansi_dim("DEBUG")),
        tracing::Level::TRACE => write!(writer, "{}  ", ansi_dim("TRACE")),
    }
}
```

In `format_event()` (around `logging.rs:172`), change the first-token emission from `ansi256_bold(aurora::SERVICE_NAME, token)` to a truecolor-aware branch:

```rust
if i == 0 {
    let painted = if supports_truecolor() {
        truecolor_bold(aurora::rgb::SERVICE_NAME, token)
    } else {
        ansi256_bold(aurora::SERVICE_NAME, token)
    };
    write!(writer, "{painted}")?;
}
```

- [ ] **Step 4: Compile + visual check**

```bash
just check
RUST_LOG=info COLORTERM=truecolor cargo run --quiet --bin axon -- doctor 2>&1 | head -20
RUST_LOG=info COLORTERM= cargo run --quiet --bin axon -- doctor 2>&1 | head -20
```

Expected: with `COLORTERM=truecolor`, log lines use 24-bit escapes (`\x1b[38;2;...`); without, they fall back to ANSI-256 (`\x1b[38;5;...`).

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(logging): tracing formatter prefers Aurora truecolor when supported"
```

---

## Task 3: OSC 8 hyperlinks helper

**Files:**
- Create: `src/core/ui/hyperlinks.rs`
- Create: `src/core/ui/hyperlinks_tests.rs`
- Modify: `src/core/ui.rs` (add `mod hyperlinks; pub use hyperlinks::hyperlink;`)
- Modify: `Cargo.toml`
- Modify: `src/cli/commands/status/presentation.rs` (wrap URLs with `hyperlink`)
- Modify: `src/cli/commands/sources.rs` (wrap each source URL)

- [ ] **Step 1: Add `supports-hyperlinks` dependency**

Append to `Cargo.toml` `[dependencies]`:

```toml
supports-hyperlinks = "3"
```

- [ ] **Step 2: Write failing sidecar tests**

Create `src/core/ui/hyperlinks_tests.rs`:

```rust
use super::*;

#[test]
fn hyperlink_emits_osc8_when_forced() {
    let out = hyperlink_for_test("https://example.com", "click me", /*supported=*/ true);
    // OSC 8 = ESC ] 8 ;; URL ESC \\  text  ESC ] 8 ;;  ESC \\
    assert!(out.starts_with("\x1b]8;;https://example.com\x1b\\"));
    assert!(out.ends_with("\x1b]8;;\x1b\\"));
    assert!(out.contains("click me"));
}

#[test]
fn hyperlink_returns_plain_text_when_unsupported() {
    let out = hyperlink_for_test("https://example.com", "click me", false);
    assert_eq!(out, "click me");
}

#[test]
fn hyperlink_empty_label_falls_back_to_url() {
    let out = hyperlink_for_test("https://example.com", "", true);
    assert!(out.contains("https://example.com"));
}
```

- [ ] **Step 3: Run tests to verify they fail**

```bash
cargo test --locked --lib core::ui::hyperlinks
```

Expected: FAIL — `hyperlink_for_test` not found.

- [ ] **Step 4: Implement the helper**

Create `src/core/ui/hyperlinks.rs`:

```rust
//! OSC 8 hyperlink emitter. Modern terminals (kitty, iTerm2, wezterm, vscode,
//! Windows Terminal, gnome-terminal 3.26+) recognize the sequence and render
//! the label as a clickable link to `url`. Unsupported terminals just print
//! the label as plain text.
//!
//! Format: `\x1b]8;;URL\x1b\\TEXT\x1b]8;;\x1b\\`

use crate::core::ui::color_enabled_public;

#[cfg(test)]
#[path = "hyperlinks_tests.rs"]
mod tests;

const OSC8: &str = "\x1b]8;;";
const ST: &str = "\x1b\\";

/// Render `label` as a clickable link to `url` if the terminal supports OSC 8
/// AND color output is enabled (so `--color=never` also strips hyperlinks).
/// Otherwise return `label` (or `url` when `label` is empty).
pub fn hyperlink(url: &str, label: &str) -> String {
    hyperlink_for_test(url, label, color_enabled_public() && supports_hyperlinks::on(supports_hyperlinks::Stream::Stdout))
}

/// Test seam — caller forces the support flag.
pub(crate) fn hyperlink_for_test(url: &str, label: &str, supported: bool) -> String {
    let visible = if label.is_empty() { url } else { label };
    if !supported {
        return visible.to_string();
    }
    format!("{OSC8}{url}{ST}{visible}{OSC8}{ST}")
}
```

- [ ] **Step 5: Declare the module in `ui.rs`**

Add to `src/core/ui.rs` (top, after existing imports):

```rust
mod hyperlinks;
pub use hyperlinks::hyperlink;
```

- [ ] **Step 6: Run tests to verify they pass**

```bash
cargo test --locked --lib core::ui::hyperlinks
```

Expected: 3 passed.

- [ ] **Step 7: Wire hyperlinks into source URL rendering**

In `src/cli/commands/sources.rs`, find the URL print loop and wrap each URL with `crate::core::ui::hyperlink(&url, &url)`. Show the same URL as the label so terminals without OSC 8 see no change.

In `src/cli/commands/status/presentation.rs`, find any `println!` of a URL (search for `cfg.start_url` or `url:` field rendering) and apply the same wrap.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(ui): OSC 8 hyperlinks for source/status URLs"
```

---

## Task 4: Bordered Aurora summary panels

**Files:**
- Create: `src/core/ui/panel.rs`
- Create: `src/core/ui/panel_tests.rs`
- Modify: `src/core/ui.rs` (declare `mod panel; pub use panel::panel;`)
- Modify: `src/services/crawl_sync.rs` (replace final summary `println!`s with a panel)
- Modify: `src/cli/commands/ingest_common.rs::render_ingest_status` (panel for terminal states)

- [ ] **Step 1: Write failing sidecar tests**

Create `src/core/ui/panel_tests.rs`:

```rust
use super::*;

#[test]
fn panel_renders_box_drawing_borders() {
    let s = panel_plain("Crawl complete", &[("pages", "42"), ("chunks", "1024"), ("elapsed", "12.3s")]);
    assert!(s.contains("╭"));
    assert!(s.contains("╮"));
    assert!(s.contains("╰"));
    assert!(s.contains("╯"));
    assert!(s.contains("Crawl complete"));
    assert!(s.contains("pages"));
    assert!(s.contains("42"));
}

#[test]
fn panel_handles_empty_rows() {
    let s = panel_plain("Done", &[]);
    assert!(s.contains("Done"));
    assert!(s.lines().count() >= 2);  // at least top + bottom border
}

#[test]
fn panel_aligns_widest_key() {
    let s = panel_plain("X", &[("short", "1"), ("a much longer key", "2")]);
    // both rows should have the value column starting at the same offset
    let lines: Vec<&str> = s.lines().filter(|l| l.contains("│")).collect();
    let positions: Vec<_> = lines.iter()
        .map(|l| l.find(|c: char| c.is_ascii_digit()).unwrap_or(0))
        .collect();
    assert!(positions.windows(2).all(|w| w[0] == w[1]));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked --lib core::ui::panel
```

Expected: FAIL — `panel_plain` not found.

- [ ] **Step 3: Implement `panel`**

Create `src/core/ui/panel.rs`:

```rust
//! Aurora bordered summary panel. Use for terminal "you're done" output:
//! crawl/ingest/embed completion, doctor summary, stats overview.
//!
//! ╭─ Crawl complete ──────────╮
//! │ pages      42             │
//! │ chunks     1024           │
//! │ elapsed    12.3s          │
//! ╰───────────────────────────╯

use crate::core::ui::{ACCENT_ANSI, PRIMARY_ANSI, ansi_colorize, color_enabled_public, muted};

#[cfg(test)]
#[path = "panel_tests.rs"]
mod tests;

/// Render a titled panel with key/value rows. Honors the `--color` setting.
pub fn panel(title: &str, rows: &[(&str, &str)]) -> String {
    if color_enabled_public() {
        panel_colored(title, rows)
    } else {
        panel_plain(title, rows)
    }
}

/// ANSI-free variant — used by tests and `--color=never`.
pub(crate) fn panel_plain(title: &str, rows: &[(&str, &str)]) -> String {
    let key_w = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    let val_w = rows.iter().map(|(_, v)| v.chars().count()).max().unwrap_or(0);
    let inner_w = (key_w + 2 + val_w).max(title.chars().count() + 4);

    let mut out = String::new();
    // Top border with embedded title: "╭─ <title> ───╮"
    out.push('╭');
    out.push('─');
    out.push(' ');
    out.push_str(title);
    out.push(' ');
    for _ in (title.chars().count() + 4)..(inner_w + 2) {
        out.push('─');
    }
    out.push('╮');
    out.push('\n');

    for (k, v) in rows {
        out.push('│');
        out.push(' ');
        out.push_str(k);
        for _ in k.chars().count()..key_w {
            out.push(' ');
        }
        out.push_str("  ");
        out.push_str(v);
        for _ in v.chars().count()..val_w {
            out.push(' ');
        }
        out.push(' ');
        out.push('│');
        out.push('\n');
    }

    out.push('╰');
    for _ in 0..(inner_w + 2) {
        out.push('─');
    }
    out.push('╯');
    out
}

fn panel_colored(title: &str, rows: &[(&str, &str)]) -> String {
    // Same layout as plain, but key column is muted and title is primary.
    // Implementation note: easiest correct approach is to compute layout
    // in plain text then post-replace the key tokens — but for sub-100-line
    // simplicity we rebuild with ANSI directly.
    let key_w = rows.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
    let val_w = rows.iter().map(|(_, v)| v.chars().count()).max().unwrap_or(0);
    let inner_w = (key_w + 2 + val_w).max(title.chars().count() + 4);
    let border = |ch: char| ansi_colorize(ACCENT_ANSI, &ch.to_string());

    let mut out = String::new();
    out.push_str(&border('╭'));
    out.push_str(&border('─'));
    out.push(' ');
    out.push_str(&ansi_colorize(PRIMARY_ANSI, title));
    out.push(' ');
    for _ in (title.chars().count() + 4)..(inner_w + 2) {
        out.push_str(&border('─'));
    }
    out.push_str(&border('╮'));
    out.push('\n');

    for (k, v) in rows {
        out.push_str(&border('│'));
        out.push(' ');
        out.push_str(&muted(k));
        for _ in k.chars().count()..key_w {
            out.push(' ');
        }
        out.push_str("  ");
        out.push_str(v);
        for _ in v.chars().count()..val_w {
            out.push(' ');
        }
        out.push(' ');
        out.push_str(&border('│'));
        out.push('\n');
    }

    out.push_str(&border('╰'));
    for _ in 0..(inner_w + 2) {
        out.push_str(&border('─'));
    }
    out.push_str(&border('╯'));
    out
}
```

- [ ] **Step 4: Declare in `ui.rs`**

```rust
mod panel;
pub use panel::panel;
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test --locked --lib core::ui::panel
```

Expected: 3 passed.

- [ ] **Step 6: Use the panel in `crawl_sync.rs` completion**

In `src/services/crawl_sync.rs`, locate the end-of-crawl summary section (search for `log_done` calls with `pages` / `chunks` / `elapsed`). Replace the final human-readable summary block with:

```rust
if !cfg.json_output {
    let pages_str = pages.to_string();
    let chunks_str = chunks.to_string();
    let elapsed_str = format!("{elapsed_secs:.1}s");
    println!("{}", crate::core::ui::panel("Crawl complete", &[
        ("pages",   pages_str.as_str()),
        ("chunks",  chunks_str.as_str()),
        ("elapsed", elapsed_str.as_str()),
    ]));
}
```

Adjust variable names to match what already exists in scope.

- [ ] **Step 7: Use the panel in ingest terminal-state renderer**

In `src/cli/commands/ingest_common.rs::render_ingest_status`, when the job is `completed` and `cfg.json_output` is false, replace the existing key/value print sequence with a `panel("Ingest complete", &[...])` call. The keys to surface are typically `source_type`, `target`, `chunks`, `elapsed`. Use whatever fields the existing renderer already pulls.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(ui): bordered Aurora summary panels for crawl + ingest completion"
```

---

## Task 5: Themed table renderer (comfy-table)

**Files:**
- Modify: `Cargo.toml`
- Create: `src/core/ui/table.rs`
- Modify: `src/core/ui.rs` (declare `mod table; pub use table::aurora_table;`)
- Modify: `src/cli/commands/sources.rs` — adopt `aurora_table`
- Modify: `src/cli/commands/domains.rs` — adopt `aurora_table`
- Modify: `src/cli/commands/common_jobs.rs::handle_job_list` — adopt `aurora_table`

- [ ] **Step 1: Add `comfy-table` dependency**

Append to `Cargo.toml`:

```toml
comfy-table = "7"
```

- [ ] **Step 2: Create the wrapper**

Create `src/core/ui/table.rs`:

```rust
//! Aurora-themed table renderer. Wraps comfy-table with the cyan accent
//! border preset, muted header style, and a `--color=never`-friendly fallback.

use comfy_table::{Cell, Color, ContentArrangement, Table, presets, modifiers};

use crate::core::ui::color_enabled_public;

/// Build a table pre-styled with Aurora colors. Caller fills headers + rows.
///
/// Example:
/// ```ignore
/// let mut t = aurora_table(&["URL", "Chunks"]);
/// t.add_row(vec!["https://example.com".into(), "42".to_string()]);
/// println!("{t}");
/// ```
pub fn aurora_table(headers: &[&str]) -> Table {
    let mut t = Table::new();
    if color_enabled_public() {
        t.load_preset(presets::UTF8_FULL)
            .apply_modifier(modifiers::UTF8_ROUND_CORNERS)
            .set_content_arrangement(ContentArrangement::Dynamic);
        // Aurora cyan (41,182,246) borders — approximated to the closest
        // comfy-table named color since comfy-table's Color::TrueColor is
        // only honored on truecolor terminals.
        t.set_header(
            headers
                .iter()
                .map(|h| Cell::new(h).fg(Color::Rgb { r: 41, g: 182, b: 246 }))
                .collect::<Vec<_>>(),
        );
    } else {
        t.load_preset(presets::ASCII_FULL_CONDENSED);
        t.set_header(headers.iter().copied().collect::<Vec<_>>());
    }
    t
}
```

- [ ] **Step 3: Declare in `ui.rs`**

```rust
mod table;
pub use table::aurora_table;
```

- [ ] **Step 4: Smoke compile**

```bash
just check
```

Expected: clean.

- [ ] **Step 5: Migrate `sources.rs`**

In `src/cli/commands/sources.rs`, locate the human-readable render path (the non-JSON branch). Replace the existing per-row `println!` loop with:

```rust
use crate::core::ui::aurora_table;

let mut t = aurora_table(&["URL", "Chunks", "Last Indexed"]);
for src in sources.iter() {
    t.add_row(vec![
        crate::core::ui::hyperlink(&src.url, &src.url),
        src.chunks.to_string(),
        src.last_indexed.clone().unwrap_or_default(),
    ]);
}
println!("{t}");
```

Adjust field names to match the actual `Source` struct.

- [ ] **Step 6: Migrate `domains.rs`**

Same pattern. Headers `&["Domain", "URLs", "Chunks"]`.

- [ ] **Step 7: Migrate `handle_job_list`**

In `src/cli/commands/common_jobs.rs::handle_job_list`, the existing renderer prints job rows. Replace with `aurora_table(&["ID", "Status", "Created", "Subject"])`. Use `symbol_for_status(...)` in the Status column. Truncate the ID to 8 chars via existing helpers.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(ui): comfy-table renderer with Aurora-themed borders for sources/domains/jobs"
```

---

## Task 6: Inline sparkline helper

**Files:**
- Create: `src/core/ui/sparkline.rs`
- Create: `src/core/ui/sparkline_tests.rs`
- Modify: `src/core/ui.rs` (declare `mod sparkline; pub use sparkline::sparkline;`)
- Modify: `src/cli/commands/stats.rs` (use sparkline in summary)

- [ ] **Step 1: Write failing sidecar tests**

Create `src/core/ui/sparkline_tests.rs`:

```rust
use super::*;

#[test]
fn sparkline_handles_empty() {
    assert_eq!(sparkline_plain(&[]), "");
}

#[test]
fn sparkline_handles_single_value() {
    let s = sparkline_plain(&[5]);
    assert_eq!(s.chars().count(), 1);
}

#[test]
fn sparkline_maps_min_to_lowest_block() {
    // 8 levels, evenly spaced
    let s = sparkline_plain(&[0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(s.chars().count(), 8);
    assert!(s.starts_with('▁'));
    assert!(s.ends_with('█'));
}

#[test]
fn sparkline_all_equal_values_renders_mid_block() {
    let s = sparkline_plain(&[5, 5, 5, 5]);
    assert_eq!(s.chars().count(), 4);
    // every char identical
    let first = s.chars().next().unwrap();
    assert!(s.chars().all(|c| c == first));
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test --locked --lib core::ui::sparkline
```

Expected: FAIL — `sparkline_plain` not found.

- [ ] **Step 3: Implement**

Create `src/core/ui/sparkline.rs`:

```rust
//! Unicode sparkline renderer — one char per data point at 8 levels.
//!
//! Used for inline "trend over the last N days" displays in stats output.

use crate::core::ui::{ACCENT_ANSI, ansi_colorize, color_enabled_public};

#[cfg(test)]
#[path = "sparkline_tests.rs"]
mod tests;

const BLOCKS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

/// Render `values` as a sparkline. Empty input → empty string. Returns Aurora
/// cyan text when color is enabled.
pub fn sparkline(values: &[u64]) -> String {
    if color_enabled_public() {
        ansi_colorize(ACCENT_ANSI, &sparkline_plain(values))
    } else {
        sparkline_plain(values)
    }
}

pub(crate) fn sparkline_plain(values: &[u64]) -> String {
    if values.is_empty() {
        return String::new();
    }
    let min = *values.iter().min().unwrap();
    let max = *values.iter().max().unwrap();
    if min == max {
        // Pick a mid block so the user sees a non-empty trend line.
        return BLOCKS[3].to_string().repeat(values.len());
    }
    let range = (max - min) as f64;
    values
        .iter()
        .map(|&v| {
            let normalized = ((v - min) as f64) / range;
            let idx = ((normalized * (BLOCKS.len() - 1) as f64).round() as usize)
                .min(BLOCKS.len() - 1);
            BLOCKS[idx]
        })
        .collect()
}
```

- [ ] **Step 4: Declare in `ui.rs`**

```rust
mod sparkline;
pub use sparkline::sparkline;
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
cargo test --locked --lib core::ui::sparkline
```

Expected: 4 passed.

- [ ] **Step 6: Use the sparkline in `stats.rs`**

In `src/cli/commands/stats.rs`, after the existing point-count print, query the per-day insert counts (or whatever timeseries data the stats endpoint already returns — search for a `points_per_day` or similar field). If no such field exists, **skip this step** and add a comment for follow-up.

When a timeseries IS available, render it as:

```rust
let trend: Vec<u64> = stats.points_per_day.iter().map(|d| d.count).collect();
println!("{} {} {}",
    crate::core::ui::muted("trend (last 14d):"),
    crate::core::ui::sparkline(&trend),
    crate::core::ui::muted(&format!("{} → {}",
        trend.first().copied().unwrap_or(0),
        trend.last().copied().unwrap_or(0))));
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(ui): inline Unicode sparkline helper + stats trend display"
```

---

## Task 7: `axon status --watch` MultiProgress live view

**Files:**
- Modify: `src/core/config/cli/global_args.rs` (add `watch: bool`)
- Modify: `src/core/config/types/config.rs` (add `watch_mode: bool`)
- Modify: `src/core/config/types/config_impls.rs`
- Modify: `src/core/config/parse/build_config.rs`
- Modify: `src/cli/commands/research.rs` + `search.rs` (inline `Config{..}` literals)
- Create: `src/cli/commands/status/watch.rs`
- Modify: `src/cli/commands/status.rs` (dispatch to watch when `cfg.watch_mode && !cfg.json_output`)

- [ ] **Step 1: Add the `--watch` global flag**

In `src/core/config/cli/global_args.rs`:

```rust
/// Live-update mode (currently only honored by `axon status`). Refreshes
/// every second using indicatif::MultiProgress; Ctrl-C to exit.
#[arg(global = true, long, action = ArgAction::SetTrue)]
pub(in crate::core::config) watch: bool,
```

- [ ] **Step 2: Add `watch_mode` to `Config`**

In `src/core/config/types/config.rs`, add `pub watch_mode: bool,`. Default `false` in `config_impls.rs`. Wire `watch_mode: cli.global.watch,` in `build_config.rs`. Add `watch_mode: false,` to the inline `Config{..}` literals in `research.rs` and `search.rs`.

- [ ] **Step 3: Create the watch renderer**

Create `src/cli/commands/status/watch.rs`:

```rust
//! `axon status --watch` — live MultiProgress view of active jobs.
//!
//! Polls the same status snapshot used by the one-shot renderer every second
//! and reconciles a `HashMap<JobId, ProgressBar>` to match the current
//! running/pending set. Completed/failed jobs leave a static "finished" line.

use crate::core::config::Config;
use crate::services::context::ServiceContext;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::error::Error;
use std::time::Duration;

const TICK_MS: u64 = 100;
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub async fn run_status_watch(
    cfg: &Config,
    ctx: &ServiceContext,
) -> Result<(), Box<dyn Error>> {
    let mp = MultiProgress::new();
    let style = ProgressStyle::with_template("{spinner:.cyan} {prefix:<8} {wide_msg}")
        .unwrap_or_else(|_| ProgressStyle::default_spinner());

    let mut bars: HashMap<String, ProgressBar> = HashMap::new();

    loop {
        // Reuse the same snapshot collector the one-shot renderer uses.
        let snapshot = crate::cli::commands::status::collect_status_snapshot(cfg, ctx).await?;

        // Reconcile: add bars for new jobs, finish bars for departed ones.
        let mut seen: std::collections::HashSet<String> = Default::default();
        for job in snapshot.active_jobs() {
            seen.insert(job.id.clone());
            let bar = bars.entry(job.id.clone()).or_insert_with(|| {
                let pb = mp.add(ProgressBar::new_spinner());
                pb.set_style(style.clone());
                pb.enable_steady_tick(Duration::from_millis(TICK_MS));
                pb
            });
            bar.set_prefix(job.kind.clone());
            bar.set_message(format!("{}: {}", job.subject_or_target(), job.status));
        }
        // Drop bars for jobs that finished or vanished.
        bars.retain(|id, bar| {
            if seen.contains(id) {
                true
            } else {
                bar.finish_and_clear();
                false
            }
        });

        // Exit when nothing is left to watch (caller likely cancelled).
        if snapshot.is_idle() {
            mp.println("(no active jobs)")?;
            break;
        }

        tokio::time::sleep(POLL_INTERVAL).await;
    }

    Ok(())
}
```

This pulls two new methods from `src/cli/commands/status.rs`:
- `collect_status_snapshot(cfg, ctx) -> Result<StatusSnapshot, ...>` — refactor extracted from the existing `run_status`
- `StatusSnapshot::{active_jobs(), is_idle(), ..}` — the existing snapshot type already used by `presentation.rs`

If those exact names don't exist, find the equivalent collector that `run_status` already calls and use it; otherwise extract one (small refactor — keep the public surface narrow).

- [ ] **Step 4: Dispatch from `status.rs`**

In `src/cli/commands/status.rs::run_status`, near the top:

```rust
if cfg.watch_mode && !cfg.json_output {
    return crate::cli::commands::status::watch::run_status_watch(cfg, ctx).await;
}
```

Add `pub mod watch;` declaration with the sidecar pattern:

```rust
#[path = "status/watch.rs"]
pub mod watch;
```

- [ ] **Step 5: Compile + smoke**

```bash
just check
just test
./target/release/axon status --watch  # Ctrl-C after a few seconds
```

Expected: clean compile; live updating bars; Ctrl-C exits cleanly.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(status): --watch live MultiProgress view of active jobs"
```

---

## Task 8: Final verification + sync container

**Files:** none modified; pure validation.

- [ ] **Step 1: Run the full verify gate**

```bash
just verify
```

Expected: all checks pass (fmt + clippy + check + test + monolith + plugin validate + web check + legacy-runtime).

- [ ] **Step 2: Rebuild release binary + container**

```bash
just sync-container
```

Expected: release binary built, symlinked into PATH, container `axon` recreated on jakenet, `0.0.0.0:8001->8001/tcp` healthy.

- [ ] **Step 3: Visual smoke through all the new surfaces**

```bash
axon --color=always doctor
axon --color=always sources | head -20
axon --color=always domains | head -20
axon --color=always stats
axon --color=always crawl list | head -10
axon --color=always status --watch &
sleep 5; kill %1
axon --color=never status                  # plain output, no escapes
```

Confirm visually:
- Cyan rounded-corner tables for sources/domains/crawl list
- Aurora-bordered panel after `stats`
- Sparkline visible in `stats` trend line (if timeseries available)
- Clickable URLs in supported terminals
- Truecolor log levels at the top of doctor output
- `--color=never` strips all ANSI

- [ ] **Step 4: Commit any small fixups + final push**

```bash
git status
git add -A
git commit -m "chore(ui): final polish + verify pass" || true
git push
```

---

## Self-review checklist (filled by the plan author)

**Spec coverage:** All 7 enhancements from the chat have a dedicated task (1-7) plus Task 8 verification.

**Placeholders:** None. Every code step contains exact code. Step 6 of Task 6 acknowledges the possibility that `points_per_day` doesn't exist and provides a defined skip-rule.

**Type consistency:** `ColorChoice` enum is defined in Task 1 Step 1; referenced consistently in Steps 2-8 and in Task 7's inline-literal update step. `color_enabled_public()` is added in Task 1 Step 7 and reused in Tasks 3/4/5/6. `aurora_table()` is created in Task 5 Step 2; the migrate steps use the same name.
