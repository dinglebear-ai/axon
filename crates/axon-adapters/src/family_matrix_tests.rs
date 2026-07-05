use std::collections::BTreeSet;

use axon_api::source::SourceKind;

use crate::{SourceFamily, source_family_matrix};

#[test]
fn family_matrix_contains_required_source_families() {
    let families = source_family_matrix()
        .iter()
        .map(|spec| spec.family)
        .collect::<BTreeSet<_>>();

    for expected in [
        SourceFamily::Local,
        SourceFamily::Git,
        SourceFamily::Web,
        SourceFamily::Feed,
        SourceFamily::Youtube,
        SourceFamily::Reddit,
        SourceFamily::Sessions,
        SourceFamily::Registry,
        SourceFamily::CliTool,
        SourceFamily::McpTool,
        SourceFamily::MemoryIntegration,
    ] {
        assert!(families.contains(&expected), "missing {expected:?}");
    }
}

#[test]
fn family_matrix_rows_have_contract_basics() {
    let mut adapters = BTreeSet::new();
    for spec in source_family_matrix() {
        assert!(
            !spec.adapter.is_empty(),
            "missing adapter for {:?}",
            spec.family
        );
        assert!(
            !spec.version.is_empty(),
            "missing version for {:?}",
            spec.family
        );
        assert!(
            !spec.metadata_families.is_empty(),
            "missing metadata families for {:?}",
            spec.family
        );
        assert!(
            !spec.degraded_modes.is_empty(),
            "missing degraded modes for {:?}",
            spec.family
        );
        assert!(
            !spec.required_graph_fact_kinds.is_empty(),
            "missing required graph facts for {:?}",
            spec.family
        );
        assert!(
            adapters.insert(spec.adapter),
            "duplicate adapter/integration row {}",
            spec.adapter
        );
    }
}

#[test]
fn family_matrix_public_resolver_choices_exclude_memory_integration() {
    let public_families = source_family_matrix()
        .iter()
        .filter(|spec| spec.public_resolver_family())
        .map(|spec| spec.family)
        .collect::<BTreeSet<_>>();

    assert!(!public_families.contains(&SourceFamily::MemoryIntegration));

    let memory = source_family_matrix()
        .iter()
        .find(|spec| spec.family == SourceFamily::MemoryIntegration)
        .expect("memory integration row");
    assert!(!memory.is_source_adapter);
    assert!(memory.supported_schemes.is_empty());
    assert!(memory.shorthand_patterns.is_empty());
    assert_eq!(memory.source_kinds, &[SourceKind::Memory]);
}

#[test]
fn family_matrix_security_sensitive_rows_declare_capability_flags() {
    let by_family = |family| {
        source_family_matrix()
            .iter()
            .find(|spec| spec.family == family)
            .unwrap()
    };

    assert!(by_family(SourceFamily::Local).may_access_local_paths);
    assert!(by_family(SourceFamily::Web).may_perform_network_fetches);
    assert!(by_family(SourceFamily::Web).may_call_render_provider);
    assert!(by_family(SourceFamily::CliTool).may_execute_tools);
    assert!(by_family(SourceFamily::McpTool).may_execute_tools);
}
