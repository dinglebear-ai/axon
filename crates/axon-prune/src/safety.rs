//! Safety checks and destructive-request gating for prune.
//!
//! Encodes the pruning-contract "Safety Rules":
//! - default prune is dry-run unless explicitly executing
//! - destructive prune requires `axon:admin`
//! - source/generation vector deletes are generation-fenced
//! - artifact deletes are artifact-id based, never arbitrary path based
//!
//! See `docs/pipeline-unification/runtime/pruning-contract.md`.

use axon_api::source::ids::SourceGenerationId;
use axon_api::source::prune::PruneSelector;

/// Why a prune was refused before any mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PruneDenied {
    /// Destructive execution attempted without the `axon:admin` scope.
    AdminRequired,
    /// A generation-fenced delete tried to remove the current (committed)
    /// generation. The contract forbids this "by accident" case.
    CurrentGenerationFenced { generation: SourceGenerationId },
    /// A destructive request lacked explicit confirmation.
    ConfirmationRequired,
    /// The selector names a boundary this build cannot execute a delete
    /// against yet (only `Source`/`Generation` vector prunes are wired).
    /// Refused rather than silently reporting a no-op "success".
    Unsupported { selector: String, guidance: String },
}

impl core::fmt::Display for PruneDenied {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PruneDenied::AdminRequired => {
                write!(f, "destructive prune requires the axon:admin scope")
            }
            PruneDenied::CurrentGenerationFenced { generation } => write!(
                f,
                "refusing to delete current generation {} (generation-fenced)",
                generation.0
            ),
            PruneDenied::ConfirmationRequired => {
                write!(f, "destructive prune requires explicit confirmation")
            }
            PruneDenied::Unsupported { selector, guidance } => {
                write!(
                    f,
                    "prune selector {selector} is not supported yet: {guidance}"
                )
            }
        }
    }
}

impl std::error::Error for PruneDenied {}

/// The authorization context a prune executes under.
#[derive(Debug, Clone, Default)]
pub struct PruneAuthz {
    pub is_admin: bool,
}

impl PruneAuthz {
    pub fn admin() -> Self {
        Self { is_admin: true }
    }

    pub fn anonymous() -> Self {
        Self { is_admin: false }
    }
}

/// Whether a selector is destructive (mutates persistent state on execute).
///
/// Every current selector is destructive when executed; this exists so future
/// non-destructive selectors (pure reporting) can be modeled without changing
/// call sites.
pub fn selector_is_destructive(_selector: &PruneSelector) -> bool {
    true
}

/// Whether a selector requires the `axon:admin` scope to execute.
///
/// Contract: destructive prune requires admin. Collection-wide and generation
/// deletes are always admin-gated; the rest inherit destructiveness.
pub fn selector_requires_admin(selector: &PruneSelector) -> bool {
    selector_is_destructive(selector)
}

/// Gate a destructive execution against the caller's authorization and
/// confirmation. Returns `Ok(())` when the prune may proceed.
///
/// Dry-run requests bypass all gating (they never mutate).
pub fn authorize_execution(
    selector: &PruneSelector,
    dry_run: bool,
    require_confirmation: bool,
    confirmed: bool,
    authz: &PruneAuthz,
) -> Result<(), PruneDenied> {
    if dry_run {
        return Ok(());
    }
    if selector_requires_admin(selector) && !authz.is_admin {
        return Err(PruneDenied::AdminRequired);
    }
    if require_confirmation && !confirmed {
        return Err(PruneDenied::ConfirmationRequired);
    }
    Ok(())
}

/// Generation-fence a delete: refuse to delete the *current* committed
/// generation. Only the fenced (non-current) generation may be pruned.
///
/// Contract safety rule: "source/generation vector deletes are
/// generation-fenced" and the test requirement "generation-fenced deletes
/// cannot delete current generation by accident".
pub fn fence_generation(
    target: &SourceGenerationId,
    current: &SourceGenerationId,
) -> Result<(), PruneDenied> {
    if target == current {
        return Err(PruneDenied::CurrentGenerationFenced {
            generation: target.clone(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "safety_tests.rs"]
mod tests;
