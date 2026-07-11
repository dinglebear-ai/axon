//! Visibility ceiling derivation.
//!
//! Per the auth contract's "Visibility" table, auth controls how much state a
//! caller can see. `VisibilityPolicy` derives the ceiling a caller is allowed
//! to see from their `CallerContext`, independent of any per-field visibility
//! already stamped on data (`axon_api::source::Visibility` on the field side).

use axon_api::source::{CallerContext, Visibility};

use crate::{AXON_ADMIN_SCOPE, scope_satisfies};

/// Derives visibility ceilings from caller identity.
///
/// Rule (auth contract): a trusted-local or admin-scoped caller may see up to
/// `Internal` (local paths, provider internals); every other caller is
/// capped at `Public` (safe metadata and redacted text). `Sensitive` is never
/// granted as a ceiling here — sensitive fields require admin-only checks at
/// the point of use, not a blanket ceiling, and `Redacted` is always visible
/// (it is the placeholder itself).
#[derive(Debug, Clone, Copy, Default)]
pub struct VisibilityPolicy;

impl VisibilityPolicy {
    pub fn new() -> Self {
        Self
    }

    /// Compute the visibility ceiling for `caller`.
    pub fn ceiling_for(&self, caller: &CallerContext) -> Visibility {
        if caller.trusted_local || scope_satisfies(&caller.scopes, AXON_ADMIN_SCOPE) {
            Visibility::Internal
        } else {
            Visibility::Public
        }
    }

    /// Whether a field/value stamped at `field_visibility` is visible to a
    /// caller whose ceiling is `ceiling`. `Redacted` is always visible (it is
    /// itself the placeholder, not the underlying secret); everything else is
    /// visible only at or below the caller's ceiling, ranked
    /// `Public < Internal < Sensitive`. `Derived` values inherit the ceiling
    /// rule of `Internal` — they are computed from internal state and are not
    /// automatically public.
    pub fn is_visible(&self, field_visibility: Visibility, ceiling: Visibility) -> bool {
        if field_visibility == Visibility::Redacted {
            return true;
        }
        Self::rank(Self::normalize(field_visibility)) <= Self::rank(ceiling)
    }

    fn normalize(visibility: Visibility) -> Visibility {
        match visibility {
            Visibility::Derived => Visibility::Internal,
            other => other,
        }
    }

    fn rank(visibility: Visibility) -> u8 {
        match visibility {
            Visibility::Public => 0,
            Visibility::Internal | Visibility::Derived => 1,
            Visibility::Sensitive => 2,
            Visibility::Redacted => 3,
        }
    }
}

/// Convenience free function equivalent to `VisibilityPolicy::default().ceiling_for(caller)`.
pub fn ceiling_for(caller: &CallerContext) -> Visibility {
    VisibilityPolicy::new().ceiling_for(caller)
}

#[cfg(test)]
#[path = "visibility_tests.rs"]
mod tests;
