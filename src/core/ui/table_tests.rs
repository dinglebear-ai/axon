use super::*;
use crate::core::ui::{COLOR_OVERRIDE, COLOR_TEST_GUARD};

#[test]
fn aurora_table_renders_under_both_color_modes() {
    let _g = COLOR_TEST_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    let prev = COLOR_OVERRIDE.load(std::sync::atomic::Ordering::Relaxed);

    // Always — UTF-8 round-corner preset. Pin column width so dynamic
    // content arrangement (which depends on terminal width) doesn't
    // collapse the cells when stdout is not a TTY (the usual test env).
    COLOR_OVERRIDE.store(1, std::sync::atomic::Ordering::Relaxed);
    let mut t = aurora_table(&["A", "B"]);
    t.set_width(80);
    t.add_row(vec!["x".to_string(), "y".to_string()]);
    let out = t.to_string();
    assert!(!out.is_empty(), "Always mode produced empty table");
    assert!(
        out.contains('╭'),
        "color mode must emit UTF-8 round corners, got: {out}"
    );

    // Never — ASCII preset, no UTF-8 round corners.
    COLOR_OVERRIDE.store(2, std::sync::atomic::Ordering::Relaxed);
    let mut t = aurora_table(&["A", "B"]);
    t.set_width(80);
    t.add_row(vec!["x".to_string(), "y".to_string()]);
    let out = t.to_string();
    assert!(!out.is_empty(), "Never mode produced empty table");
    assert!(
        !out.contains('╭'),
        "no-color mode must not emit UTF-8 round corners, got: {out}"
    );

    COLOR_OVERRIDE.store(prev, std::sync::atomic::Ordering::Relaxed);
}
