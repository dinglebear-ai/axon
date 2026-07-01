use super::*;

fn plan_for(files: &[&str]) -> Vec<PlanStep> {
    let paths = files
        .iter()
        .map(|path| (*path).to_owned())
        .collect::<Vec<_>>();
    let categories = classify(&paths, false);
    command_plan(&paths, &categories, false)
}

fn plan_names(plan: &[PlanStep]) -> Vec<&'static str> {
    plan.iter().map(|step| step.name).collect()
}

fn plan_commands(plan: &[PlanStep]) -> Vec<&'static str> {
    plan.iter().map(|step| step.command).collect()
}

#[test]
fn no_changed_files_have_no_local_pre_push_work() {
    let plan = plan_for(&[]);
    assert!(plan.is_empty());
}

#[test]
fn prose_docs_do_not_trigger_version_sync() {
    let paths = vec!["docs/sessions/2026-06-22-example.md".to_owned()];
    let categories = classify(&paths, false);
    assert!(categories.docs);
    assert!(!categories.version_files);
    assert!(command_plan(&paths, &categories, false).is_empty());
}

#[test]
fn version_bearing_docs_still_trigger_version_sync() {
    let plan = plan_for(&["README.md"]);
    assert_eq!(plan_names(&plan), vec!["version-sync"]);
    assert_eq!(plan_commands(&plan), vec!["cargo xtask check-version-sync"]);
}

#[test]
fn rust_changes_keep_runtime_checks() {
    let plan = plan_for(&["crates/axon-vector/src/ops/commands/query.rs"]);
    let names = plan_names(&plan);
    assert!(!names.contains(&"version-sync"));
    assert!(names.contains(&"web-assets-placeholder"));
    assert!(!names.contains(&"clippy"));
}

#[test]
fn router_changes_run_workflow_guards() {
    let plan = plan_for(&["xtask/src/pre_push.rs"]);
    let names = plan_names(&plan);
    assert!(names.contains(&"workflow-lint"));
    assert!(names.contains(&"ci-path-tests"));
    assert!(names.contains(&"workflow-shape-tests"));
    assert!(!names.contains(&"clippy"));
}
