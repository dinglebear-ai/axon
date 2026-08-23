use super::*;

fn repository_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask has a repository parent")
        .to_path_buf()
}

#[test]
fn projection_family_contains_all_operations() {
    let generated = generate_projection_contract(&repository_root()).unwrap();
    assert_eq!(generated["operations"].as_array().unwrap().len(), 5);
}

#[test]
fn projection_fixtures_execute_and_cover_every_operation() {
    let fixtures = load_fixtures(&repository_root()).unwrap();
    validate_fixture_coverage(&fixtures).unwrap();
    assert!(fixtures.len() >= 10);
}

#[test]
fn projection_family_emits_json_before_markdown() {
    let artifacts = projection_artifacts(&repository_root()).unwrap();
    assert_eq!(artifacts.len(), 2);
    assert!(artifacts[0].path.ends_with("projections.json"));
    assert!(artifacts[1].path.ends_with("projections.md"));
}
