use super::*;

fn snap(actions: usize, footer: bool, body: bool, notice: bool) -> HeightSnapshot {
    HeightSnapshot {
        action_row_count: actions,
        footer_visible: footer,
        output_body_visible: body,
        output_notice_visible: notice,
    }
}

#[test]
fn empty_state_is_minimum_height() {
    let h = compute_desired_height(snap(0, false, false, false));
    assert!(
        (h - MIN_WINDOW_HEIGHT).abs() < 0.01,
        "expected min={MIN_WINDOW_HEIGHT}, got {h}"
    );
}

#[test]
fn launch_height_fits_within_one_row_plus_chrome() {
    // Sanity bound — the prompt-only window should be small enough that
    // it visibly reads as "just an input" on a typical 1080p display.
    assert!(MIN_WINDOW_HEIGHT < 120.0);
}

#[test]
fn typing_grows_height_per_action_row() {
    let h0 = compute_desired_height(snap(0, false, false, false));
    let h1 = compute_desired_height(snap(1, false, false, false));
    let h3 = compute_desired_height(snap(3, false, false, false));
    assert!(h1 > h0);
    assert!(h3 > h1);
}

#[test]
fn output_body_grows_more_than_notice() {
    let body = compute_desired_height(snap(0, true, true, false));
    let notice = compute_desired_height(snap(0, true, false, true));
    assert!(body > notice);
}

#[test]
fn body_and_notice_are_mutually_exclusive() {
    // If both flags are set, body wins (a non-empty card is always
    // preferred over the notice fallback).
    let both = compute_desired_height(snap(0, false, true, true));
    let body = compute_desired_height(snap(0, false, true, false));
    assert!((both - body).abs() < 0.01);
}

#[test]
fn height_caps_at_max() {
    // Lots of action rows + footer + output body — must clamp.
    let h = compute_desired_height(snap(50, true, true, false));
    assert!(h <= MAX_WINDOW_HEIGHT + 0.01);
}

#[test]
fn clearing_query_collapses_back_toward_minimum() {
    // User had a few actions visible, then cleared the query.
    let typed = compute_desired_height(snap(3, true, false, false));
    let cleared = compute_desired_height(snap(0, false, false, false));
    assert!(cleared < typed);
    assert!((cleared - MIN_WINDOW_HEIGHT).abs() < 0.01);
}

#[test]
fn output_persists_when_query_cleared() {
    // Per Part 1 hysteresis spec: clearing the query collapses the list
    // but keeps the most recent output card. This test pins the
    // assumption that the compute function honors it as long as the
    // caller passes `output_*_visible=true` even with zero actions.
    let with_output = compute_desired_height(snap(0, false, true, false));
    let bare = compute_desired_height(snap(0, false, false, false));
    assert!(with_output > bare);
}
