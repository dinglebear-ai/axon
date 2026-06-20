use std::collections::HashMap;

#[test]
fn release_checkout_sparse_paths_are_valid_when_checkout_blocks_define_sparse_checkout() {
    let workflow = include_str!("../.github/workflows/release.yml");
    let blocks = checkout_sparse_blocks(workflow);
    // The release workflow now uses a full checkout. The non-cone sparse list
    // previously omitted root Cargo.toml/Cargo.lock, which broke every build, so
    // full checkout is the chosen shape and there are no sparse blocks to
    // validate. This guard still has
    // teeth if sparse-checkout is ever reintroduced: each block below must carry
    // the required paths and disable cone mode.
    for (index, block) in blocks.iter().enumerate() {
        let paths = parse_sparse_checkout_paths(block);
        for required in ["tests", "scripts", "config", "vendor", ".cargo"] {
            assert!(
                paths.iter().any(|path| path == required),
                "checkout block #{index} is missing {required} from sparse-checkout paths; \
                 paths following sparse-checkout-cone-mode are ignored by actions/checkout"
            );
        }
        assert!(
            block
                .iter()
                .any(|line| line.trim() == "sparse-checkout-cone-mode: false"),
            "checkout block #{index} must explicitly disable cone mode"
        );
        let cone_index = block
            .iter()
            .position(|line| line.trim().starts_with("sparse-checkout-cone-mode:"))
            .expect("cone mode line is present");
        assert!(
            block[(cone_index + 1)..]
                .iter()
                .all(|line| !line.trim().starts_with(['.', '/'])
                    && !["tests", "scripts", "config", "vendor"].contains(&line.trim())),
            "checkout block #{index} has path-looking entries indented under sparse-checkout-cone-mode"
        );
    }
}

fn checkout_sparse_blocks(workflow: &str) -> Vec<Vec<&str>> {
    let lines: Vec<&str> = workflow.lines().collect();
    let mut blocks = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if !line.contains("uses: actions/checkout@") {
            continue;
        }
        let mut block = Vec::new();
        for candidate in lines.iter().skip(idx) {
            if candidate.trim_start().starts_with("- uses:") && !block.is_empty() {
                break;
            }
            if candidate.trim_start().starts_with("- name:") && !block.is_empty() {
                break;
            }
            block.push(*candidate);
        }
        if block.iter().any(|line| line.contains("sparse-checkout: |")) {
            blocks.push(block);
        }
    }
    blocks
}

fn parse_sparse_checkout_paths(block: &[&str]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut in_sparse_checkout = false;
    let mut sparse_indent = None;
    for line in block {
        let trimmed = line.trim();
        if trimmed == "sparse-checkout: |" {
            in_sparse_checkout = true;
            sparse_indent = None;
            continue;
        }
        if !in_sparse_checkout {
            continue;
        }
        if trimmed.starts_with("sparse-checkout-cone-mode:") {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }
        let indent = leading_spaces(line);
        let expected = *sparse_indent.get_or_insert(indent);
        if indent == expected {
            paths.push(trimmed.to_string());
        }
    }
    paths
}

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|ch| *ch == ' ').count()
}

#[test]
fn ci_uses_guard_for_named_cargo_test_filters() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let forbidden =
        "cargo test --locked server_mode_post_bodies_match_canonical_rest_contract_fields --lib";
    assert!(
        !workflow.contains(forbidden),
        "CI must not run stale cargo test filters that match zero tests"
    );

    let mut named_filters: HashMap<&str, &str> = HashMap::new();
    named_filters.insert(
        "rest_route_contracts_match_openapi_request_schemas",
        "scripts/cargo_test_filter_guard.py",
    );

    for (filter, guard) in named_filters {
        if workflow.contains(filter) {
            assert!(
                workflow.contains(&format!("python3 {guard} -- cargo test")),
                "named cargo test filter {filter} must be run through {guard}"
            );
        }
    }
}

#[test]
fn ci_runs_release_version_gate_before_merge() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let version_sync = workflow_job_block(workflow, "version-sync");
    assert!(
        version_sync.contains(
            "cargo xtask check-release-versions --base origin/main --head HEAD --mode pr"
        ),
        "CI must run the multi-component release version gate on pull requests"
    );
    assert!(
        version_sync.contains("fetch-depth: 0"),
        "release version gate needs tags and history"
    );
    for path in [
        "release/components.toml",
        "apps/android",
        "apps/chrome-extension",
        "apps/palette-tauri",
        "apps/web/openapi/axon.json",
        "migrations",
    ] {
        assert!(
            sparse_checkout_covers(version_sync, path),
            "version-sync checkout must include {path}"
        );
    }
}

#[test]
fn ci_xtask_compiling_jobs_checkout_release_manifest() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    for job_name in ["check", "msrv", "clippy", "test", "windows-check"] {
        let job = workflow_job_block(workflow, job_name);
        if job.contains("cargo check --workspace --all-targets")
            || job.contains("cargo clippy --workspace --all-targets")
            || job.contains("cargo nextest run --workspace")
            || job.contains("cargo test -p xtask")
            || job.contains("cargo check -p xtask")
        {
            for path in [
                "release/components.toml",
                "apps/android",
                "apps/chrome-extension",
                "apps/palette-tauri",
                "apps/web/openapi/axon.json",
                "migrations",
                "assets",
            ] {
                assert!(
                    sparse_checkout_covers(job, path),
                    "{job_name} compiles xtask tests and must checkout {path}"
                );
            }
        }
    }
}

