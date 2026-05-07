use axon::services::acp::{AcpClientScaffold, validate_adapter_command};
use axon::services::types::{AcpAdapterCommand, AcpPromptTurnRequest};

#[test]
fn acp_scaffold_is_constructible() {
    let adapter = AcpAdapterCommand::new("cat", vec![]);
    let scaffold = AcpClientScaffold::new(adapter.clone());
    assert_eq!(scaffold.adapter(), &adapter);
}

#[test]
fn acp_adapter_validation_rejects_empty_program() {
    let adapter = AcpAdapterCommand::new("   ", vec!["--stdio".to_string()]);
    let err = validate_adapter_command(&adapter).expect_err("empty command should fail");
    assert!(err.to_string().contains("cannot be empty"));
}

#[test]
fn acp_prompt_turn_request_is_constructible() {
    let req = AcpPromptTurnRequest {
        session_id: Some("session-1".to_string()),
        prompt: vec!["hello".to_string()],
        model: None,
        session_mode: None,
        blocked_mcp_tools: vec![],
        mcp_servers: vec![],
    };
    assert_eq!(req.session_id.as_deref(), Some("session-1"));
    assert_eq!(req.prompt.len(), 1);
    assert_eq!(req.model, None);
}
