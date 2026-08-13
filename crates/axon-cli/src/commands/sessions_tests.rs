use super::*;

#[test]
fn waited_session_label_includes_position_and_provider() {
    assert_eq!(
        session_progress_label(1, 4, SessionProvider::Codex),
        "session 2/4 · codex"
    );
}
