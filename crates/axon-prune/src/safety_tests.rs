use super::*;
use axon_api::source::ids::{SourceGenerationId, SourceId};
use axon_api::source::prune::PruneSelector;

fn source_sel() -> PruneSelector {
    PruneSelector::Source {
        source_id: SourceId::new("owner/repo"),
    }
}

#[test]
fn dry_run_bypasses_all_gating_even_for_anonymous() {
    let sel = source_sel();
    // dry_run=true, no admin, confirmation required but not confirmed: still OK.
    let out = authorize_execution(&sel, true, true, false, &PruneAuthz::anonymous());
    assert!(out.is_ok());
}

#[test]
fn destructive_execution_requires_admin() {
    let sel = source_sel();
    let out = authorize_execution(&sel, false, false, false, &PruneAuthz::anonymous());
    assert_eq!(out, Err(PruneDenied::AdminRequired));
}

#[test]
fn admin_may_execute_when_confirmed() {
    let sel = source_sel();
    let out = authorize_execution(&sel, false, true, true, &PruneAuthz::admin());
    assert!(out.is_ok());
}

#[test]
fn admin_still_needs_confirmation_when_required() {
    let sel = source_sel();
    let out = authorize_execution(&sel, false, true, false, &PruneAuthz::admin());
    assert_eq!(out, Err(PruneDenied::ConfirmationRequired));
}

#[test]
fn fence_refuses_current_generation() {
    let cur = SourceGenerationId::new("gen-current");
    let out = fence_generation(&cur, &cur);
    assert_eq!(
        out,
        Err(PruneDenied::CurrentGenerationFenced {
            generation: cur.clone()
        })
    );
}

#[test]
fn fence_allows_non_current_generation() {
    let cur = SourceGenerationId::new("gen-current");
    let old = SourceGenerationId::new("gen-old");
    assert!(fence_generation(&old, &cur).is_ok());
}

#[test]
fn every_selector_is_admin_gated() {
    let selectors = [
        source_sel(),
        PruneSelector::Collection {
            collection: "axon".into(),
        },
        PruneSelector::JobRetention {
            older_than_days: 30,
        },
        PruneSelector::Cache { older_than_days: 7 },
    ];
    for sel in selectors {
        assert!(selector_requires_admin(&sel), "{sel:?} must be admin-gated");
    }
}