#[test]
fn windows_xtask_check_avoids_duplicate_repository_scans() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let job = workflow_job_block(workflow, "windows-check");

    assert!(
        job.contains("timeout-minutes: 25"),
        "windows-check must have a bounded timeout because Windows runners can hang on repo scans"
    );
    assert!(
        job.contains("cargo check -p xtask --locked")
            && job.contains("cargo test -p xtask --locked")
            && job.contains("cargo xtask check-mcp-http"),
        "windows-check should keep the Windows-specific xtask compile/test coverage"
    );
    assert!(
        !job.contains("cargo xtask check-no-mod-rs"),
        "check-no-mod-rs already runs in the Linux no-mod-rs job and has hung on Windows"
    );
}

#[test]
fn rest_api_parity_checkout_covers_openapi_drift_inputs() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let job = workflow_job_block(workflow, "rest-api-parity");

    assert!(
        job.contains("cargo xtask check-openapi-drift"),
        "rest-api-parity must run the generated OpenAPI drift guard"
    );

    for path in ["apps/web", "apps/palette-tauri", "apps/android"] {
        assert!(
            sparse_checkout_covers(job, path),
            "rest-api-parity runs check-openapi-drift and must checkout {path}"
        );
    }
}

#[test]
fn ci_runs_android_generated_openapi_client_tests() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let job = workflow_job_block(workflow, "android-openapi-client");

    assert!(
        sparse_checkout_covers(job, "apps/android"),
        "android OpenAPI client verification must checkout apps/android"
    );
    assert!(
        sparse_checkout_covers(job, "apps/web/openapi"),
        "android OpenAPI client verification must checkout the generated OpenAPI spec"
    );
    assert!(
        job.contains(":app:verifyOpenApiGeneratedClient"),
        "CI must run the Android generated OpenAPI client verification task"
    );
    assert!(
        workflow.contains(
            "AURORA_REF: ${{ vars.AURORA_REF || '8748eb6434b3bbe4c75f25bfff71950b7efc051b' }}"
        ) && job.contains("repository: ${{ env.AURORA_REPO }}")
            && job.contains("ref: ${{ env.AURORA_REF }}")
            && job.contains("AXON_AURORA_ANDROID_PATH"),
        "android OpenAPI client verification must pin and provide the Aurora composite build path"
    );
}

#[test]
fn auto_tag_uses_validated_xtask_release_plan() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    let plan = workflow_job_block(workflow, "plan");
    let release = workflow_job_block(workflow, "release");
    assert!(
        plan.contains("cargo xtask check-release-versions --head HEAD --mode main --json"),
        "auto-tag must use the validated shared xtask release-version detector"
    );
    assert!(
        plan.contains("fetch-depth: 0"),
        "auto-tag release planning needs tag history"
    );
    assert!(
        plan.contains(
            "matrix=$(jq -c '{include: [.[] | select(.changed == true)]}' release-plan.json)"
        ),
        "auto-tag matrix must include only changed components"
    );
    assert!(
        release.contains(r#"needs.plan.outputs.matrix != '{"include":[]}'"#),
        "auto-tag must skip release job for an empty matrix"
    );
    assert!(
        release.contains("fromJson(needs.plan.outputs.matrix)"),
        "auto-tag must expand the xtask plan as a matrix"
    );
    assert!(
        release.contains("matrix.candidate_tag") && release.contains("matrix.release_workflow"),
        "auto-tag must consume tags and workflows from the xtask release plan"
    );
    assert!(
        release
            .find("Wait for CI to pass on this commit")
            .expect("CI wait step")
            < release.find("Create and push tag").expect("tag step"),
        "auto-tag must wait for CI before creating release tags"
    );
    for required in [
        "if ! runs_json=$(gh run list",
        "gh run list failed while polling ci.yml",
        "--branch main",
        "--event push",
        ".headSha == $sha",
        ".event == \"push\"",
        ".headBranch == \"main\"",
    ] {
        assert!(
            release.contains(required),
            "auto-tag CI polling must constrain {required}"
        );
    }
}

#[test]
fn ci_has_changed_path_classifier_and_stable_gate() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    assert!(
        workflow.contains("changes:"),
        "CI must define a changes job"
    );
    assert!(
        workflow.contains("scripts/ci/changed_paths.py"),
        "CI must use the tested changed path classifier"
    );
    assert!(workflow.contains("ci-gate:"), "CI must expose ci-gate");
    assert!(
        !workflow.contains("production-gate:"),
        "production-gate should be replaced by ci-gate so branch protection has one clear required check"
    );
}

