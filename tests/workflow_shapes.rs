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
    let contracts = workflow_job_block(workflow, "rust-contracts");
    assert!(
        contracts.contains(
            "cargo xtask check-release-versions --base origin/main --head HEAD --mode pr"
        ),
        "CI must run the multi-component release version gate on pull requests"
    );
    assert!(
        contracts.contains("fetch-depth: 0"),
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
            sparse_checkout_covers(contracts, path),
            "rust-contracts checkout must include {path}"
        );
    }
}

#[test]
fn ci_xtask_compiling_jobs_checkout_release_manifest() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    for job_name in ["clippy", "test", "windows-check"] {
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
        job.contains("timeout-minutes: 40"),
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
        "check-no-mod-rs already runs in rust-contracts and has hung on Windows"
    );
}

#[test]
fn rest_api_parity_checkout_covers_openapi_drift_inputs() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let job = workflow_job_block(workflow, "rust-contracts");

    assert!(
        job.contains("cargo xtask check-openapi-drift"),
        "rust-contracts must run the generated OpenAPI drift guard"
    );

    for path in ["apps/web", "apps/palette-tauri", "apps/android"] {
        assert!(
            sparse_checkout_covers(job, path),
            "rust-contracts runs check-openapi-drift and must checkout {path}"
        );
    }
}

#[test]
fn ci_runs_android_generated_openapi_client_tests() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let job = workflow_job_block(workflow, "android");

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
fn android_ci_setup_does_not_install_unused_emulator_packages() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let setup = workflow
        .split("      - name: Set up Android SDK")
        .nth(1)
        .and_then(|rest| rest.split("      - name: Run Android unit tests").next())
        .expect("android SDK setup block exists");

    assert!(
        setup.contains("uses: android-actions/setup-android@v3"),
        "android job must set up SDK licenses/tooling before Gradle runs"
    );
    assert!(
        setup.contains("packages: \"\""),
        "android job should not install default tools/emulator packages for unit/lint/APK builds"
    );
    assert!(
        !setup.contains("connected")
            && !setup.contains("sdkmanager emulator")
            && !setup.contains("avdmanager"),
        "android job should not require emulator setup unless connected tests are added"
    );
}

#[test]
fn android_packaging_workflows_own_the_ci_heap_override() {
    let properties = include_str!("../apps/android/gradle.properties");
    let ci = workflow_job_block(include_str!("../.github/workflows/ci.yml"), "android");
    let release = include_str!("../.github/workflows/android-release.yml");
    let heap_override =
        r#"-Dorg.gradle.jvmargs="-Xmx3072m -XX:MaxMetaspaceSize=512m -Dfile.encoding=UTF-8""#;

    assert!(
        properties.lines().any(|line| {
            line.starts_with("org.gradle.jvmargs=")
                && line.contains("-Xmx2048m")
                && line.contains("-XX:MaxMetaspaceSize=512m")
        }),
        "the release-owned Android project must retain its bounded 2 GiB local default"
    );
    assert!(
        properties
            .lines()
            .any(|line| line.trim() == "org.gradle.workers.max=2"),
        "Android CI must keep Gradle worker concurrency bounded on shared runners"
    );
    assert_eq!(
        ci.matches(heap_override).count(),
        2,
        "debug and release APK packaging in CI must each use the 3 GiB runner override"
    );
    assert_eq!(
        release.matches(heap_override).count(),
        1,
        "the Android artifact workflow must use the same 3 GiB runner override"
    );
}

#[test]
fn lefthook_pre_push_uses_path_aware_router() {
    let lefthook = include_str!("../lefthook.yml");
    let pre_push = lefthook
        .split("pre-push:")
        .nth(1)
        .expect("pre-push section exists");

    assert!(
        pre_push.contains("cargo xtask pre-push"),
        "pre-push should delegate to the path-aware router"
    );
    for always_on_heavy_command in [
        "npm --prefix apps/web run build",
        "cargo xtask check-openapi-drift",
        "cargo clippy --workspace --all-targets",
        "cargo nextest run --workspace",
    ] {
        assert!(
            !pre_push.contains(always_on_heavy_command),
            "{always_on_heavy_command} must be selected by cargo xtask pre-push, not always run by lefthook"
        );
    }
}

