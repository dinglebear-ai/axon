use super::*;

#[test]
fn config_postcondition_rejects_unchanged_and_wrong_state() {
    let request = json!({"keyPath": "model.default", "value": "gpt-5"});
    let before = json!({"persisted": {"model": {"default": "gpt-4"}}});

    assert_eq!(
        verify_intended_effect(
            &ControlAction::ConfigValueWrite,
            &request,
            Some(&before),
            &before,
            Some("r1"),
            Some("r1"),
        ),
        EffectProof::Absent("canonical state is unchanged".to_string())
    );

    let wrong = json!({"persisted": {"model": {"default": "gpt-4.1"}}});
    assert!(matches!(
        verify_intended_effect(
            &ControlAction::ConfigValueWrite,
            &request,
            Some(&before),
            &wrong,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Absent(_)
    ));
}

#[test]
fn config_postcondition_accepts_only_the_approved_value() {
    let request = json!({"keyPath": ["model", "default"], "value": "gpt-5"});
    let before = json!({"persisted": {"model": {"default": "gpt-4"}}});
    let after = json!({"persisted": {"model": {"default": "gpt-5"}}});
    assert_eq!(
        verify_intended_effect(
            &ControlAction::ConfigValueWrite,
            &request,
            Some(&before),
            &after,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Applied
    );
}

#[test]
fn recovery_requires_changed_revision_and_requested_entity_state() {
    let request = json!({"plugin": "safe-plugin"});
    let absent = json!({"plugins": []});
    let present = json!({"plugins": [{"plugin": "safe-plugin"}]});

    assert!(matches!(
        verify_intended_effect(
            &ControlAction::PluginInstall,
            &request,
            None,
            &present,
            Some("r1"),
            Some("r1"),
        ),
        EffectProof::Absent(_)
    ));
    assert!(matches!(
        verify_intended_effect(
            &ControlAction::PluginInstall,
            &request,
            None,
            &absent,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Absent(_)
    ));
    assert_eq!(
        verify_intended_effect(
            &ControlAction::PluginInstall,
            &request,
            None,
            &present,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Applied
    );
}
