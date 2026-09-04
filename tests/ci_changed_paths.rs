use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic per-process discriminator for classify() temp dirs. The previous
/// (pid, file-count, timestamp) name collided when two parallel tests started
/// within the same clock tick, and each classify() begins by removing "its"
/// directory — deleting the other test's live workspace (flaky failures that
/// moved between tests from run to run).
static CLASSIFY_SEQ: AtomicU64 = AtomicU64::new(0);

fn classify(event: &str, files: &[&str]) -> HashMap<String, String> {
    let temp_dir = std::env::temp_dir().join(format!(
        "axon-ci-paths-{}-{}",
        std::process::id(),
        CLASSIFY_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let changed = temp_dir.join("changed.txt");
    let output = temp_dir.join("github_output.txt");
    fs::write(&changed, files.join("\n")).expect("write changed file list");

    let status = Command::new("python3")
        .arg("scripts/ci/changed_paths.py")
        .arg("--event")
        .arg(event)
        .arg("--changed-files")
        .arg(&changed)
        .arg("--output")
        .arg(&output)
        .status()
        .expect("run changed_paths.py");
    assert!(status.success(), "changed_paths.py exited with {status}");

    let raw = fs::read_to_string(&output).expect("read github output");
    raw.lines()
        .map(|line| {
            let (key, value) = line.split_once('=').expect("key=value output");
            (key.to_string(), value.to_string())
        })
        .collect()
}

#[test]
fn docs_only_changes_skip_expensive_runtime_categories() {
    let out = classify(
        "pull_request",
        &[
            "docs/guides/configuration.md",
            "docs/sessions/2026-06-21-example.md",
        ],
    );
    assert_eq!(out["docs"], "true");
    // Prose-only docs (guides, session logs) must NOT drag in the release-version
    // gate inside rust-contracts, which compiles xtask. The step keys off
    // version_files rather than the broad docs category.
    assert_eq!(out["version_files"], "false");
    assert_eq!(out["rust"], "false");
    assert_eq!(out["docs_contracts"], "false");
    assert_eq!(out["aurora_inventory"], "false");
    assert_eq!(out["android"], "false");
    assert_eq!(out["palette"], "false");
    assert_eq!(out["docker"], "false");
    assert_eq!(out["release"], "false");
    assert_eq!(out["codeql_rust"], "false");
}

#[test]
fn ci_contract_tests_do_not_trigger_application_rust_or_codeql() {
    let out = classify("pull_request", &["tests/workflow_shapes.rs"]);
    assert_eq!(out["ci_contracts"], "true");
    assert_eq!(out["rust"], "false");
    assert_eq!(out["codeql_rust"], "false");
}

#[test]
fn agent_skill_changes_skip_expensive_runtime_categories() {
    let out = classify(
        "pull_request",
        &[
            "plugins/axon/skills/cli/SKILL.md",
            "plugins/axon/skills/cli/rules/install.md",
        ],
    );
    for key in [
        "all",
        "routing_fallback",
        "full_ci",
        "ci_all",
        "codeql_all",
        "docs",
        "docs_contracts",
        "aurora_inventory",
        "workflow",
        "rust",
        "web",
        "android",
        "palette",
        "chrome",
        "docker",
        "compose",
        "mcp",
        "security",
        "release",
        "version_files",
        "openapi",
        "codeql_actions",
        "codeql_javascript_typescript",
        "codeql_python",
        "codeql_rust",
        "codeql_java_kotlin",
    ] {
        assert_eq!(out[key], "false", ".agents changes should not enable {key}");
    }
}

#[test]
fn version_bearing_root_docs_trigger_release_contracts() {
    for file in ["README.md", "CHANGELOG.md"] {
        let out = classify("pull_request", &[file]);
        assert_eq!(
            out["version_files"], "true",
            "{file} must still trigger release version checks"
        );
        assert_eq!(out["docs"], "true", "{file} is still a docs change");
        assert_eq!(out["rust"], "false", "{file} alone is not a rust change");
        assert_eq!(
            out["release"], "true",
            "{file} is a version-bearing release change"
        );
    }
}

#[test]
fn rust_core_changes_enable_runtime_image_and_codeql_without_dependency_audit() {
    let out = classify("pull_request", &["src/vector/ops/query.rs"]);
    assert_eq!(out["rust"], "true");
    assert_eq!(out["release"], "false");
    assert_eq!(out["mcp"], "false");
    assert_eq!(out["security"], "false");
    assert_eq!(out["codeql_rust"], "true");
    assert_eq!(out["docker"], "true");
}

#[test]
fn mcp_changes_enable_mcp_schema_and_runtime_checks() {
    let out = classify("pull_request", &["src/mcp/server/tool_schema.rs"]);
    assert_eq!(out["rust"], "true");
    assert_eq!(out["mcp"], "true");
    assert_eq!(out["release"], "false");
    assert_eq!(out["codeql_rust"], "true");
}

#[test]
fn workspace_crate_changes_enable_rust_runtime_gates() {
    let out = classify("pull_request", &["crates/axon-core/src/config.rs"]);
    assert_eq!(out["rust"], "true");
    assert_eq!(out["release"], "false");
    assert_eq!(out["security"], "false");
    assert_eq!(out["docker"], "true");
    assert_eq!(out["codeql_rust"], "true");
}

#[test]
fn axon_mcp_crate_changes_enable_mcp_schema_and_runtime_checks() {
    let out = classify(
        "pull_request",
        &["crates/axon-mcp/src/server/tool_schema.rs"],
    );
    assert_eq!(out["rust"], "true");
    assert_eq!(out["mcp"], "true");
    assert_eq!(out["release"], "false");
    assert_eq!(out["codeql_rust"], "true");
}

#[test]
fn rag_paths_use_the_shared_classifier() {
    for file in [
        "crates/axon-embedding/src/lib.rs",
        "crates/axon-llm/src/lib.rs",
        "crates/axon-retrieval/src/lib.rs",
        "crates/axon-vectors/src/lib.rs",
        "crates/axon-services/src/source/runner.rs",
        "tests/rag_live_integration.rs",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["rag"], "true", "{file} should enable live RAG CI");
    }
    let out = classify("pull_request", &["crates/axon-api/src/lib.rs"]);
    assert_eq!(out["rag"], "false");
}

#[test]
fn axon_api_mcp_schema_changes_enable_mcp_contract_checks() {
    for file in [
        "crates/axon-api/src/mcp_schema.rs",
        "crates/axon-api/src/mcp_schema/requests.rs",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["rust"], "true");
        assert_eq!(out["mcp"], "true", "{file} must enable MCP checks");
        assert_eq!(out["security"], "false");
    }
}

