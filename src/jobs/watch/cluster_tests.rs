use super::*;

fn seeds(urls: &[&str]) -> Vec<String> {
    let owned: Vec<String> = urls.iter().map(|s| s.to_string()).collect();
    let mut s: Vec<String> = group_by_common_prefix(&owned)
        .into_iter()
        .map(|c| c.seed)
        .collect();
    s.sort();
    s
}

#[test]
fn same_dir_one_seed() {
    let c = group_by_common_prefix(&["https://h/a/b/x".into(), "https://h/a/b/y".into()]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].seed, "https://h/a/b/");
}
#[test]
fn nested_seeds_common_ancestor() {
    let c = group_by_common_prefix(&["https://h/a/b/x".into(), "https://h/a/c/y".into()]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].seed, "https://h/a/");
}
#[test]
fn siblings_dont_merge() {
    assert_eq!(
        seeds(&["https://h/a/x", "https://h/b/y"]),
        vec!["https://h/a/x".to_string(), "https://h/b/y".to_string()]
    );
}
#[test]
fn hosts_dont_merge() {
    assert_eq!(
        seeds(&["https://h1/a/x", "https://h2/a/y"]),
        vec!["https://h1/a/x".to_string(), "https://h2/a/y".to_string()]
    );
}
#[test]
fn single_seed_is_url() {
    let c = group_by_common_prefix(&["https://h/a/b/c".into()]);
    assert_eq!(c[0].seed, "https://h/a/b/c");
}
#[test]
fn query_and_fragment_stripped_before_grouping() {
    // `?v=2` and `#frag` must not leak into the last path segment, so these two
    // cluster identically to their bare-path forms (seed = their common dir).
    let c = group_by_common_prefix(&["https://h/a/b/x?v=2".into(), "https://h/a/b/y#frag".into()]);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].seed, "https://h/a/b/");
}
#[test]
fn root_only_separate() {
    assert_eq!(
        seeds(&["https://h/", "https://h2/"]),
        vec!["https://h/".to_string(), "https://h2/".to_string()]
    );
}
