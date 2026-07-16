//! Round-trip + shape tests for the shared prune DTOs (owned in
//! `axon-api::source::prune`, produced/consumed here).

use axon_api::source::ids::{JobId, SourceGenerationId, SourceId};
use axon_api::source::prune::{PruneExecuteRequest, PruneRequest, PruneSelector};
use uuid::Uuid;

#[test]
fn prune_request_defaults_to_dry_run_when_deserialized() {
    // Only `selector` provided; dry_run must default to true (default-safe).
    let json = r#"{ "selector": { "kind": "source", "source_id": "owner/repo" } }"#;
    let req: PruneRequest = serde_json::from_str(json).unwrap();
    assert!(req.dry_run, "dry_run must default to true");
    assert!(!req.require_confirmation);
    assert_eq!(
        req.selector,
        PruneSelector::Source {
            source_id: SourceId::new("owner/repo")
        }
    );
}

#[test]
fn selector_variants_round_trip() {
    let selectors = vec![
        PruneSelector::Source {
            source_id: SourceId::new("s1"),
        },
        PruneSelector::Generation {
            source_id: SourceId::new("s1"),
            generation: SourceGenerationId::new("g1"),
        },
        PruneSelector::Collection {
            collection: "axon".into(),
        },
        PruneSelector::JobRetention {
            older_than_days: 30,
        },
        PruneSelector::Cache { older_than_days: 7 },
    ];
    for sel in selectors {
        let json = serde_json::to_string(&sel).unwrap();
        let back: PruneSelector = serde_json::from_str(&json).unwrap();
        assert_eq!(sel, back, "round-trip mismatch for {json}");
    }
}

#[test]
fn plan_is_reviewable_as_json() {
    // Safety rule: prune plans must be reviewable as JSON.
    use crate::plan::PrunePlanner;
    use crate::testing::{FakeScopeSource, cleanup_debt_estimate};

    let plan = PrunePlanner::new(FakeScopeSource::new(cleanup_debt_estimate())).resolve(
        &PruneSelector::Source {
            source_id: SourceId::new("owner/repo"),
        },
    );
    let json = serde_json::to_value(&plan).unwrap();
    assert!(json.get("steps").is_some());
    assert_eq!(json["destructive"], serde_json::json!(true));
    assert_eq!(json["requires_admin"], serde_json::json!(true));
}

#[test]
fn request_helpers_set_expected_flags() {
    let sel = PruneSelector::Cache { older_than_days: 1 };
    let dry = PruneRequest::dry_run(sel.clone(), "housekeeping");
    assert!(dry.dry_run);
    let exec = PruneRequest::execute(sel, "operator requested");
    assert!(!exec.dry_run);
    assert!(exec.require_confirmation);
}

#[test]
fn execute_request_references_reviewed_plan_id_instead_of_inline_plan() {
    let plan_id = JobId::new(Uuid::new_v4());
    let request = PruneExecuteRequest {
        plan_id,
        confirm: true,
        reason: "reviewed".to_string(),
    };
    let json = serde_json::to_value(request).expect("serialize execute request");
    assert_eq!(json["plan_id"], serde_json::json!(plan_id.0));
    assert!(json.get("plan").is_none());
}
