//! Shared animation primitives for the palette UI.
//!
//! All animations follow the discipline established in PR #99's hotfix
//! `56919c11`: never wrap `Animation::repeat()` around anything that renders
//! before user input is accepted. The helpers here are designed for
//! *one-shot* transitions driven by stored `Instant`s — the render pass
//! computes the current frame's progress and the animation self-terminates
//! when `progress >= 1.0`.
//!
//! The window's resize tick (in `ui.rs`) self-terminates when
//! `current_height ≈ target_height`.

use std::time::Duration;

/// Master toggle for accessibility / reduce-motion. When `true`, all easing
/// collapses to instant — the state machine still advances, the visual
/// transition is skipped. Compile-time constant for now (no user-facing
/// setting); flip locally to test.
pub(crate) const REDUCE_MOTION: bool = false;

/// Resize tick interval for the window-height lerp. ~60 FPS is overkill for
/// a window resize; 16ms keeps the motion smooth without flooding the GPUI
/// runloop with notifications.
pub(crate) const RESIZE_TICK_MS: u64 = 16;

/// How close `current_height` has to get to `target_height` before the
/// resize tick self-terminates and snaps to the target. Under one pixel is
/// indistinguishable from "done" on any DPI.
pub(crate) const RESIZE_SNAP_EPSILON: f32 = 0.75;

/// Linear interpolation between `from` and `to` at progress `t` (clamped to
/// `[0.0, 1.0]`).
pub(crate) fn lerp_f32(from: f32, to: f32, t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    from + (to - from) * t
}

/// Cubic ease-out. `t` in `[0.0, 1.0]`, returns `[0.0, 1.0]`.
/// Standard easing curve: fast start, gentle settle.
pub(crate) fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let inv = 1.0 - t;
    1.0 - inv * inv * inv
}

/// Compute one-shot animation progress in `[0.0, 1.0]` for an animation
/// that started `elapsed` ago and runs for `duration`.
///
/// Returns `1.0` immediately when `REDUCE_MOTION` is on or when `duration`
/// is zero — the state machine completes, the transition is skipped.
pub(crate) fn one_shot_progress(elapsed: Duration, duration: Duration) -> f32 {
    if REDUCE_MOTION {
        return 1.0;
    }
    if duration.is_zero() {
        return 1.0;
    }
    let elapsed_ms = elapsed.as_secs_f32() * 1000.0;
    let total_ms = duration.as_secs_f32() * 1000.0;
    (elapsed_ms / total_ms).clamp(0.0, 1.0)
}

/// Step a current value toward a target by the given delta. Returns the
/// new value. If the delta would overshoot, returns the target exactly.
///
/// Used by the window-resize lerp where we want fixed-step movement, not
/// time-based easing (the tick frequency is the control variable).
pub(crate) fn step_toward(current: f32, target: f32, step: f32) -> f32 {
    let step = step.abs();
    if step == 0.0 {
        // Zero step means "no movement this tick" — keep the current value.
        // Returning `target` would cause an unintended instant jump.
        return current;
    }
    if (target - current).abs() <= step {
        return target;
    }
    if target > current {
        current + step
    } else {
        current - step
    }
}

#[cfg(test)]
#[path = "anim_tests.rs"]
mod tests;
