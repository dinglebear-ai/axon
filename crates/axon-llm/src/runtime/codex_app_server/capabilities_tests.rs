use super::*;
use serde_json::json;

// Fixtures captured from `generate-json-schema` v2 shapes for `model/list`
// and `account/rateLimits/read` responses.

fn model_list_fixture() -> Value {
    json!({
        "models": [
            { "id": "o4-mini", "defaultEffort": "medium" },
            { "id": "o3", "defaultEffort": "high" },
            { "id": "gpt-4.1", "defaultEffort": null },
            { "id": "gpt-4.1-mini" }
        ]
    })
}

fn rate_limits_fixture() -> Value {
    json!({
        "requestsRemaining": 450,
        "tokensRemaining": 1800000,
        "tier": "plus"
    })
}

fn rate_limits_snake_case_fixture() -> Value {
    json!({
        "requests_remaining": 100,
        "tokens_remaining": 50000
    })
}

#[test]
fn parse_model_list_extracts_ids_and_efforts() {
    let models = parse_model_list(&model_list_fixture());
    assert_eq!(models.len(), 4);
    assert_eq!(models[0].id, "o4-mini");
    assert_eq!(models[0].default_effort.as_deref(), Some("medium"));
    assert_eq!(models[1].id, "o3");
    assert_eq!(models[1].default_effort.as_deref(), Some("high"));
    assert_eq!(models[2].id, "gpt-4.1");
    assert_eq!(
        models[2].default_effort, None,
        "explicit null should parse as None"
    );
    assert_eq!(models[3].id, "gpt-4.1-mini");
    assert_eq!(
        models[3].default_effort, None,
        "absent key should parse as None"
    );
}

#[test]
fn parse_model_list_empty_array() {
    let result = parse_model_list(&json!({ "models": [] }));
    assert!(result.is_empty());
}

#[test]
fn parse_model_list_missing_models_key() {
    let result = parse_model_list(&json!({}));
    assert!(result.is_empty());
}

#[test]
fn parse_model_list_skips_entries_without_id() {
    let fixture = json!({
        "models": [
            { "defaultEffort": "low" },
            { "id": "gpt-4o" }
        ]
    });
    let result = parse_model_list(&fixture);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, "gpt-4o");
}

#[test]
fn parse_rate_limits_camel_case() {
    let rl = parse_rate_limits(&rate_limits_fixture());
    assert_eq!(rl.requests_remaining, Some(450));
    assert_eq!(rl.tokens_remaining, Some(1_800_000));
    assert!(rl.raw.is_some());
}

#[test]
fn parse_rate_limits_snake_case_fallback() {
    let rl = parse_rate_limits(&rate_limits_snake_case_fixture());
    assert_eq!(rl.requests_remaining, Some(100));
    assert_eq!(rl.tokens_remaining, Some(50_000));
}

#[test]
fn parse_rate_limits_prefers_camel_when_both_present() {
    let fixture = json!({
        "requestsRemaining": 99,
        "requests_remaining": 1,
        "tokensRemaining": 999,
        "tokens_remaining": 1
    });
    let rl = parse_rate_limits(&fixture);
    assert_eq!(rl.requests_remaining, Some(99));
    assert_eq!(rl.tokens_remaining, Some(999));
}

#[test]
fn parse_rate_limits_empty_object() {
    let rl = parse_rate_limits(&json!({}));
    assert_eq!(rl.requests_remaining, None);
    assert_eq!(rl.tokens_remaining, None);
}

#[test]
fn capabilities_to_json_ok_shape() {
    let caps = CodexCapabilities {
        models: Ok(parse_model_list(&model_list_fixture())),
        rate_limits: Ok(parse_rate_limits(&rate_limits_fixture())),
    };
    let json = caps.to_json();
    assert!(json["models"].is_array());
    assert_eq!(json["models"].as_array().unwrap().len(), 4);
    assert!(json["rate_limits"]["requests_remaining"].is_number());
}

#[test]
fn capabilities_to_json_error_shape() {
    let caps = CodexCapabilities {
        models: Err("model/list: no response received".to_string()),
        rate_limits: Err("account/rateLimits/read error: method not found".to_string()),
    };
    let json = caps.to_json();
    assert!(json["models"]["error"].is_string());
    assert!(json["rate_limits"]["error"].is_string());
}

#[test]
fn extract_result_surfaces_rpc_error_message() {
    let response = json!({
        "id": 10,
        "error": { "code": -32601, "message": "Method not found" }
    });
    let result = extract_result(&response);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Method not found"));
}

#[test]
fn extract_result_returns_result_field() {
    let response = json!({
        "id": 10,
        "result": { "models": [] }
    });
    let result = extract_result(&response).unwrap();
    assert!(result["models"].is_array());
}

#[test]
fn extract_result_missing_result_returns_empty_object() {
    let response = json!({ "id": 10 });
    let result = extract_result(&response).unwrap();
    assert!(result.is_object());
}