#[test]
fn openapi_changes_enable_android_palette_and_rest_contracts() {
    let out = classify("pull_request", &["apps/web/openapi/axon.json"]);
    assert_eq!(out["openapi"], "true");
    assert_eq!(out["web"], "true");
    assert_eq!(out["android"], "true");
    assert_eq!(out["palette"], "true");
    assert_eq!(out["rust"], "false");
}

#[test]
fn android_changes_enable_kotlin_codeql_only_for_app_language() {
    let out = classify(
        "pull_request",
        &["apps/android/app/src/main/java/com/axon/app/MainActivity.kt"],
    );
    assert_eq!(out["android"], "true");
    assert_eq!(out["codeql_java_kotlin"], "true");
    assert_eq!(out["codeql_rust"], "false");
    assert_eq!(out["release_please"], "true");
}

#[test]
fn release_please_runs_only_for_components_it_owns() {
    for file in [
        "apps/android/app/build.gradle.kts",
        "apps/palette-tauri/package.json",
        "apps/chrome-extension/manifest.json",
        "release-please-config.json",
        ".release-please-manifest.json",
        ".github/workflows/release-please.yml",
    ] {
        let out = classify("push", &[file]);
        assert_eq!(out["release_please"], "true", "{file}");
    }

    for file in [
        "src/main.rs",
        "crates/axon-core/src/lib.rs",
        "apps/web/src/app.tsx",
    ] {
        let out = classify("push", &[file]);
        assert_eq!(out["release_please"], "false", "{file}");
    }
}

