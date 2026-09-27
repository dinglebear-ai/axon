use super::*;
use axon_api::source::*;
use serde_json::json;

pub(super) fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (directory, file) in [
        ("configured", "one.txt"),
        ("requested", "two.txt"),
        ("kept", "three.txt"),
    ] {
        std::fs::create_dir(root.path().join(directory)).unwrap();
        std::fs::write(root.path().join(directory).join(file), "supported text").unwrap();
    }
    root
}

fn request(source: String) -> SourceRequest {
    let mut request = SourceRequest::new(source);
    request
        .options
        .values
        .insert("exclude_paths".into(), json!(["requested/", "configured/"]));
    request
}

pub(super) fn assert_options(plan: &SourcePlan) {
    assert_eq!(
        plan.request.options.values["exclude_paths"],
        json!(["configured/", "requested/"])
    );
    assert_eq!(
        plan.route.validated_options.values["exclude_paths"],
        plan.request.options.values["exclude_paths"]
    );
}

#[tokio::test]
async fn routed_git_discovery_keeps_configured_and_requested_exclusions() {
    let root = fixture();
    let request = request("https://github.com/unraid/core".into());
    let route = crate::source::routing::resolve_source_route(&request)
        .unwrap()
        .route;
    let mut plan = git_source_plan(
        &request.source,
        &route,
        false,
        &SourceLimits::default(),
        &["configured/".into(), "configured/".into()],
    );
    assert_options(&plan);
    // Materialization owns this private option; the public router rejects it.
    plan.route
        .validated_options
        .values
        .insert("repo_root".into(), json!(root.path().to_string_lossy()));
    let manifest = axon_adapters::git::GitSourceAdapter::new()
        .discover(&plan)
        .await
        .unwrap();
    let paths: Vec<_> = manifest
        .items
        .iter()
        .filter_map(|item| item.display_path.as_deref())
        .collect();
    assert_eq!(paths, ["kept/three.txt"]);
}

#[test]
fn merging_exclusions_preserves_unrelated_options_and_substring_values() {
    let mut options = AdapterOptions::default();
    options
        .values
        .insert("exclude_paths".into(), json!([".bin", "vendor/", ".bin"]));
    options
        .values
        .insert("binary_policy".into(), json!("include"));
    merge_exclude_paths(&mut options, &["vendor/".into()]);
    assert_eq!(options.values["exclude_paths"], json!(["vendor/", ".bin"]));
    assert_eq!(options.values["binary_policy"], json!("include"));
}
