use super::*;

fn snap(actions: usize, footer: bool, body: bool) -> HeightSnapshot {
    HeightSnapshot {
        action_row_count: actions,
        empty_placeholder_visible: false,
        footer_visible: footer,
        output_body_visible: body,
    }
}

#[test]
fn empty_state_is_minimum_height() {
    let h = compute_desired_height(snap(0, false, false));
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
    let h0 = compute_desired_height(snap(0, false, false));
    let h1 = compute_desired_height(snap(1, false, false));
    let h3 = compute_desired_height(snap(3, false, false));
    assert!(h1 > h0);
    assert!(h3 > h1);
}

#[test]
fn empty_placeholder_reserves_one_row() {
    // Zero matches with the list visible still renders the "No matching
    // commands" placeholder — height must reserve a row so it isn't clipped.
    let bare = compute_desired_height(snap(0, false, false));
    let with_placeholder = compute_desired_height(HeightSnapshot {
        action_row_count: 0,
        empty_placeholder_visible: true,
        footer_visible: false,
        output_body_visible: false,
    });
    assert!(with_placeholder > bare);
    // Should be roughly one row's worth larger.
    let one_row = compute_desired_height(snap(1, false, false));
    assert!((with_placeholder - one_row).abs() < 0.01);
}

#[test]
fn height_caps_at_max() {
    // Lots of action rows + footer + output body — must clamp.
    let h = compute_desired_height(snap(50, true, true));
    assert!(h <= MAX_WINDOW_HEIGHT + 0.01);
}

#[test]
fn clearing_query_collapses_back_toward_minimum() {
    // User had a few actions visible, then cleared the query.
    let typed = compute_desired_height(snap(3, true, false));
    let cleared = compute_desired_height(snap(0, false, false));
    assert!(cleared < typed);
    assert!((cleared - MIN_WINDOW_HEIGHT).abs() < 0.01);
}

#[test]
fn output_persists_when_query_cleared() {
    // Per Part 1 hysteresis spec: clearing the query collapses the list
    // but keeps the most recent output card. This test pins the
    // assumption that the compute function honors it as long as the
    // caller passes `output_body_visible=true` even with zero actions.
    let with_output = compute_desired_height(snap(0, false, true));
    let bare = compute_desired_height(snap(0, false, false));
    assert!(with_output > bare);
}
