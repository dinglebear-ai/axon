use super::*;

#[test]
fn every_mutation_has_an_explicit_completion_strategy() {
    for mutation in MutationAction::ALL {
        let action = ControlAction::from(mutation);
        assert!(matches!(
            completion_strategy(&action),
            CompletionStrategy::CanonicalReadback | CompletionStrategy::ResponseAcknowledged
        ));
    }
}

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
    let request = json!({
        "target": "safe-plugin",
        "source": "https://plugins.example/safe-plugin.tgz"
    });
    let absent = json!({"plugins": []});
    let source_only = json!({
        "plugins": [{"source": "https://plugins.example/safe-plugin.tgz"}]
    });
    let present = json!({"plugins": [{"id": "safe-plugin"}]});

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
            &source_only,
            Some("r1"),
            Some("r2"),
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

#[test]
fn marketplace_upgrade_uses_native_selector_but_remains_unprovable() {
    let request = json!({"marketplaceName": "official"});
    let current = json!({"marketplaces": [{"name": "official"}]});

    assert!(matches!(
        verify_intended_effect(
            &ControlAction::MarketplaceUpgrade,
            &request,
            Some(&current),
            &current,
            Some("r1"),
            Some("r1"),
        ),
        EffectProof::Unknown(_)
    ));
    assert!(matches!(
        verify_intended_effect(
            &ControlAction::MarketplaceUpgrade,
            &request,
            Some(&current),
            &current,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Unknown(_)
    ));
    assert!(matches!(
        verify_intended_effect(
            &ControlAction::MarketplaceUpgrade,
            &json!({"marketplaceName": "missing"}),
            Some(&current),
            &current,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Absent(_)
    ));
}

#[test]
fn skill_config_requires_requested_values_not_only_existence() {
    let request = json!({"name": "review", "enabled": true});
    let wrong = json!({"skills": [{"name": "review", "enabled": false}]});
    let applied = json!({"skills": [{"name": "review", "enabled": true}]});

    assert!(matches!(
        verify_intended_effect(
            &ControlAction::SkillConfigWrite,
            &request,
            None,
            &wrong,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Absent(_)
    ));
    assert_eq!(
        verify_intended_effect(
            &ControlAction::SkillConfigWrite,
            &request,
            None,
            &applied,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Applied
    );
    assert!(matches!(
        verify_intended_effect(
            &ControlAction::SkillConfigWrite,
            &request,
            None,
            &applied,
            Some("r2"),
            Some("r2"),
        ),
        EffectProof::Absent(_)
    ));
}

#[test]
fn entity_matching_is_scoped_to_the_requested_action() {
    let request = json!({"target": "shared-name"});
    let state = json!({
        "plugins": [{"name": "plugin-a"}],
        "marketplaces": [{"name": "shared-name"}],
        "servers": [{"name": "mcp-a"}]
    });

    assert!(matches!(
        verify_intended_effect(
            &ControlAction::PluginInstall,
            &request,
            None,
            &state,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Absent(_)
    ));
    assert!(matches!(
        verify_intended_effect(
            &ControlAction::McpServerOauthLogin,
            &json!({"target": "mcp-a"}),
            None,
            &state,
            Some("r1"),
            Some("r2"),
        ),
        EffectProof::Unknown(_)
    ));
}