#[test]
fn compose_inputs_do_not_force_an_application_image_build() {
    for file in [
        ".env.example",
        "docker-compose.yaml",
        "docker-compose.prod.yaml",
        "docker-compose.external-providers.yaml",
        "docker-compose.external-qdrant.yaml",
        "docker-compose.llama.yaml",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(
            out["compose"], "true",
            "{file} should enable compose checks"
        );
        assert_eq!(out["docker"], "false", "{file} is not an image build input");
        assert_eq!(out["android"], "false", "{file} should not enable Android");
        assert_eq!(out["palette"], "false", "{file} should not enable palette");
    }
}

#[test]
fn shared_assets_route_to_web_chrome_and_the_main_image_build() {
    let out = classify("pull_request", &["assets/logo.png"]);
    assert_eq!(out["web"], "true");
    assert_eq!(out["chrome"], "true");
    assert_eq!(out["docker"], "true");
    assert_eq!(out["android"], "false");
    assert_eq!(out["palette"], "false");
}

#[test]
fn chrome_extension_changes_enable_the_chrome_gate() {
    let out = classify(
        "pull_request",
        &["apps/chrome-extension/src/background/background.js"],
    );
    assert_eq!(out["chrome"], "true");
    assert_eq!(out["codeql_javascript_typescript"], "true");
}

#[test]
fn non_language_app_assets_skip_codeql_language_builds() {
    for file in [
        "apps/web/src/styles/app.css",
        "apps/palette-tauri/src/styles/app.css",
        "apps/android/app/src/main/res/drawable/logo.xml",
        "apps/chrome-extension/manifest.json",
    ] {
        let out = classify("pull_request", &[file]);
        for key in [
            "codeql_javascript_typescript",
            "codeql_python",
            "codeql_rust",
            "codeql_java_kotlin",
        ] {
            assert_eq!(out[key], "false", "{file} must not enable {key}");
        }
    }
}

#[test]
fn dockerfile_change_routes_to_image_smoke_without_compose_validation() {
    let out = classify("pull_request", &["config/Dockerfile"]);
    assert_eq!(out["docker"], "true");
    assert_eq!(out["compose"], "false");
    assert_eq!(out["android"], "false");
    assert_eq!(out["palette"], "false");
}

#[test]
fn image_build_inputs_enable_image_smoke_without_product_apps() {
    for file in [
        ".dockerignore",
        "Cargo.toml",
        "Cargo.lock",
        "crates/axon-core/Cargo.toml",
        "config/Dockerfile",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["docker"], "true", "{file} affects image packaging");
        assert_eq!(out["android"], "false");
        assert_eq!(out["palette"], "false");
    }
}

#[test]
fn ci_executed_helper_scripts_enable_their_consuming_jobs() {
    let aurora = classify(
        "pull_request",
        &["scripts/check_aurora_primitive_inventory.py"],
    );
    assert_eq!(aurora["docs"], "true");
    assert_eq!(aurora["docs_contracts"], "true");
    assert_eq!(aurora["aurora_inventory"], "true");
    assert_eq!(aurora["android"], "false");
    assert_eq!(aurora["palette"], "false");

    for file in [
        "scripts/check_lefthook_pre_commit_speed.py",
        "scripts/refresh_generated_contracts_staged.py",
        "scripts/enforce_monoliths.py",
        "scripts/test-ask-quality-regressions.sh",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["rust"], "true", "{file} should enable Rust CI jobs");
        assert_eq!(
            out["security"], "false",
            "{file} does not change dependency inputs"
        );
        assert_eq!(out["docker"], "false", "{file} is not an image build input");
    }

    for file in [
        "scripts/test-mcp-oauth-protection.sh",
        "scripts/test-mcp-tools-mcporter.sh",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["rust"], "true", "{file} should enable Rust CI jobs");
        assert_eq!(out["mcp"], "true", "{file} should enable MCP jobs");
        assert_eq!(
            out["security"], "false",
            "{file} does not change dependency inputs"
        );
    }
}

