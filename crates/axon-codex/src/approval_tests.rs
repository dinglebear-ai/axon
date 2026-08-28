use super::*;
use serde_json::json;

fn request<'a>(action: &str, params: &'a Value, timeout: Duration) -> ApprovalRequest<'a> {
    ApprovalRequest {
        action: action.into(),
        origin: "palette:user".into(),
        target: "target".into(),
        risk: ApprovalRisk::High,
        summary: "Confirm operation".into(),
        effect: "Mutates Codex control state".into(),
        params,
        timeout,
    }
}

#[test]
fn approval_is_digest_bound_and_single_use() {
    let service = ApprovalService::default();
    let params = json!({"plugin":"example"});
    let prompt = service
        .create(request("plugin/install", &params, Duration::from_secs(30)))
        .unwrap();
    assert!(
        service
            .decide(prompt.id, "wrong", ApprovalDecision::Approve)
            .unwrap_err()
            .contains("digest mismatch")
    );
    assert!(
        service
            .decide(prompt.id, &prompt.digest, ApprovalDecision::Approve)
            .is_err()
    );
}

#[test]
fn valid_decision_cannot_be_replayed() {
    let service = ApprovalService::default();
    let params = json!({"value":"gpt"});
    let prompt = service
        .create(request(
            "config/value/write",
            &params,
            Duration::from_secs(30),
        ))
        .unwrap();
    assert_eq!(
        service
            .decide(prompt.id, &prompt.digest, ApprovalDecision::Approve)
            .unwrap(),
        ApprovalDecision::Approve
    );
    assert!(
        service
            .decide(prompt.id, &prompt.digest, ApprovalDecision::Approve)
            .unwrap_err()
            .contains("consumed")
    );
}

#[tokio::test]
async fn expired_approval_is_rejected() {
    let service = ApprovalService::default();
    let params = json!({});
    let prompt = service
        .create(request(
            "mcpServer/oauth/login",
            &params,
            Duration::from_millis(1),
        ))
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(
        service
            .decide(prompt.id, &prompt.digest, ApprovalDecision::Approve)
            .unwrap_err(),
        "approval expired"
    );
}
