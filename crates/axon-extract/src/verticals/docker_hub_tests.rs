use super::*;

#[test]
fn test_matches_community_image() {
    assert!(matches("https://hub.docker.com/r/library/nginx"));
    assert!(matches("https://hub.docker.com/r/bitnami/postgresql"));
    assert!(!matches("https://hub.docker.com/u/someuser"));
    assert!(!matches("https://hub.docker.com/"));
}

#[test]
fn test_build_extra_fields() {
    let extra = build_extra(
        "library",
        "nginx",
        "library/nginx",
        1_000_000,
        5000,
        true,
        "2024-01-15T10:00:00Z",
    );
    assert_eq!(extra["docker_namespace"], "library");
    assert_eq!(extra["docker_image"], "nginx");
    assert_eq!(extra["docker_full_name"], "library/nginx");
    assert_eq!(extra["docker_pulls"], 1_000_000u64);
    assert_eq!(extra["docker_stars"], 5000u64);
    assert_eq!(extra["docker_is_official"], true);
    assert_eq!(extra["docker_last_updated"], "2024-01-15T10:00:00Z");

    // Empty last_updated should not appear in output
    let extra_no_date = build_extra("myns", "myimg", "myns/myimg", 0, 0, false, "");
    assert!(extra_no_date.get("docker_last_updated").is_none());
}
