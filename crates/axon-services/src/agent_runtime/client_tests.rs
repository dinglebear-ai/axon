use super::*;

#[test]
fn oversized_agent_body_keeps_stable_error_code() {
    let error = labby_agent_body_error(axon_core::http::HttpError::ResponseTooLarge {
        max_bytes: MAX_LABBY_AGENT_RESPONSE_BYTES,
    });
    assert_eq!(error.to_string(), "labby_agent_payload_too_large");
}
