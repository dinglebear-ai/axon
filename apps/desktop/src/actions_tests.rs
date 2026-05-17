use super::*;

#[test]
fn contains_ignore_ascii_case_basic() {
    assert!(contains_ignore_ascii_case("Scrape URL", "scrape"));
    assert!(contains_ignore_ascii_case("Scrape URL", "SCRAPE"));
    assert!(contains_ignore_ascii_case("Scrape URL", "rApE"));
    assert!(!contains_ignore_ascii_case("Scrape URL", "crawl"));
}

#[test]
fn contains_ignore_ascii_case_edges() {
    // empty needle always matches
    assert!(contains_ignore_ascii_case("anything", ""));
    // needle longer than haystack
    assert!(!contains_ignore_ascii_case("ab", "abc"));
    // exact match
    assert!(contains_ignore_ascii_case("doctor", "DOCTOR"));
    // overlapping prefix only
    assert!(!contains_ignore_ascii_case("ask", "asks"));
}

#[test]
fn action_matches_empty_input_matches_all() {
    for action in ACTIONS {
        assert!(action_matches(*action, ""));
        assert!(action_matches(*action, "   "));
    }
}

#[test]
fn action_matches_case_insensitive_subcommand() {
    let scrape = ACTIONS
        .iter()
        .find(|a| a.subcommand == "scrape")
        .copied()
        .expect("scrape action present");
    assert!(action_matches(scrape, "SCRAPE"));
    assert!(action_matches(scrape, "ScrApe"));
    assert!(action_matches(scrape, "rap"));
}

#[test]
fn action_matches_alias_hit() {
    let ingest = ACTIONS
        .iter()
        .find(|a| a.subcommand == "ingest")
        .copied()
        .expect("ingest action present");
    // aliases include "repo", "youtube", "reddit"
    assert!(action_matches(ingest, "Repo"));
    assert!(action_matches(ingest, "YOUTUBE"));
}

#[test]
fn action_matches_label_hit() {
    let doctor = ACTIONS
        .iter()
        .find(|a| a.subcommand == "doctor")
        .copied()
        .expect("doctor action present");
    // label "Doctor" should match "doc" without to_lowercase()
    assert!(action_matches(doctor, "doc"));
    assert!(action_matches(doctor, "DOC"));
}

#[test]
fn action_matches_miss() {
    let ask = ACTIONS
        .iter()
        .find(|a| a.subcommand == "ask")
        .copied()
        .expect("ask action present");
    assert!(!action_matches(ask, "zzz"));
}
