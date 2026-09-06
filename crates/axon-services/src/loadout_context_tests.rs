use super::*;

fn capability(member_id: &str) -> CapabilityRef {
    CapabilityRef {
        provider: "labby:test".into(),
        family: "skill".into(),
        member_id: member_id.into(),
        expected_revision: "r1".into(),
    }
}

#[test]
fn context_is_deterministic_and_sorted() {
    let first = build_context(&[capability("z"), capability("a")]).unwrap();
    let second = build_context(&[capability("a"), capability("z")]).unwrap();
    assert_eq!(first, second);
    assert!(first.find(":a@").unwrap() < first.find(":z@").unwrap());
    assert!(first.contains("trust=\"untrusted_metadata_only\""));
}

#[test]
fn binding_limits_fail_closed() {
    let binding = LoadoutBinding {
        integration_id: "x".repeat(257),
        loadout_id: "loadout".into(),
        expected_revision: 1,
        conversation_binding: None,
    };
    assert!(
        validate_binding(&binding)
            .unwrap_err()
            .to_string()
            .contains("integration_id")
    );
}

#[test]
fn execution_context_id_is_revision_and_generation_bound() {
    let preview = Preview {
        loadout_id: "loadout".into(),
        active_revision: 2,
        catalog_generation: "catalog-a".into(),
        runtime_identity: "axon".into(),
        effective: vec![],
        missing: vec![],
        conflicts: vec![],
    };
    assert_ne!(
        context_id("labby", &preview, 1),
        context_id("labby", &preview, 2)
    );
}
