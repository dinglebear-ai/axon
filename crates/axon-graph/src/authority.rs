//! Authority ranking and conflict resolution for the SourceGraph.
//!
//! Authority levels are defined in
//! `docs/pipeline-unification/sources/source-graph.md` ("Authority Levels").
//! This module provides a *ranked* view of authority used by the merge logic to
//! decide which competing claim wins under the
//! `keep_highest_authority_with_evidence` conflict policy (graph-schema.md).

use axon_api::source::AuthorityLevel;

/// A ranked authority level.
///
/// Ordered from lowest to highest trust. `Conflicting` is intentionally *not*
/// part of this ranking — it is an outcome, not an input claim, and is applied
/// to a node/edge only when competing high-authority evidence disagrees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Authority {
    /// Relation exists but authority is not established.
    Unknown,
    /// Community/third-party but useful.
    Community,
    /// Mirror/cache/copy of another source.
    Mirror,
    /// Derived from metadata, redirects, sitemap, llms.txt, or content evidence.
    Inferred,
    /// Verified through a trusted signal (between inferred and official).
    Verified,
    /// Claimed by package/repo/site owner or user-pinned as official.
    Official,
    /// User explicitly declared the relation. Wins for routing.
    UserPinned,
}

impl Authority {
    /// Map to the transport-neutral [`AuthorityLevel`] DTO.
    pub const fn to_level(self) -> AuthorityLevel {
        match self {
            Self::Unknown => AuthorityLevel::Unknown,
            Self::Community => AuthorityLevel::Community,
            Self::Mirror => AuthorityLevel::Mirror,
            Self::Inferred => AuthorityLevel::Inferred,
            Self::Verified => AuthorityLevel::Verified,
            Self::Official => AuthorityLevel::Official,
            Self::UserPinned => AuthorityLevel::UserPinned,
        }
    }

    /// Map from the transport-neutral [`AuthorityLevel`] DTO.
    ///
    /// `Conflicting` collapses to `Unknown` for ranking purposes because a
    /// conflicting claim carries no authority of its own.
    pub const fn from_level(level: AuthorityLevel) -> Self {
        match level {
            AuthorityLevel::Unknown | AuthorityLevel::Conflicting => Self::Unknown,
            AuthorityLevel::Community => Self::Community,
            AuthorityLevel::Mirror => Self::Mirror,
            AuthorityLevel::Inferred => Self::Inferred,
            AuthorityLevel::Verified => Self::Verified,
            AuthorityLevel::Official => Self::Official,
            AuthorityLevel::UserPinned => Self::UserPinned,
        }
    }

    /// The rank of this authority, higher meaning more trusted.
    pub const fn rank(self) -> u8 {
        match self {
            Self::Unknown => 0,
            Self::Community => 1,
            Self::Mirror => 2,
            Self::Inferred => 3,
            Self::Verified => 4,
            Self::Official => 5,
            Self::UserPinned => 6,
        }
    }

    /// Whether this authority is high enough to establish an authoritative edge.
    ///
    /// Per source-graph.md conflict rules: "Low-confidence text mentions should
    /// not create authoritative edges." Only `Verified` and above are
    /// authoritative.
    pub const fn is_authoritative(self) -> bool {
        self.rank() >= Self::Verified.rank()
    }
}

/// Resolve the winning authority between an existing claim and a new claim.
///
/// Returns [`AuthorityDecision`] describing the winner and whether the two
/// claims *conflict* — i.e. both are authoritative but disagree. A conflict does
/// not overwrite; the caller records it as an explicit graph conflict
/// ("Preserve conflicting evidence; do not overwrite it with the newest
/// claim.").
pub fn resolve_authority(existing: Authority, incoming: Authority) -> AuthorityDecision {
    if incoming.rank() > existing.rank() {
        AuthorityDecision {
            winner: incoming,
            conflict: false,
        }
    } else if incoming.rank() < existing.rank() {
        AuthorityDecision {
            winner: existing,
            conflict: false,
        }
    } else {
        // Equal authority. Keep the existing claim (idempotent, deterministic),
        // but flag a conflict when both sides are authoritative so the divergent
        // evidence is preserved rather than silently merged.
        AuthorityDecision {
            winner: existing,
            conflict: existing.is_authoritative(),
        }
    }
}

/// The outcome of [`resolve_authority`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthorityDecision {
    /// The authority that wins for the merged node/edge.
    pub winner: Authority,
    /// Whether the two claims conflict (both authoritative, equal rank).
    pub conflict: bool,
}

#[cfg(test)]
#[path = "authority_tests.rs"]
mod tests;