#[test]
fn auto_tag_uses_validated_xtask_release_plan() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    let plan = workflow_job_block(workflow, "plan");
    let ci_gate = workflow_job_block(workflow, "ci-gate");
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
            "if ! jq -e 'type == \"array\" and all(.[]; (.release_please_managed | type) == \"boolean\")' release-plan.json"
        ) && plan.contains("exit 1"),
        "auto-tag must fail closed unless every release-plan item declares boolean release_please_managed ownership"
    );
    assert!(
        plan.contains(
            "matrix=$(jq -c '{include: [.[] | select(.changed == true and .release_please_managed == false)]}' release-plan.json)"
        ),
        "auto-tag matrix must include only changed components that release-please does not own"
    );
    assert_eq!(
        plan.matches("matrix=$(jq -c").count(),
        1,
        "auto-tag must have exactly one matrix assignment so a broader selector cannot bypass ownership"
    );
    assert!(
        !plan.contains("select(.changed == true)]"),
        "the former changed-only selector would reintroduce release-please-owned components"
    );
    assert!(
        ci_gate.contains(r#"needs.plan.outputs.matrix != '{"include":[]}'"#)
            && release.contains(r#"needs.plan.outputs.matrix != '{"include":[]}'"#),
        "auto-tag must skip CI gating and releases for an empty matrix"
    );
    assert!(
        ci_gate.contains("runs-on: ubuntu-24.04")
            && ci_gate.contains("timeout-minutes: 65")
            && release.contains("runs-on: ubuntu-24.04"),
        "auto-tag polling and tagging must not consume self-hosted runners"
    );
    assert!(
        release.contains("needs: [plan, ci-gate]"),
        "the release matrix must wait for the shared CI gate"
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
        ci_gate.contains("Wait for CI to pass on this commit")
            && release.contains("Create and push tag")
            && !release.contains("Wait for CI to pass on this commit"),
        "one shared CI gate must run before the release matrix creates tags"
    );
    for required in [
        "if ! runs_json=$(gh run list",
        "--repo \"${{ github.repository }}\"",
        "gh run list failed while polling ci.yml",
        "--branch main",
        "--event push",
        ".headSha == $sha",
        ".event == \"push\"",
        ".headBranch == \"main\"",
    ] {
        assert!(
            ci_gate.contains(required),
            "auto-tag CI polling must constrain {required}"
        );
    }
}

#[test]
fn auto_tag_creates_github_release_before_explicit_artifact_dispatch() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    let release = workflow_job_block(workflow, "release");

    let tag_step = release
        .find("- name: Create and push tag")
        .expect("auto-tag creates the component tag");
    let github_release_step = release
        .find("- name: Ensure GitHub Release exists")
        .expect("auto-tag idempotently creates the GitHub Release");
    let dispatch_step = release
        .find("- name: Dispatch release workflow")
        .expect("auto-tag dispatches the artifact workflow");
    assert!(
        tag_step < github_release_step && github_release_step < dispatch_step,
        "auto-tag must push the tag, ensure its GitHub Release, then dispatch artifacts"
    );

    let github_release = &release[github_release_step..dispatch_step];
    let view = github_release
        .find("if gh release view \"$tag\" --repo \"$repo\"")
        .expect("GitHub Release existence check uses the explicit repository");
    let create = github_release
        .find("gh release create \"$tag\" --verify-tag --generate-notes --repo \"$repo\"")
        .expect("missing GitHub Release is created from the verified tag");
    assert!(
        view < create,
        "GitHub Release creation must be guarded by the idempotent existence check"
    );

    let dispatch = &release[dispatch_step..];
    assert!(
        dispatch.contains("gh workflow run \"${{ matrix.release_workflow }}\"")
            && dispatch.contains("--repo \"${{ github.repository }}\"")
            && dispatch.contains("--ref \"${{ matrix.candidate_tag }}\"")
            && dispatch.contains("-f publish=true"),
        "artifact dispatch must name the workflow, repository, tag ref, and publish input explicitly"
    );
}

