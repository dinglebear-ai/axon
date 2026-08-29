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
fn artifact_mutations_require_https_digest_and_secret_references() {
    let action = ControlAction::PluginInstall;
    assert!(validate_mutation_params(&action, &json!({"source":"file:///tmp/plugin"})).is_err());
    assert!(
        validate_mutation_params(
            &action,
            &json!({"source":"https://example.test/plugin.zip"})
        )
        .is_err()
    );
    assert!(
        validate_mutation_params(
            &action,
            &json!({
                "source":"https://example.test/plugin.zip",
                "sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "api_token":"plain-text"
            })
        )
        .is_err()
    );
    assert!(
        validate_mutation_params(
            &action,
            &json!({
                "source":"https://example.test/plugin.zip",
                "sha256":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                "api_token":"env:PLUGIN_TOKEN"
            })
        )
        .is_ok()
    );
}
