//! Dependency-free civil-calendar <-> days-since-epoch conversion, shared by
//! `github_bridge.rs` (rate-limit "retry at" timestamp formatting) and
//! `github_feed/normalize.rs` (parsing GitHub's `created_at` ISO 8601
//! timestamps into Unix seconds). Both call sites previously hand-rolled
//! their own copy of this Howard Hinnant civil-calendar algorithm pair;
//! consolidated here so a correctness fix in one direction can't drift from
//! the other, since they are exact inverses of each other.
//!
//! Avoids pulling in a `chrono` dependency just for two small conversions.

/// Howard Hinnant's `civil_from_days` algorithm — days-since-epoch to
/// proleptic Gregorian (year, month, day).
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Inverse of `civil_from_days` — proleptic Gregorian (year, month, day) to
/// days-since-epoch.
pub(crate) fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
#[path = "date_math_tests.rs"]
mod tests;