#[test]
fn workflow_dispatch_enables_everything() {
    for event in ["workflow_dispatch"] {
        let out = classify(event, &[]);
        for key in [
            "all",
            "rust",
            "web",
            "android",
            "palette",
            "chrome",
            "docker",
            "compose",
            "mcp",
            "security",
            "release",
            "version_files",
            "openapi",
            "codeql_actions",
            "codeql_javascript_typescript",
            "codeql_python",
            "codeql_rust",
            "codeql_java_kotlin",
        ] {
            assert_eq!(out[key], "true", "{event} should enable {key}");
        }
        assert_eq!(out["routing_fallback"], "false");
    }
}

/// The weekly cron exists for the security sweep and the CodeQL languages, not
/// to rebuild every product. It used to share `workflow_dispatch`'s all-true
/// branch, so Monday rebuilt Android APKs, the Tauri desktop binary, the
/// container image and the full Rust matrix from an unchanged tree.
#[test]
fn schedule_enables_only_the_security_and_codeql_lanes() {
    let out = classify("schedule", &[]);
    for key in [
        "security",
        "codeql_actions",
        "codeql_javascript_typescript",
        "codeql_python",
        "codeql_rust",
        "codeql_java_kotlin",
    ] {
        assert_eq!(out[key], "true", "schedule should enable {key}");
    }
    for key in [
        "all",
        "rust",
        "web",
        "android",
        "palette",
        "chrome",
        "docker",
        "docker_build",
        "compose",
        "release",
        "version_files",
        "openapi",
    ] {
        assert_eq!(out[key], "false", "schedule should not enable {key}");
    }
    assert_eq!(out["routing_fallback"], "false");
}

#[test]
fn empty_pull_request_diff_is_an_explicit_conservative_fallback() {
    let out = classify("pull_request", &[]);
    assert_eq!(out["all"], "true");
    assert_eq!(out["routing_fallback"], "true");
}

#[test]
fn release_workflow_changes_do_not_enable_unrelated_product_builds() {
    let out = classify("pull_request", &[".github/workflows/release.yml"]);
    assert_eq!(out["workflow"], "true");
    assert_eq!(out["codeql_actions"], "true");
    for key in [
        "all",
        "full_ci",
        "ci_all",
        "codeql_all",
        "rust",
        "web",
        "android",
        "palette",
        "chrome",
        "docker",
        "compose",
        "mcp",
        "security",
        "release",
        "version_files",
        "openapi",
        "codeql_javascript_typescript",
        "codeql_python",
        "codeql_rust",
        "codeql_java_kotlin",
    ] {
        assert_eq!(out[key], "false", "release.yml must not enable {key}");
    }
}

#[test]
fn workflow_files_route_only_the_ci_surface_they_own() {
    for (file, enabled) in [
        (".github/workflows/ci.yml", "ci_all"),
        (".github/workflows/codeql.yml", "codeql_all"),
        (".github/workflows/android-release.yml", "android"),
        (".github/workflows/palette-release.yml", "palette"),
        (".github/workflows/chrome-extension-release.yml", "chrome"),
        (".github/workflows/compose-smoke.yml", "compose"),
        (".github/workflows/docker-image.yml", "docker"),
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["workflow"], "true", "{file} is a workflow change");
        assert_eq!(out[enabled], "true", "{file} must enable {enabled}");
        for key in [
            "full_ci", "web", "android", "palette", "chrome", "docker", "compose", "mcp", "openapi",
        ] {
            if key != enabled {
                assert_eq!(out[key], "false", "{file} must not enable {key}");
            }
        }
    }
}

