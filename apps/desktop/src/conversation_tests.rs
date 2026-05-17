use super::*;
use chrono::{DateTime, Utc};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

fn ask_argv(arg: &str) -> Vec<String> {
    vec!["--local".to_string(), "ask".to_string(), arg.to_string()]
}

#[test]
fn first_ask_no_conversation_does_not_inject_follow_up() {
    let mut argv = ask_argv("why is sky blue");
    inject_follow_up("ask", &mut argv, None);
    assert_eq!(argv, vec!["--local", "ask", "why is sky blue"]);
}

#[test]
fn second_ask_with_conversation_injects_follow_up_after_subcommand() {
    let now = Instant::now();
    let convo = AskConversation::new(now);
    let mut argv = ask_argv("and what about red");
    inject_follow_up("ask", &mut argv, Some(&convo));
    assert_eq!(
        argv,
        vec!["--local", "ask", "--follow-up", "and what about red"]
    );
}

#[test]
fn non_ask_action_never_injects_follow_up_even_with_conversation() {
    let now = Instant::now();
    let convo = AskConversation::new(now);
    let mut argv = vec![
        "--local".to_string(),
        "scrape".to_string(),
        "https://example.com".to_string(),
    ];
    inject_follow_up("scrape", &mut argv, Some(&convo));
    assert_eq!(argv, vec!["--local", "scrape", "https://example.com"]);
}

#[test]
fn conversation_starts_with_one_turn() {
    let now = Instant::now();
    let convo = AskConversation::new(now);
    assert_eq!(convo.turn_count, 1);
}

#[test]
fn bump_increments_turn_count_and_updates_last_turn_at() {
    let t0 = Instant::now();
    let mut convo = AskConversation::new(t0);
    let t1 = t0 + Duration::from_secs(5);
    convo.bump(t1);
    assert_eq!(convo.turn_count, 2);
    assert_eq!(convo.last_turn_at, t1);
}

#[test]
fn is_stale_returns_true_after_idle_timeout() {
    let t0 = Instant::now();
    let convo = AskConversation::new(t0);
    let later = t0 + CONVERSATION_IDLE_TIMEOUT + Duration::from_secs(1);
    assert!(convo.is_stale(later));
}

#[test]
fn is_stale_returns_false_within_idle_window() {
    let t0 = Instant::now();
    let convo = AskConversation::new(t0);
    let later = t0 + Duration::from_secs(60);
    assert!(!convo.is_stale(later));
}

// ---- restore_from_latest tests -----------------------------------------

fn sessions_dir(home: &Path) -> std::path::PathBuf {
    home.join(".axon").join("ask-sessions")
}

fn write_latest(home: &Path, name: &str) {
    let dir = sessions_dir(home);
    fs::create_dir_all(&dir).expect("create ask-sessions dir");
    fs::write(dir.join("latest"), format!("{name}\n")).expect("write latest");
}

fn write_session(home: &Path, name: &str, body: &str) {
    let dir = sessions_dir(home);
    fs::create_dir_all(&dir).expect("create ask-sessions dir");
    fs::write(dir.join(format!("{name}.jsonl")), body).expect("write jsonl");
}

fn turn_line(ts: DateTime<Utc>, user: &str, assistant: &str) -> String {
    let v = serde_json::json!({
        "schema": "axon.ask.turn.v1",
        "created_at": ts.to_rfc3339(),
        "collection": "axon",
        "user": user,
        "assistant": assistant,
    });
    serde_json::to_string(&v).expect("serialize turn")
}

#[test]
fn restore_from_latest_returns_none_when_latest_missing() {
    let tmp = TempDir::new().unwrap();
    let got = restore_from_latest_at(tmp.path(), Utc::now(), Instant::now());
    assert!(got.is_none());
}

#[test]
fn restore_from_latest_returns_none_when_session_file_missing() {
    let tmp = TempDir::new().unwrap();
    write_latest(tmp.path(), "ghost");
    let got = restore_from_latest_at(tmp.path(), Utc::now(), Instant::now());
    assert!(got.is_none(), "missing <name>.jsonl should yield None");
}

#[test]
fn restore_from_latest_returns_none_when_session_file_empty() {
    let tmp = TempDir::new().unwrap();
    write_latest(tmp.path(), "empty");
    write_session(tmp.path(), "empty", "");
    let got = restore_from_latest_at(tmp.path(), Utc::now(), Instant::now());
    assert!(got.is_none(), "zero turns should yield None");
}

