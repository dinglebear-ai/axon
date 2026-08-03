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
fn ordinary_prose_docs_do_not_trigger_generated_contracts() {
    let paths = vec!["docs/sessions/2026-06-22-example.md".to_owned()];
    let categories = classify(&paths, false);
    assert!(categories.docs);
    assert!(!categories.version_files);
    let plan = command_plan(&paths, &categories, false);
    assert!(!plan_names(&plan).contains(&"generated-contracts"));
    assert!(plan.is_empty());
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
    assert!(names.contains(&"generated-contracts"));
    assert!(plan_commands(&plan).contains(&"cargo xtask generated-contracts check"));
    assert!(!names.contains(&"clippy"));
}

#[test]
fn api_source_changes_run_the_unified_generated_contract_gate() {
    let plan = plan_for(&["crates/axon-api/src/source/enums/runtime.rs"]);
    assert!(plan_names(&plan).contains(&"generated-contracts"));
    assert!(plan_commands(&plan).contains(&"cargo xtask generated-contracts check"));
}

#[test]
fn every_aggregate_generated_contract_output_runs_the_unified_gate() {
    let outputs = [
        "docs/reference/api/schemas.json",
        "docs/reference/api/errors.schema.json",
        "docs/reference/api/errors.md",
        "docs/reference/cli/commands.json",
        "docs/reference/cli/commands.md",
        "docs/reference/cli/axon-help.md",
        "docs/reference/rest/openapi.json",
        "docs/reference/rest/openapi.md",
        "docs/reference/rest/schemas.md",
        "docs/reference/mcp/tool-schema.json",
        "crates/axon-mcp/tests/golden/tool-schema.json",
        "docs/reference/mcp/pipeline-tool-schema.md",
        "docs/reference/config/config.schema.json",
        "docs/reference/config/env.schema.json",
        "docs/reference/config/config-toml.md",
        "docs/reference/config/env.md",
        "docs/reference/runtime/events.schema.json",
        "docs/reference/runtime/events.md",
        "docs/reference/runtime/database-schema.json",
        "docs/reference/runtime/database-schema.md",
        "docs/reference/sources/graph.schema.json",
        "docs/reference/sources/graph.md",
        "docs/reference/sources/vector-payload.schema.json",
        "docs/reference/sources/vector-payload.md",
        "docs/reference/runtime/provider-capabilities.schema.json",
        "docs/reference/runtime/provider-capabilities.md",
        "docs/reference/sources/adapter-scopes.json",
        "docs/reference/sources/adapter-scopes.md",
        "docs/reference/generated/cli.md",
        "docs/reference/generated/cli-help.md",
        "docs/reference/generated/openapi.md",
        "docs/reference/generated/mcp.md",
        "docs/reference/api/dto.md",
        "docs/reference/api/enums.md",
        "docs/reference/generated/errors.md",
        "docs/reference/runtime/observability.md",
        "docs/reference/generated/config.md",
        "docs/reference/generated/env.md",
        "docs/reference/runtime/providers.md",
        "docs/reference/runtime/schema.md",
        "docs/reference/generated/memory.md",
        "docs/reference/generated/presentation.md",
        "docs/reference/generated/schemas.md",
        "docs/reference/generated/new-source.md",
        "xtask/tests/fixtures/schemas/adapters/snapshots/adapter-scopes.json",
        "xtask/tests/fixtures/schemas/api/snapshots/schemas.json",
        "xtask/tests/fixtures/schemas/cli/snapshots/commands.json",
        "xtask/tests/fixtures/schemas/config/snapshots/config.schema.json",
        "xtask/tests/fixtures/schemas/config/snapshots/env.schema.json",
        "xtask/tests/fixtures/schemas/database/snapshots/database-schema.json",
        "xtask/tests/fixtures/schemas/errors/snapshots/errors.schema.json",
        "xtask/tests/fixtures/schemas/events/snapshots/events.schema.json",
        "xtask/tests/fixtures/schemas/graph/snapshots/graph.schema.json",
        "xtask/tests/fixtures/schemas/mcp/snapshots/tool-schema.json",
        "xtask/tests/fixtures/schemas/openapi/snapshots/openapi.json",
        "xtask/tests/fixtures/schemas/providers/snapshots/provider-capabilities.schema.json",
        "xtask/tests/fixtures/schemas/vector-payload/snapshots/vector-payload.schema.json",
    ];

    for output in outputs {
        let plan = plan_for(&[output]);
        assert!(
            plan_names(&plan).contains(&"generated-contracts"),
            "aggregate-generated output {output} skipped the unified gate"
        );
        assert!(
            plan_commands(&plan).contains(&"cargo xtask generated-contracts check"),
            "aggregate-generated output {output} selected the wrong command"
        );
    }
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