#[test]
fn changed_path_router_edits_use_the_narrowest_safe_contract_lane() {
    let router = classify("pull_request", &["scripts/ci/changed_paths.py"]);
    assert_eq!(router["full_ci"], "false");
    assert_eq!(router["workflow"], "false");
    assert_eq!(router["ci_contracts"], "true");
    assert_eq!(router["codeql_python"], "true");
    for key in [
        "rust",
        "web",
        "android",
        "palette",
        "chrome",
        "docker",
        "compose",
        "mcp",
        "rag",
        "security",
        "release",
        "codeql_actions",
        "codeql_javascript_typescript",
        "codeql_rust",
        "codeql_java_kotlin",
    ] {
        assert_eq!(
            router[key], "false",
            "classifier source must not enable {key}"
        );
    }

    for file in ["tests/ci_changed_paths.rs", "tests/workflow_shapes.rs"] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["full_ci"], "false", "{file} has targeted coverage");
        assert_eq!(out["ci_contracts"], "true", "{file} is a CI contract");
        assert_eq!(out["rust"], "false", "{file} avoids the full Rust fanout");
    }

    for file in [
        "lefthook.yml",
        "scripts/clear-git-local-env.sh",
        "xtask/src/pre_push.rs",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["full_ci"], "false", "{file} has a targeted hook lane");
        assert_eq!(out["hooks"], "true", "{file} changes hook behavior");
    }
}

#[test]
fn shared_rust_setup_action_routes_only_to_its_ci_consumers() {
    let out = classify(
        "pull_request",
        &[".github/actions/setup-rust-kache/action.yml"],
    );
    for key in ["workflow", "ci_contracts", "rust", "palette", "security"] {
        assert_eq!(out[key], "true", "Rust setup action must enable {key}");
    }
    for key in [
        "ci_all", "web", "android", "chrome", "docker", "compose", "mcp", "rag",
    ] {
        assert_eq!(out[key], "false", "Rust setup action must not enable {key}");
    }
}

#[test]
fn auto_tag_inputs_match_native_shipping_and_release_planner_paths() {
    for file in [
        "src/main.rs",
        "crates/axon-core/src/lib.rs",
        "apps/web/src/main.tsx",
        "Cargo.lock",
        "vendor/example/src/lib.rs",
    ] {
        let out = classify("push", &[file]);
        assert_eq!(
            out["auto_tag"], "true",
            "{file} must enable auto-tag planning"
        );
    }
    for file in [
        "apps/android/app/src/main/MainActivity.kt",
        "apps/palette-tauri/src/main.tsx",
        ".github/workflows/auto-tag.yml",
        ".github/actions/setup-rust-kache/action.yml",
        "release/components.toml",
        "xtask-release/src/gate.rs",
        "xtask/src/main.rs",
    ] {
        let out = classify("push", &[file]);
        assert_eq!(
            out["auto_tag"], "false",
            "{file} must not enable auto-tag planning"
        );
    }
}

#[test]
fn rust_ci_helper_scripts_enable_the_jobs_that_execute_them() {
    for file in [
        "scripts/cargo_test_filter_guard.py",
        "scripts/check_shell_completions.sh",
        "scripts/generate_mcp_schema_doc.py",
    ] {
        let out = classify("pull_request", &[file]);
        assert_eq!(out["rust"], "true", "{file} should enable rust jobs");
        assert_eq!(
            out["release"], "false",
            "{file} should use the debug smoke lane rather than a release build"
        );
        assert_eq!(
            out["security"], "false",
            "{file} does not change dependency inputs"
        );
        assert_eq!(
            out["codeql_python"],
            if file.ends_with(".py") {
                "true"
            } else {
                "false"
            },
            "{file} codeql_python should track the .py extension"
        );
        assert_eq!(out["codeql_rust"], "false", "{file} is not Rust source");
    }
}