#[test]
fn restore_from_latest_returns_none_when_last_turn_stale() {
    let tmp = TempDir::new().unwrap();
    let now = Utc::now();
    let stale = now - chrono::Duration::from_std(CONVERSATION_IDLE_TIMEOUT).unwrap()
        - chrono::Duration::seconds(5);
    write_latest(tmp.path(), "stale");
    write_session(tmp.path(), "stale", &(turn_line(stale, "hi", "yo") + "\n"));
    let got = restore_from_latest_at(tmp.path(), now, Instant::now());
    assert!(got.is_none(), "turn older than idle window should not restore");
}

#[test]
fn restore_from_latest_returns_some_with_correct_turn_count_when_fresh() {
    let tmp = TempDir::new().unwrap();
    let now = Utc::now();
    let t0 = now - chrono::Duration::seconds(120);
    let t1 = now - chrono::Duration::seconds(60);
    let t2 = now - chrono::Duration::seconds(10);
    write_latest(tmp.path(), "live");
    let body = format!(
        "{}\n{}\n{}\n",
        turn_line(t0, "q1", "a1"),
        turn_line(t1, "q2", "a2"),
        turn_line(t2, "q3", "a3"),
    );
    write_session(tmp.path(), "live", &body);
    let now_instant = Instant::now();
    let convo = restore_from_latest_at(tmp.path(), now, now_instant)
        .expect("fresh session should restore");
    assert_eq!(convo.turn_count, 3);
    // last_turn_at should be ~10s before now_instant, well inside the idle window.
    assert!(!convo.is_stale(now_instant));
    let reconstructed_age = now_instant.saturating_duration_since(convo.last_turn_at);
    assert!(
        reconstructed_age >= Duration::from_secs(5)
            && reconstructed_age <= Duration::from_secs(20),
        "reconstructed Instant should reflect ~10s wall-clock age, got {reconstructed_age:?}"
    );
}

#[test]
fn restore_from_latest_recovers_valid_lines_when_last_line_corrupt() {
    let tmp = TempDir::new().unwrap();
    let now = Utc::now();
    let t0 = now - chrono::Duration::seconds(45);
    let t1 = now - chrono::Duration::seconds(30);
    write_latest(tmp.path(), "partial");
    // Two valid turns + a truncated trailing line (no newline, not valid JSON).
    let body = format!(
        "{}\n{}\n{{\"schema\":\"axon.ask.turn.v1\",\"created_a",
        turn_line(t0, "q1", "a1"),
        turn_line(t1, "q2", "a2"),
    );
    write_session(tmp.path(), "partial", &body);
    let convo = restore_from_latest_at(tmp.path(), now, Instant::now())
        .expect("partial corruption should still recover the valid prefix");
    assert_eq!(convo.turn_count, 2);
}

#[test]
fn restore_from_latest_returns_none_when_latest_pointer_is_empty() {
    let tmp = TempDir::new().unwrap();
    let dir = sessions_dir(tmp.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("latest"), "   \n").unwrap();
    let got = restore_from_latest_at(tmp.path(), Utc::now(), Instant::now());
    assert!(got.is_none());
}

#[test]
fn restore_from_latest_rejects_path_traversal_in_latest_pointer() {
    let tmp = TempDir::new().unwrap();
    let dir = sessions_dir(tmp.path());
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("latest"), "../../etc/passwd").unwrap();
    let got = restore_from_latest_at(tmp.path(), Utc::now(), Instant::now());
    assert!(got.is_none(), "unsafe session name must not resolve");
}

#[test]
fn restore_from_latest_does_not_panic_on_garbage_file() {
    let tmp = TempDir::new().unwrap();
    write_latest(tmp.path(), "garbage");
    write_session(
        tmp.path(),
        "garbage",
        "this is not json\nneither is this\n{not even close}\n",
    );
    let got = restore_from_latest_at(tmp.path(), Utc::now(), Instant::now());
    assert!(got.is_none(), "all-corrupt file should yield None, not panic");
}

#[test]
fn inject_follow_up_handles_argv_without_subcommand_token_gracefully() {
    // Defensive: if argv was somehow built without the subcommand token,
    // we must not panic — just leave it alone.
    let mut argv = vec!["--local".to_string()];
    let convo = AskConversation::new(Instant::now());
    inject_follow_up("ask", &mut argv, Some(&convo));
    assert_eq!(argv, vec!["--local"]);
}
