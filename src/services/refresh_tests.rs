use super::*;

fn origin(source_type: &str, seed_url: &str) -> RefreshOrigin {
    RefreshOrigin {
        seed_url: seed_url.to_string(),
        source_type: source_type.to_string(),
        chunks: 1,
        action: classify_action(source_type, seed_url),
    }
}

#[test]
fn web_origins_classify_as_crawl() {
    assert_eq!(
        classify_action("embed", "https://docs.example.com"),
        RefreshAction::Crawl
    );
    assert_eq!(
        classify_action("scrape", "http://example.com/page"),
        RefreshAction::Crawl
    );
    assert_eq!(
        classify_action("crawl", "https://example.com"),
        RefreshAction::Crawl
    );
}

#[test]
fn ingest_origins_classify_as_ingest() {
    assert_eq!(
        classify_action("github", "owner/repo"),
        RefreshAction::Ingest
    );
    assert_eq!(classify_action("reddit", "r/rust"), RefreshAction::Ingest);
    assert_eq!(
        classify_action("youtube", "https://youtube.com/watch?v=x"),
        RefreshAction::Ingest
    );
}

#[test]
fn sessions_and_non_url_embeds_are_skipped() {
    assert!(matches!(
        classify_action("sessions", "all"),
        RefreshAction::Skip(_)
    ));
    // A local file/dir embed: seed is a filesystem path, not re-crawlable.
    assert!(matches!(
        classify_action("embed", "/home/user/docs/file.md"),
        RefreshAction::Skip(_)
    ));
}

#[test]
fn filter_matches_source_type_or_seed_substring() {
    let gh = origin("github", "octocat/hello");
    let web = origin("embed", "https://docs.rs/serde");

    assert!(matches_filter(&gh, None));
    assert!(matches_filter(&gh, Some("github")));
    assert!(!matches_filter(&gh, Some("reddit")));

    // Domain/substring narrowing against the seed URL.
    assert!(matches_filter(&web, Some("docs.rs")));
    assert!(matches_filter(&web, Some("DOCS.RS"))); // case-insensitive
    assert!(!matches_filter(&web, Some("github")));

    // Empty/whitespace filter behaves like no filter.
    assert!(matches_filter(&web, Some("   ")));
}

#[test]
fn plan_counts_by_action() {
    let plan = RefreshPlan {
        origins: vec![
            origin("embed", "https://a.example.com"),
            origin("github", "owner/repo"),
            origin("reddit", "r/rust"),
            origin("sessions", "all"),
        ],
    };
    assert_eq!(plan.crawl_count(), 1);
    assert_eq!(plan.ingest_count(), 2);
    assert_eq!(plan.skip_count(), 1);
}