#[test]
fn auto_tag_partial_success_rerun_accepts_the_existing_tag_at_the_same_commit() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    let release = workflow_job_block(workflow, "release");
    let script = workflow_step_script(
        release,
        "Create and push tag",
        "Ensure GitHub Release exists",
    );

    let tag = "v99.99.99-test";
    let script = script
        .replace("${{ matrix.candidate_tag }}", tag)
        .replace("${{ github.sha }}", "$(git rev-parse HEAD)");
    let harness = format!(
        r#"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
git init --bare "$root/remote.git"
git init "$root/checkout"
cd "$root/checkout"
git config user.name "Axon Test"
git config user.email "axon-test@example.invalid"
echo retry-fixture > README.md
git add README.md
git commit -m "retry fixture"
git remote add origin "$root/remote.git"
git push origin HEAD:main
git tag {tag}
git push origin {tag}
bash -euo pipefail -c "$AUTO_TAG_SCRIPT"
test "$(git rev-parse {tag}^{{commit}})" = "$(git rev-parse HEAD)"
"#
    );
    let mut command = command_without_git_local_env("bash");
    let output = command
        .args(["-euo", "pipefail", "-c", &harness])
        .env("AUTO_TAG_SCRIPT", script)
        .output()
        .expect("run auto-tag tag step");

    assert!(
        output.status.success(),
        "a rerun after the tag was pushed must continue to GitHub Release creation and dispatch; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn auto_tag_partial_success_rerun_accepts_the_existing_tag_after_main_advances() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    let release = workflow_job_block(workflow, "release");
    let script = workflow_step_script(
        release,
        "Create and push tag",
        "Ensure GitHub Release exists",
    );

    let tag = "v99.99.97-recovery";
    let script = script
        .replace("${{ matrix.candidate_tag }}", tag)
        .replace("${{ github.sha }}", "$(git rev-parse HEAD)");
    let harness = format!(
        r#"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
git init --bare "$root/remote.git"
git init "$root/checkout"
cd "$root/checkout"
git config user.name "Axon Test"
git config user.email "axon-test@example.invalid"
echo candidate > README.md
git add README.md
git commit -m "candidate"
candidate_sha="$(git rev-parse HEAD)"
git remote add origin "$root/remote.git"
git push origin HEAD:main
git tag {tag}
git push origin {tag}
echo advanced > README.md
git add README.md
git commit -m "advance main"
git push origin HEAD:main
git switch --detach "$candidate_sha"
bash -euo pipefail -c "$AUTO_TAG_SCRIPT"
test "$(git rev-parse {tag}^{{commit}})" = "$candidate_sha"
test "$(git ls-remote --heads origin refs/heads/main | awk 'NR == 1 {{ print $1 }}')" != "$candidate_sha"
"#
    );
    let mut command = command_without_git_local_env("bash");
    let output = command
        .args(["-euo", "pipefail", "-c", &harness])
        .env("AUTO_TAG_SCRIPT", script)
        .output()
        .expect("run auto-tag recovery step after main advances");

    assert!(
        output.status.success(),
        "a rerun must recover GitHub Release creation and dispatch after its tag was pushed, even when main advanced; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn auto_tag_rejects_a_superseded_main_commit_before_creating_a_tag() {
    let workflow = include_str!("../.github/workflows/auto-tag.yml");
    let release = workflow_job_block(workflow, "release");
    let script = workflow_step_script(
        release,
        "Create and push tag",
        "Ensure GitHub Release exists",
    );

    let tag = "v99.99.98-superseded";
    let script = script
        .replace("${{ matrix.candidate_tag }}", tag)
        .replace("${{ github.sha }}", "$(git rev-parse HEAD)");
    let harness = format!(
        r#"
root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT
git init --bare "$root/remote.git"
git init "$root/checkout"
cd "$root/checkout"
git config user.name "Axon Test"
git config user.email "axon-test@example.invalid"
echo candidate > README.md
git add README.md
git commit -m "candidate"
candidate_sha="$(git rev-parse HEAD)"
git remote add origin "$root/remote.git"
git push origin HEAD:main
echo advanced > README.md
git add README.md
git commit -m "advance main"
git push origin HEAD:main
git switch --detach "$candidate_sha"
if bash -euo pipefail -c "$AUTO_TAG_SCRIPT"; then
  echo "superseded workflow commit unexpectedly passed the tag guard" >&2
  exit 1
fi
if git ls-remote --exit-code --tags origin "refs/tags/{tag}" >/dev/null 2>&1; then
  echo "superseded workflow commit created remote tag {tag}" >&2
  exit 1
fi
"#
    );
    let mut command = command_without_git_local_env("bash");
    let output = command
        .args(["-euo", "pipefail", "-c", &harness])
        .env("AUTO_TAG_SCRIPT", script)
        .output()
        .expect("run auto-tag tag step against advanced remote main");

    assert!(
        output.status.success(),
        "an obsolete main push run must fail closed before tag creation; stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn release_please_fixups_validate_and_forward_pr_branch_refs() {
    let workflow = include_str!("../.github/workflows/release-please.yml");
    let fixups = workflow_job_block(workflow, "release-pr-fixups");

    for (variable, field) in [
        ("branch", "headBranchName"),
        ("base_branch", "baseBranchName"),
    ] {
        let extraction =
            format!(r#"{variable}="$(jq -er '.{field} | select(length > 0)' <<<"$pr")""#);
        assert!(
            fixups.contains(&extraction),
            "release PR fixups must fail closed when {field} is missing or empty"
        );
    }

    assert!(
        fixups.contains("git checkout \"$branch\""),
        "fixup planning must run from the reported release PR branch"
    );
    let (_, after_plan_start) = fixups
        .split_once("cargo xtask release-please-fixup-plan")
        .expect("release PR fixup planner invocation exists");
    let (plan_args, _) = after_plan_start
        .split_once("cargo xtask check-release-versions")
        .expect("release version check follows fixup planning");
    assert!(
        plan_args.contains("--base \"origin/$base_branch\"") && plan_args.contains("--head HEAD"),
        "the fixup planner itself must compare the release branch with its reported base branch"
    );
}

#[test]
fn ci_keeps_expensive_artifacts_off_ordinary_pull_requests() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let smoke = workflow_job_block(workflow, "smoke-binary");
    assert!(smoke.contains("github.event_name == 'pull_request'"));
    assert!(smoke.contains("cargo build --locked --bin axon"));
    assert!(!smoke.contains("cargo build --release"));

    let binary_smoke_build = workflow_job_block(workflow, "binary-smoke-build");
    let mcp_smoke = workflow_job_block(workflow, "mcp-smoke");
    let windows_check = workflow_job_block(workflow, "windows-check");
    let windows_build = workflow_job_block(workflow, "windows-build");
    assert!(binary_smoke_build.contains("github.event_name != 'pull_request'"));
    assert!(binary_smoke_build.contains("needs.changes.outputs.release == 'true'"));
    assert!(binary_smoke_build.contains("'ci:full'"));
    assert!(!binary_smoke_build.contains("cargo build --release"));
    assert!(mcp_smoke.contains("'ci:full'"));
    assert!(windows_check.contains("github.event_name == 'pull_request'"));
    assert!(windows_build.contains("'ci:full'"));
}

#[test]
fn ci_builds_web_assets_once_for_binary_artifact_jobs() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let web = workflow_job_block(workflow, "web-panel");
    let binary_smoke_build = workflow_job_block(workflow, "binary-smoke-build");
    let windows = workflow_job_block(workflow, "windows-build");

    assert!(web.contains("npm --prefix apps/web run build"));
    assert!(web.contains("name: axon-web-assets"));
    for (name, job) in [
        ("binary-smoke-build", binary_smoke_build),
        ("windows-build", windows),
    ] {
        assert!(
            job.contains("uses: actions/download-artifact@v5")
                && job.contains("name: axon-web-assets"),
            "{name} must reuse the web-panel artifact"
        );
        assert!(
            !job.contains("npm ci --prefix apps/web"),
            "{name} must not reinstall web dependencies"
        );
    }
}

#[test]
fn rust_setup_installs_sqlite_for_cross_process_regressions() {
    let setup = include_str!("../.github/actions/setup-rust-kache/action.yml");
    assert!(
        setup.contains("command -v sqlite3 >/dev/null 2>&1 || need_install=true"),
        "the shared Rust setup must detect a missing sqlite3 CLI"
    );
    assert!(
        setup.contains(
            r#"packages="build-essential pkg-config ripgrep sqlite3 libssl-dev libdbus-1-dev""#
        ),
        "the shared Rust setup must install sqlite3 for cross-process WAL and stress tests"
    );
}

#[test]
fn linux_smoke_artifact_uses_a_pinned_compatible_runtime() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let binary_smoke_build = workflow_job_block(workflow, "binary-smoke-build");
    let mcp_smoke = workflow_job_block(workflow, "mcp-smoke");

    assert!(
        binary_smoke_build.contains("runs-on: ubuntu-24.04"),
        "the reusable Linux smoke binary must be built on the oldest supported smoke runtime"
    );
    assert!(
        mcp_smoke.contains("runs-on: ubuntu-24.04"),
        "the MCP consumer must run on the same pinned Ubuntu runtime as the binary producer"
    );
    assert!(
        binary_smoke_build.contains("name: axon-linux-smoke")
            && mcp_smoke.contains("name: axon-linux-smoke"),
        "the producer and MCP consumer must share the same smoke artifact"
    );
}

