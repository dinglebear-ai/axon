use super::*;
use std::time::Duration;

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

#[test]
fn inject_follow_up_handles_argv_without_subcommand_token_gracefully() {
    // Defensive: if argv was somehow built without the subcommand token,
    // we must not panic — just leave it alone.
    let mut argv = vec!["--local".to_string()];
    let convo = AskConversation::new(Instant::now());
    inject_follow_up("ask", &mut argv, Some(&convo));
    assert_eq!(argv, vec!["--local"]);
}