#[test]
fn ci_gate_covers_expensive_and_contract_jobs() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let gate = workflow_job_block(workflow, "ci-gate");
    for job in [
        "mcp-transport-modes",
        "version-sync",
        "aurora-primitive-inventory",
        "android",
        "android-openapi-client",
        "no-mod-rs",
        "toml-fmt",
        "lefthook-pre-commit-speed",
        "palette-tauri",
        "windows-check",
        "windows-build",
        "shell-completions-smoke",
        "web-panel",
        "mcp-schema-doc-sync",
        "rest-api-parity",
        "mcp-oauth-smoke",
        "advisory-lock-policy",
        "ban-skip-validation",
        "monolith",
        "fmt",
        "check",
        "msrv",
        "clippy",
        "test",
        "security",
        "mcp-smoke",
        "rag-changes",
        "live-rag-pr",
        "release",
        "release-smoke",
    ] {
        assert!(
            gate.contains(&format!("- {job}")),
            "ci-gate must need {job}"
        );
        assert!(
            gate.contains(&format!("require_success_or_skipped {job}")),
            "ci-gate must verify {job}"
        );
    }
}

#[test]
fn compose_and_docker_workflows_use_changed_path_classifier() {
    let compose = include_str!("../.github/workflows/compose-smoke.yml");
    let docker = include_str!("../.github/workflows/docker-image.yml");
    assert!(compose.contains("scripts/ci/changed_paths.py"));
    assert!(compose.contains("AXON_CHANGED_PATHS"));
    assert!(compose.contains("github.event.pull_request.base.sha"));
    assert!(compose.contains("git show \"${{ github.event.pull_request.base.sha }}:$classifier\""));
    assert!(compose.contains("python3 \"$AXON_CHANGED_PATHS\""));
    assert!(compose.contains("needs.changes.outputs.compose == 'true'"));
    assert!(compose.contains("needs.changes.outputs.docker == 'true'"));
    assert!(compose.contains("compose-smoke-gate:"));
    assert!(compose.contains("require_success_or_intentional_skip compose-config"));
    assert!(compose.contains("require_success_or_intentional_skip image-build-smoke"));
    assert!(docker.contains("scripts/ci/changed_paths.py"));
    assert!(docker.contains("AXON_CHANGED_PATHS"));
    assert!(docker.contains("python3 \"$AXON_CHANGED_PATHS\""));
    assert!(docker.contains("needs.changes.outputs.docker == 'true'"));
    assert!(docker.contains("startsWith(github.ref, 'refs/tags/v')"));
}

#[test]
fn codeql_workflow_routes_language_matrix_by_changed_paths() {
    let workflow = include_str!("../.github/workflows/codeql.yml");
    assert!(workflow.contains("scripts/ci/changed_paths.py"));
    assert!(workflow.contains("AXON_CHANGED_PATHS"));
    assert!(workflow.contains("github.event.pull_request.base.sha"));
    assert!(
        workflow.contains("git show \"${{ github.event.pull_request.base.sha }}:$classifier\"")
    );
    assert!(workflow.contains("args.output.write_text"));
    assert!(workflow.contains("python3 \"$AXON_CHANGED_PATHS\""));
    assert!(
        !workflow.contains("source changed-paths.out"),
        "CodeQL must not source classifier output as shell"
    );
    assert!(workflow.contains("codeql_actions"));
    assert!(workflow.contains("codeql_javascript_typescript"));
    assert!(workflow.contains("codeql_python"));
    assert!(workflow.contains("codeql_rust"));
    assert!(workflow.contains("codeql_java_kotlin"));
    assert!(workflow.contains("fromJson(needs.changes.outputs.matrix)"));
    assert!(workflow.contains("codeql-gate:"));
    assert!(workflow.contains("require_success_or_skipped analyze"));
}

#[test]
fn ci_workflow_runs_changed_path_classifier_from_trusted_base_when_available() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    assert!(workflow.contains("AXON_CHANGED_PATHS"));
    assert!(workflow.contains("github.event.pull_request.base.sha"));
    assert!(
        workflow.contains("git show \"${{ github.event.pull_request.base.sha }}:$classifier\"")
    );
    assert!(workflow.contains("python3 \"$AXON_CHANGED_PATHS\""));
    assert!(
        !workflow.contains("python3 scripts/ci/changed_paths.py"),
        "CI should call the prepared trusted classifier path"
    );
}

fn workflow_job_block<'a>(workflow: &'a str, job_name: &str) -> &'a str {
    let marker = format!("  {job_name}:");
    let start = workflow
        .find(&marker)
        .unwrap_or_else(|| panic!("missing workflow job {job_name}"));
    let rest = &workflow[start + marker.len()..];
    let end = rest
        .lines()
        .scan(0, |offset, line| {
            let line_start = *offset;
            *offset += line.len() + 1;
            Some((line_start, line))
        })
        .skip(1)
        .find_map(|(offset, line)| {
            if line.starts_with("  ") && !line.starts_with("    ") {
                Some(offset)
            } else {
                None
            }
        })
        .unwrap_or(rest.len());
    &rest[..end]
}

fn sparse_checkout_covers(block: &str, path: &str) -> bool {
    block.lines().map(str::trim).any(|entry| {
        entry == path
            || path
                .strip_prefix(entry)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}
