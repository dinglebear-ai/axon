use super::*;
use serde_json::json;

#[test]
fn read_actions_are_allowed_and_mutations_default_deny() {
    let policy = WritePolicy::deny_all();
    assert!(policy.authorize(&ControlAction::AccountRead).is_ok());
    assert!(policy.authorize(&ControlAction::McpServersList).is_ok());
    assert!(policy.authorize(&ControlAction::PluginInstall).is_err());
    assert!(policy.authorize(&ControlAction::ConfigValueWrite).is_err());
}

#[test]
fn action_methods_match_current_app_server_contract() {
    assert_eq!(
        ControlAction::ConfigValueWrite.method(),
        "config/value/write"
    );
    assert_eq!(
        ControlAction::McpServersList.method(),
        "mcpServerStatus/list"
    );
    assert_eq!(ControlAction::PluginInstall.method(), "plugin/install");
    assert_eq!(ControlAction::SkillsList.method(), "skills/list");
}

#[test]
fn mutation_actions_exclude_reads_and_map_to_control_methods() {
    assert_eq!(
        MutationAction::ConfigValueWrite.method(),
        "config/value/write"
    );
    assert_eq!(
        ControlAction::from(MutationAction::McpServerReload),
        ControlAction::McpServerReload
    );
    assert!(serde_json::from_value::<MutationAction>(json!("config_read")).is_err());
    assert_eq!(
        serde_json::to_value(MutationAction::PluginInstall).unwrap(),
        json!("plugin_install")
    );
}

#[test]
fn account_projection_drops_tokens_and_masks_email() {
    let raw = json!({
        "account": {
            "type": "chatgpt",
            "email": "jacob@example.com",
            "planType": "pro",
            "accessToken": "secret-token",
            "refreshToken": "another-secret"
        }
    });
    let summary = account_summary(&raw);
    assert_eq!(summary.email_hint.as_deref(), Some("j***@example.com"));
    let encoded = serde_json::to_string(&summary).unwrap();
    assert!(!encoded.contains("secret"));
    assert!(!encoded.contains("accessToken"));
}

#[test]
fn native_plugin_and_config_mutations_reject_plaintext_secrets() {
    let action = ControlAction::PluginInstall;
    assert!(validate_mutation_params(&action, &json!({"pluginName":"example"})).is_ok());
    assert!(
        validate_mutation_params(
            &action,
            &json!({
                "pluginName":"example",
                "api_token":"plain-text"
            })
        )
        .is_err()
    );
    assert!(
        validate_mutation_params(
            &action,
            &json!({
                "pluginName":"example",
                "api_token":"env:PLUGIN_TOKEN"
            })
        )
        .is_ok()
    );
    let config = ControlAction::ConfigValueWrite;
    assert!(
        validate_mutation_params(
            &config,
            &json!({"keyPath":"providers.openai.api_key","mergeStrategy":"upsert","value":"plain"})
        )
        .is_err()
    );
    assert!(validate_mutation_params(&config, &json!({"keyPath":"providers.openai.api_key","mergeStrategy":"upsert","value":"env:OPENAI_API_KEY"})).is_ok());
    assert!(validate_mutation_params(&config, &json!({"keyPath":"providers.openai.apiKey","mergeStrategy":"upsert","value":"sk-plaintext"})).is_err());
    assert!(
        validate_mutation_params(
            &action,
            &json!({"pluginName":"example","apiKey":{"plaintext":"secret"}})
        )
        .is_err()
    );
    assert!(validate_mutation_params(&action, &json!({"pluginName":"example","downloadUrl":"https://example.com/archive?access_token=secret"})).is_err());
}

#[test]
fn marketplace_sources_are_restricted_to_stable_public_forges() {
    let action = ControlAction::MarketplaceAdd;
    assert!(
        validate_mutation_params(
            &action,
            &json!({"source":"https://github.com/acme/plugins"})
        )
        .is_ok()
    );
    for source in [
        "https://127.0.0.1/plugins",
        "https://[::1]/plugins",
        "https://metadata.internal/plugins",
        "https://attacker.example/plugins",
        "https://github.com/acme/plugins?token=secret",
    ] {
        assert!(
            validate_mutation_params(&action, &json!({"source":source})).is_err(),
            "accepted {source}"
        );
    }
}