#[test]
fn toml_fmt_installs_rust_before_mise_cargo_tools() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let toml_fmt = workflow_job_block(workflow, "toml-fmt");
    let rust_setup = toml_fmt
        .find("uses: dtolnay/rust-toolchain@29eef336d9b2848a0b548edc03f92a220660cdb8")
        .expect("toml-fmt Rust setup");
    let mise_install = toml_fmt
        .find("uses: jdx/mise-action@e6a8b3978addb5a52f2b4cd9d91eafa7f0ab959d")
        .expect("toml-fmt mise install");

    assert!(
        toml_fmt.contains("toolchain: 1.97.1"),
        "toml-fmt must use the repository-pinned Rust toolchain"
    );
    assert!(
        rust_setup < mise_install,
        "toml-fmt must install cargo before mise invokes the cargo taplo backend"
    );
}

#[test]
fn rust_ci_uses_the_repository_toolchain_pin() {
    let toolchain = include_str!("../rust-toolchain.toml");
    let channel = toolchain
        .lines()
        .find_map(|line| line.trim().strip_prefix("channel = \""))
        .and_then(|value| value.strip_suffix('"'))
        .expect("rust-toolchain.toml channel");
    let setup = include_str!("../.github/actions/setup-rust-kache/action.yml");
    assert!(
        setup.contains(&format!("default: \"{channel}\"")),
        "the shared Rust action must default to rust-toolchain.toml's channel"
    );
    for workflow in [
        include_str!("../.github/workflows/ci.yml"),
        include_str!("../.github/workflows/release.yml"),
        include_str!("../.github/workflows/palette-release.yml"),
    ] {
        for line in workflow
            .lines()
            .filter(|line| line.trim_start().starts_with("toolchain:"))
        {
            assert!(
                line.contains(channel),
                "explicit CI toolchain must match {channel}: {line}"
            );
        }
    }
}

