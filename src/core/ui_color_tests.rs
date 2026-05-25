//! Color-choice contract tests for `core::ui`.
//!
//! `COLOR_OVERRIDE` is a process-wide atomic so these tests cannot run in
//! parallel with anything else that mutates it. They share one `#[test]`
//! that exercises every atomic branch sequentially under a guard.
//!
//! Env-var precedence (`NO_COLOR`, `FORCE_COLOR`) is not exercised here
//! because env mutation is `unsafe` under Rust 2024 and the workspace
//! denies `unsafe-code`. Those code paths are covered by manual smoke
//! testing documented in the PR description.

use super::*;
use crate::core::config::ColorChoice;

#[test]
fn color_choice_contract() {
    let _g = COLOR_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let prev = COLOR_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);

    // ── Never disables color unconditionally. ────────────────────────────
    install_color_choice(ColorChoice::Never);
    assert!(!color_enabled_public(), "Never must disable color");
    assert!(
        !color_forced_always(),
        "Never must not report forced-always"
    );

    // ── Always enables color and reports the forced flag. ────────────────
    install_color_choice(ColorChoice::Always);
    assert!(color_enabled_public(), "Always must enable color");
    assert!(
        color_forced_always(),
        "Always must report color_forced_always"
    );

    // ── After Auto, forced flag must clear. ──────────────────────────────
    install_color_choice(ColorChoice::Auto);
    assert!(
        !color_forced_always(),
        "Auto must not report color_forced_always"
    );

    // Restore so other tests aren't poisoned.
    COLOR_OVERRIDE.store(prev, std::sync::atomic::Ordering::Relaxed);
}
