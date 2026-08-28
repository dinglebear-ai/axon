use std::time::Duration;

use serde_json::json;

use super::*;

#[test]
fn preserves_unknown_server_request_and_notification_methods() {
    let epoch = RuntimeEpoch(7);
    let request = parse_frame(
        epoch,
        br#"{"jsonrpc":"2.0","id":4,"method":"future/do","params":{"x":1}}"#,
    )
    .unwrap();
    assert_eq!(
        request,
        IncomingMessage::ServerRequest {
            id: RequestId { epoch, sequence: 4 },
            method: "future/do".to_owned(),
            params: json!({"x": 1}),
        }
    );

    let notification = parse_frame(
        epoch,
        br#"{"jsonrpc":"2.0","method":"future/changed","params":{"ok":true}}"#,
    )
    .unwrap();
    assert!(matches!(
        notification,
        IncomingMessage::Notification { method, .. } if method == "future/changed"
    ));
}

#[tokio::test]
async fn correlates_response_in_constant_time_registry() {
    let pending = PendingRequests::new(RuntimeEpoch(3));
    let request = pending.register().unwrap();
    let id = request.id;
    pending.resolve(id, Ok(json!({"model": "gpt"}))).unwrap();
    assert_eq!(
        request.wait(Duration::from_millis(20)).await.unwrap(),
        Ok(json!({"model": "gpt"}))
    );
}

#[tokio::test]
async fn timeout_unregisters_request_and_rejects_late_response() {
    let pending = PendingRequests::new(RuntimeEpoch(5));
    let request = pending.register().unwrap();
    let id = request.id;
    assert_eq!(
        request.wait(Duration::from_millis(1)).await,
        Err(ProtocolError::Timeout(id))
    );
    assert_eq!(
        pending.resolve(id, Ok(Value::Null)),
        Err(ProtocolError::UnknownRequest(id))
    );
}

#[tokio::test]
async fn restart_interrupts_pending_and_prevents_cross_epoch_resolution() {
    let pending = PendingRequests::new(RuntimeEpoch(8));
    let request = pending.register().unwrap();
    let id = request.id;
    pending.restart(RuntimeEpoch(9)).unwrap();
    assert_eq!(
        request.wait(Duration::from_millis(20)).await,
        Err(ProtocolError::RuntimeInterrupted {
            previous: RuntimeEpoch(8),
            next: RuntimeEpoch(9),
        })
    );
    assert_eq!(
        pending.resolve(id, Ok(Value::Null)),
        Err(ProtocolError::UnknownRequest(id))
    );
}

#[test]
fn enforces_frame_size_and_json_depth_limits() {
    let oversized = vec![b' '; MAX_FRAME_BYTES + 1];
    assert!(matches!(
        parse_frame(RuntimeEpoch(1), &oversized),
        Err(ProtocolError::FrameTooLarge { .. })
    ));

    let mut nested = String::new();
    nested.extend(std::iter::repeat_n('[', MAX_JSON_DEPTH + 1));
    nested.push('0');
    nested.extend(std::iter::repeat_n(']', MAX_JSON_DEPTH + 1));
    assert!(matches!(
        parse_frame(RuntimeEpoch(1), nested.as_bytes()),
        Err(ProtocolError::JsonTooDeep { .. })
    ));
}

#[test]
fn emits_typed_requests_for_current_methods() {
    let id = RequestId {
        epoch: RuntimeEpoch(2),
        sequence: 11,
    };
    for method in [
        "initialize",
        "model/list",
        "account/rateLimits/read",
        "thread/start",
        "turn/start",
    ] {
        let request = ClientRequest::new(id, method, json!({}));
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(value["id"], 11);
        assert_eq!(value["method"], method);
    }
}