#[test]
fn kache_daemon_probe_is_pipefail_safe() {
    let setup = include_str!("../.github/actions/setup-rust-kache/action.yml");
    assert!(
        setup.contains("status=\"$(kache daemon status 2>&1)\""),
        "the daemon probe must capture the complete status output before matching"
    );
    assert!(
        !setup.contains("kache daemon status 2>&1 | grep -q"),
        "grep -q must not SIGPIPE the status command under pipefail"
    );
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
        "rust-contracts",
        "aurora-primitive-inventory",
        "android",
        "toml-fmt",
        "lefthook-pre-commit-speed",
        "palette-tauri",
        "windows-check",
        "windows-build",
        "smoke-binary",
        "web-panel",
        "chrome-extension",
        "clippy",
        "test",
        "security",
        "mcp-smoke",
        "rag-changes",
        "live-rag-pr",
        "binary-smoke-build",
        "binary-smoke",
    ] {
        assert!(
            gate.contains(&format!("- {job}")),
            "ci-gate must need {job}"
        );
        assert!(
            gate.contains(&format!("require_success_or_intentional_skip {job}")),
            "ci-gate must verify {job}"
        );
    }
    assert!(gate.contains("require_success changes"));
    assert!(
        !gate.contains("success|skipped"),
        "ci-gate must not accept an unexplained skipped required job"
    );
}

