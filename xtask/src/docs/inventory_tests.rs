use super::*;
use std::fs;

const SAMPLE_CONTRACT: &str = r#"# Axon Documentation

## Required Living Documentation

```text
docs/
  README.md
  reference/
    cli/
      overview.md
      commands.md
    ...implementation contracts...
```

## Generated Artifacts
"#;

#[test]
fn parses_flat_and_nested_paths_and_skips_ellipsis() {
    let files = parse_final_docs_tree(SAMPLE_CONTRACT).unwrap();
    assert_eq!(
        files,
        vec![
            "docs/README.md".to_string(),
            "docs/reference/cli/commands.md".to_string(),
            "docs/reference/cli/overview.md".to_string(),
        ]
    );
}

#[test]
fn check_rejects_obsolete_pipeline_contract_root() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs/pipeline-unification")).unwrap();
    let err = check(dir.path()).unwrap_err();
    assert!(err.to_string().contains("docs/pipeline-unification"));
}

#[test]
fn check_reports_every_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("docs/README.md"), SAMPLE_CONTRACT).unwrap();
    let err = check(dir.path()).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("docs/README.md"));
    assert!(msg.contains("docs/reference/cli/commands.md"));
    assert!(msg.contains("docs/reference/cli/overview.md"));
}

#[test]
fn check_passes_when_all_files_exist() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("docs")).unwrap();
    fs::write(dir.path().join("docs/README.md"), SAMPLE_CONTRACT).unwrap();
    fs::create_dir_all(dir.path().join("docs/reference/cli")).unwrap();
    fs::write(dir.path().join("docs/reference/cli/commands.md"), "x").unwrap();
    fs::write(dir.path().join("docs/reference/cli/overview.md"), "x").unwrap();
    check(dir.path()).unwrap();
}
