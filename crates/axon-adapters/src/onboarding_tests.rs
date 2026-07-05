use crate::{SourceFamily, onboarding_rows, onboarding_status, source_family_matrix};

#[test]
fn onboarding_required_families_have_all_rows_complete() {
    for spec in source_family_matrix() {
        let status = onboarding_status(spec);
        assert!(status.identity.complete, "{:?} identity", spec.family);
        assert!(status.resolver.complete, "{:?} resolver", spec.family);
        assert!(status.router.complete, "{:?} router", spec.family);
        assert!(status.adapter.complete, "{:?} adapter", spec.family);
        assert!(status.scopes.complete, "{:?} scopes", spec.family);
        assert!(status.ledger.complete, "{:?} ledger", spec.family);
        assert!(status.parsing.complete, "{:?} parsing", spec.family);
        assert!(status.graph.complete, "{:?} graph", spec.family);
        assert!(status.chunking.complete, "{:?} chunking", spec.family);
        assert!(status.metadata.complete, "{:?} metadata", spec.family);
        assert!(
            status.auth_secrets.complete,
            "{:?} auth/secrets",
            spec.family
        );
        assert!(
            status.observability.complete,
            "{:?} observability",
            spec.family
        );
        assert!(
            status.error_handling.complete,
            "{:?} error handling",
            spec.family
        );
        assert!(status.tests.complete, "{:?} tests", spec.family);
        assert!(status.docs.complete, "{:?} docs", spec.family);
    }
}

#[test]
fn onboarding_status_has_exact_contract_row_count() {
    for spec in source_family_matrix() {
        let status = onboarding_status(spec);
        assert_eq!(onboarding_rows(&status).len(), 15, "{:?}", spec.family);
    }
}

#[test]
fn onboarding_keeps_memory_as_integration_not_source_adapter() {
    let memory = source_family_matrix()
        .iter()
        .find(|spec| spec.family == SourceFamily::MemoryIntegration)
        .expect("memory integration row");
    let status = onboarding_status(memory);

    assert!(!memory.is_source_adapter);
    assert!(memory.scopes.is_empty());
    assert!(status.adapter.complete);
    assert!(status.ledger.complete);
    assert!(status.docs.complete);
}
