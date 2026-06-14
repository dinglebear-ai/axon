use super::*;
use crate::core::config::Config;

#[test]
fn limiter_key_distinguishes_codex_command_and_model() {
    let req = CompletionRequest::new("hello").backend_from_config(&Config {
        llm_backend: LlmBackendKind::CodexAppServer,
        codex_cmd: "/opt/codex/bin/codex".to_string(),
        codex_model: "gpt-5.5".to_string(),
        ..Config::default()
    });

    assert_eq!(
        completion_limiter_key(&req),
        CompletionKey::Codex {
            cmd: "/opt/codex/bin/codex".to_string(),
            model: "gpt-5.5".to_string(),
        }
    );
}
