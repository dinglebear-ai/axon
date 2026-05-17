use super::*;

#[test]
fn lerp_endpoints() {
    assert_eq!(lerp_f32(10.0, 20.0, 0.0), 10.0);
    assert_eq!(lerp_f32(10.0, 20.0, 1.0), 20.0);
    assert_eq!(lerp_f32(10.0, 20.0, 0.5), 15.0);
}

#[test]
fn lerp_clamps_out_of_range_t() {
    assert_eq!(lerp_f32(0.0, 100.0, -1.0), 0.0);
    assert_eq!(lerp_f32(0.0, 100.0, 2.0), 100.0);
}

#[test]
fn ease_out_cubic_is_monotonic_and_bounded() {
    assert_eq!(ease_out_cubic(0.0), 0.0);
    assert!((ease_out_cubic(1.0) - 1.0).abs() < 1e-6);
    let mut prev = 0.0_f32;
    for i in 1..=10 {
        let cur = ease_out_cubic(i as f32 / 10.0);
        assert!(cur >= prev, "ease_out_cubic must be monotonic");
        assert!((0.0..=1.0).contains(&cur));
        prev = cur;
    }
}

#[test]
fn one_shot_progress_zero_duration_completes_immediately() {
    assert_eq!(
        one_shot_progress(Duration::from_millis(0), Duration::from_millis(0)),
        1.0
    );
}

#[test]
fn one_shot_progress_midpoint() {
    let p = one_shot_progress(Duration::from_millis(100), Duration::from_millis(200));
    assert!((p - 0.5).abs() < 1e-6, "expected 0.5, got {p}");
}

#[test]
fn one_shot_progress_caps_at_one() {
    let p = one_shot_progress(Duration::from_secs(10), Duration::from_millis(200));
    assert_eq!(p, 1.0);
}

#[test]
fn step_toward_increasing() {
    assert_eq!(step_toward(0.0, 10.0, 3.0), 3.0);
    assert_eq!(step_toward(3.0, 10.0, 3.0), 6.0);
}

#[test]
fn step_toward_decreasing() {
    assert_eq!(step_toward(10.0, 0.0, 3.0), 7.0);
}

#[test]
fn step_toward_does_not_overshoot() {
    assert_eq!(step_toward(9.0, 10.0, 5.0), 10.0);
    assert_eq!(step_toward(1.0, 0.0, 5.0), 0.0);
}

#[test]
fn step_toward_already_there_is_idempotent() {
    assert_eq!(step_toward(5.0, 5.0, 3.0), 5.0);
}
