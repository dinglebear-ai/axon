//! Time handling and age computation for memory scoring.
//!
//! `Timestamp` is a plain RFC3339 string newtype in `axon-api`, so this module
//! provides a minimal, dependency-free parser sufficient to compute the
//! `age_days` input the decay contract requires (measured from
//! `last_reinforced_at` when present, otherwise `updated_at`, otherwise
//! `created_at`).

use axon_api::source::{MemoryRecord, Timestamp};

/// A monotonic-ish wall clock injected into the store so tests can pin "now".
pub trait Clock: Send + Sync {
    /// Current time as an RFC3339 UTC string.
    fn now_rfc3339(&self) -> String;
    /// Current time as whole seconds since the Unix epoch.
    fn now_epoch_secs(&self) -> i64;
}

/// Parse an RFC3339 / ISO-8601 UTC timestamp into seconds since the Unix epoch.
///
/// Accepts `YYYY-MM-DDTHH:MM:SS[.fraction][Z|+HH:MM|-HH:MM]`. Fractional
/// seconds are truncated; offsets are applied to normalize to UTC. Returns
/// `None` on malformed input.
pub fn parse_epoch_secs(ts: &str) -> Option<i64> {
    let bytes = ts.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = ts.get(0..4)?.parse().ok()?;
    let month: i64 = ts.get(5..7)?.parse().ok()?;
    let day: i64 = ts.get(8..10)?.parse().ok()?;
    let hour: i64 = ts.get(11..13)?.parse().ok()?;
    let minute: i64 = ts.get(14..16)?.parse().ok()?;
    let second: i64 = ts.get(17..19)?.parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    // Timezone offset: look past optional fractional seconds.
    let mut offset_secs: i64 = 0;
    let rest = &ts[19..];
    let tz_part = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if !tz_part.is_empty() && tz_part != "Z" {
        let sign = match tz_part.as_bytes()[0] {
            b'+' => 1,
            b'-' => -1,
            _ => return None,
        };
        let oh: i64 = tz_part.get(1..3)?.parse().ok()?;
        let om: i64 = tz_part.get(4..6)?.parse().ok()?;
        offset_secs = sign * (oh * 3600 + om * 60);
    }

    let days = days_from_civil(year, month, day);
    let utc = days * 86_400 + hour * 3600 + minute * 60 + second - offset_secs;
    Some(utc)
}

/// Format epoch seconds as an RFC3339 UTC string (`...Z`, whole seconds).
pub fn format_rfc3339(epoch_secs: i64) -> String {
    let days = epoch_secs.div_euclid(86_400);
    let secs_of_day = epoch_secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Days since 1970-01-01 for a civil (proleptic Gregorian) date.
///
/// Howard Hinnant's `days_from_civil` algorithm.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    era * 146_097 + doe - 719_468
}

/// Civil (year, month, day) from days since 1970-01-01.
///
/// Inverse of [`days_from_civil`] (Howard Hinnant's `civil_from_days`).
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// A real wall clock backed by `SystemTime`.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_epoch_secs(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    fn now_rfc3339(&self) -> String {
        format_rfc3339(self.now_epoch_secs())
    }
}

/// The reference timestamp used for age: `last_reinforced_at` → `updated_at`
/// (last history event) → `created_at` (first history event).
pub fn age_reference(record: &MemoryRecord) -> Option<String> {
    if let Some(decay) = &record.decay
        && let Some(Timestamp(ts)) = &decay.last_reinforced_at
    {
        return Some(ts.clone());
    }
    if let Some(last) = record.history.last() {
        return Some(last.timestamp.0.clone());
    }
    record
        .history
        .first()
        .map(|event| event.timestamp.0.clone())
}

/// Age in days between the record's reference timestamp and `now_epoch_secs`.
/// Returns `0.0` when the reference is missing or unparseable.
pub fn age_days(record: &MemoryRecord, now_epoch_secs: i64) -> f64 {
    let Some(reference) = age_reference(record) else {
        return 0.0;
    };
    let Some(ref_secs) = parse_epoch_secs(&reference) else {
        return 0.0;
    };
    let delta = (now_epoch_secs - ref_secs).max(0) as f64;
    delta / 86_400.0
}

#[cfg(test)]
#[path = "record_tests.rs"]
mod tests;