#[test]
fn live_rag_uses_a_dynamic_tei_host_port() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let live_rag = workflow_job_block(workflow, "live-rag-pr");
    assert!(live_rag.contains("-p 127.0.0.1::80"));
    assert!(live_rag.contains("docker port axon-tei 80/tcp"));
    assert!(live_rag.contains("echo \"TEI_URL=http://127.0.0.1:$tei_port\""));
    assert!(
        !live_rag.contains("-p 52000:80"),
        "hosted runners must not assume the production TEI port is free"
    );
}

#[test]
fn ci_runs_docs_and_chrome_contract_checks() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    let contracts = workflow_job_block(workflow, "rust-contracts");
    assert!(contracts.contains("generated-contracts check"));
    assert!(!contracts.contains("schemas generate --check"));
    assert!(!contracts.contains("docs generate --check"));

    let chrome = workflow_job_block(workflow, "chrome-extension");
    assert!(chrome.contains("needs.changes.outputs.chrome == 'true'"));
    assert!(chrome.contains("npm test --prefix apps/chrome-extension"));

    assert!(contracts.contains("needs.changes.outputs.version_files == 'true'"));
}

#[test]
fn required_codeql_is_not_variable_gated() {
    // The Claude Code Review workflow was removed (org move broke its app token);
    // only codeql remains among the always-run required contract workflows.
    let codeql = include_str!("../.github/workflows/codeql.yml");
    assert!(!codeql.contains("AXON_ENABLE_HEAVY_CI"));
    assert!(!codeql.contains("TEMP(refactor)"));
    assert!(codeql.contains("require_success analyze"));
    assert!(!codeql.contains("success|skipped"));
    assert!(
        !codeql.contains("runs-on: [self-hosted, unraid]"),
        "CodeQL must not consume the self-hosted Rust runner pool"
    );
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
    assert!(workflow.contains("require_success analyze"));
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
    let marker = format!("\n  {job_name}:\n");
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

fn workflow_step_script(job: &str, step_name: &str, next_step_name: &str) -> String {
    let step_marker = format!("      - name: {step_name}\n");
    let next_marker = format!("      - name: {next_step_name}\n");
    let step = job
        .split_once(&step_marker)
        .unwrap_or_else(|| panic!("missing workflow step {step_name}"))
        .1
        .split_once(&next_marker)
        .unwrap_or_else(|| panic!("missing workflow step {next_step_name}"))
        .0;
    let script = step
        .split_once("        run: |\n")
        .unwrap_or_else(|| panic!("workflow step {step_name} has no shell script"))
        .1;
    script
        .lines()
        .map(|line| line.strip_prefix("          ").unwrap_or(line))
        .collect::<Vec<_>>()
        .join("\n")
}

fn command_without_git_local_env(program: &str) -> std::process::Command {
    let local_env = std::process::Command::new("git")
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .expect("list repository-local Git environment variables");
    assert!(
        local_env.status.success(),
        "git rev-parse --local-env-vars failed: {}",
        String::from_utf8_lossy(&local_env.stderr)
    );

    let mut command = std::process::Command::new(program);
    for variable in String::from_utf8_lossy(&local_env.stdout)
        .lines()
        .filter(|variable| !variable.is_empty())
    {
        command.env_remove(variable);
    }
    command
}

fn sparse_checkout_covers(block: &str, path: &str) -> bool {
    // Self-hosted CI does full checkouts (sparse-checkout was removed because it
    // poisoned the shared per-runner workdir). A job with no `sparse-checkout:`
    // block checks out the entire tree, so it inherently covers every path.
    if !block.contains("sparse-checkout:") {
        return true;
    }
    block.lines().map(str::trim).any(|entry| {
        entry == path
            || path
                .strip_prefix(entry)
                .is_some_and(|suffix| suffix.starts_with('/'))
    })
}
