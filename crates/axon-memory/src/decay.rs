//! Memory scoring, decay, reinforcement, and status/contradiction penalties.
//!
//! Implements the score formula from
//! `docs/pipeline-unification/runtime/memory-contract.md` ("Score formula"):
//!
//! ```text
//! base_score =
//!   0.45 * semantic_score +
//!   0.20 * confidence +
//!   0.15 * salience +
//!   0.10 * scope_match +
//!   0.10 * reinforcement_score
//!
//! decay_multiplier =
//!   1.0                                  when profile = none or pinned
//!   0.5 ^ (age_days / half_life_days)    otherwise
//!
//! memory_score =
//!   clamp01(base_score * decay_multiplier - contradiction_penalty - status_penalty)
//! ```

use axon_api::source::{DecayProfile, MemoryRecord, MemoryStatus, MemoryType};

/// Contract default contradiction penalty for unresolved contradictions.
pub const CONTRADICTION_PENALTY: f32 = 0.25;

/// All inputs to a memory score, normalized to `0.0..=1.0` where noted.
#[derive(Debug, Clone, Copy)]
pub struct ScoreInputs {
    /// Vector similarity between query and memory body (0..=1). `0.0` for
    /// non-semantic recall paths (e.g. keyword-only search).
    pub semantic_score: f32,
    pub confidence: f32,
    pub salience: f32,
    /// Relevance of memory scope to caller context (0..=1).
    pub scope_match: f32,
    /// `ln(1 + reinforcement_count) / 5.0`, clamped to 1.0.
    pub reinforcement_score: f32,
    pub decay_multiplier: f32,
    pub contradiction_penalty: f32,
    pub status_penalty: f32,
}

/// Clamp a value into the `0.0..=1.0` range.
pub fn clamp01(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

/// `reinforcement_score = min(1.0, ln(1 + reinforcement_count) / 5.0)`.
pub fn reinforcement_score(reinforcement_count: u32) -> f32 {
    let raw = (1.0 + reinforcement_count as f32).ln() / 5.0;
    clamp01(raw)
}

/// `status_penalty` per the contract: `1.0` forgotten, `0.5` superseded,
/// `0.25` archived unless included, `0` otherwise (active/review/contradicted/
/// working). Contradiction is applied as a separate penalty.
pub fn status_penalty(status: MemoryStatus, include_archived: bool) -> f32 {
    match status {
        MemoryStatus::Forgotten => 1.0,
        MemoryStatus::Superseded => 0.5,
        MemoryStatus::Archived if !include_archived => 0.25,
        _ => 0.0,
    }
}

/// `decay_multiplier = 0.5 ^ (age_days / half_life_days)`, or `1.0` when the
/// profile is `none` or the memory is pinned.
pub fn decay_multiplier(profile: DecayProfile, pinned: bool, age_days: f64) -> f32 {
    if pinned {
        return 1.0;
    }
    match profile.half_life_days() {
        None => 1.0,
        Some(half_life) if half_life > 0.0 => {
            let exponent = age_days.max(0.0) / half_life;
            0.5f64.powf(exponent) as f32
        }
        Some(_) => 1.0,
    }
}

/// `base_score` weighted blend from the contract.
pub fn base_score(inputs: &ScoreInputs) -> f32 {
    0.45 * inputs.semantic_score
        + 0.20 * inputs.confidence
        + 0.15 * inputs.salience
        + 0.10 * inputs.scope_match
        + 0.10 * inputs.reinforcement_score
}

/// Full `memory_score` from all inputs.
pub fn memory_score(inputs: &ScoreInputs) -> f32 {
    let base = base_score(inputs);
    let scored =
        base * inputs.decay_multiplier - inputs.contradiction_penalty - inputs.status_penalty;
    clamp01(scored)
}

/// Resolve the effective decay profile for a record: an explicit decay policy
/// profile string wins; otherwise the memory type's default profile applies.
pub fn resolve_profile(memory_type: MemoryType, profile_str: Option<&str>) -> DecayProfile {
    match profile_str {
        Some("very_fast") => DecayProfile::VeryFast,
        Some("fast") => DecayProfile::Fast,
        Some("normal") => DecayProfile::Normal,
        Some("slow") => DecayProfile::Slow,
        Some("very_slow") => DecayProfile::VerySlow,
        Some("none") => DecayProfile::None,
        _ => memory_type.default_decay_profile(),
    }
}

/// Compute a record's live `memory_score` given the age (in days) since the
/// most recent of `last_reinforced_at`/`updated_at`/`created_at`, a semantic
/// score, a scope-match score, and whether archived memories are included.
///
/// Contradiction penalty applies when the record status is `Contradicted`.
pub fn score_record(
    record: &MemoryRecord,
    age_days: f64,
    semantic_score: f32,
    scope_match: f32,
    include_archived: bool,
) -> f32 {
    let profile_str = record.decay.as_ref().map(|d| d.profile.as_str());
    let profile = resolve_profile(record.memory_type, profile_str);
    let pinned = record.decay.as_ref().map(|d| d.pinned).unwrap_or(false);
    let count = record
        .decay
        .as_ref()
        .map(|d| d.reinforcement_count)
        .unwrap_or(0);
    let contradiction_penalty = if record.status == MemoryStatus::Contradicted {
        CONTRADICTION_PENALTY
    } else {
        0.0
    };
    let inputs = ScoreInputs {
        semantic_score: clamp01(semantic_score),
        confidence: clamp01(record.confidence),
        salience: clamp01(record.salience),
        scope_match: clamp01(scope_match),
        reinforcement_score: reinforcement_score(count),
        decay_multiplier: decay_multiplier(profile, pinned, age_days),
        contradiction_penalty,
        status_penalty: status_penalty(record.status, include_archived),
    };
    memory_score(&inputs)
}

#[cfg(test)]
#[path = "decay_tests.rs"]
mod tests;
