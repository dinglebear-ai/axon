//! Color-choice contract tests for `core::ui`.
//!
//! `COLOR_OVERRIDE` is a process-wide atomic so these tests cannot run in
//! parallel with anything else that mutates it. They share one `#[test]`
//! that exercises every atomic branch sequentially under a guard.
//!
//! Env-var precedence (`NO_COLOR`, `FORCE_COLOR`) is covered through
//! subprocess regression tests so this module can avoid mutating process env.

use super::*;
use crate::config::ColorChoice;

#[test]
fn color_choice_contract() {
    let _g = COLOR_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let prev = COLOR_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);

    // ── Never disables color unconditionally. ────────────────────────────
    install_color_choice(ColorChoice::Never);
    assert!(!color_enabled_public(), "Never must disable color");
    assert!(!stderr_color_enabled(), "Never must disable stderr color");
    assert_eq!(info("info"), "info");
    assert_eq!(neutral("neutral"), "neutral");
    assert!(
        !color_forced_always(),
        "Never must not report forced-always"
    );

    // ── Always enables color and reports the forced flag. ────────────────
    install_color_choice(ColorChoice::Always);
    assert!(color_enabled_public(), "Always must enable color");
    assert!(stderr_color_enabled(), "Always must enable stderr color");
    assert!(info("info").contains("38;2;114;200;245"));
    assert!(neutral("neutral").contains("38;2;145;168;182"));
    assert!(!accent_when(false, "plain").contains("\x1b["));
    assert!(accent_when(true, "color").contains("38;2;41;182;246"));
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
    if !color_env_forced() {
        assert!(
            !color_enabled_for_auto_tty(false),
            "Auto must stay plain for a redirected stream"
        );
    }

    // Restore so other tests aren't poisoned.
    COLOR_OVERRIDE.store(prev, std::sync::atomic::Ordering::Relaxed);
}
