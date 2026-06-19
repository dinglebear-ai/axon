use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn classify(event: &str, files: &[&str]) -> HashMap<String, String> {
    let temp_dir = std::env::temp_dir().join(format!(
        "axon-ci-paths-{}-{}-{}",
        std::process::id(),
        files.len(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after unix epoch")
            .as_nanos()
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
        &["docs/guides/configuration.md", "README.md"],
    );
    assert_eq!(out["docs"], "true");
    assert_eq!(out["rust"], "false");
    assert_eq!(out["android"], "false");
    assert_eq!(out["palette"], "false");
    assert_eq!(out["docker"], "false");
    assert_eq!(out["codeql_rust"], "false");
}

#[test]
fn rust_core_changes_enable_runtime_release_mcp_and_rust_codeql() {
    let out = classify("pull_request", &["src/vector/ops/query.rs"]);
    assert_eq!(out["rust"], "true");
    assert_eq!(out["release"], "true");
    assert_eq!(out["mcp"], "false");
    assert_eq!(out["security"], "true");
    assert_eq!(out["codeql_rust"], "true");
    assert_eq!(out["docker"], "true");
}

#[test]
fn mcp_changes_enable_mcp_schema_and_runtime_checks() {
    let out = classify("pull_request", &["src/mcp/server/tool_schema.rs"]);
    assert_eq!(out["rust"], "true");
    assert_eq!(out["mcp"], "true");
    assert_eq!(out["release"], "true");
    assert_eq!(out["codeql_rust"], "true");
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
}

#[test]
fn workflow_dispatch_and_schedule_enable_everything() {
    for event in ["workflow_dispatch", "schedule"] {
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
            "openapi",
            "codeql_actions",
            "codeql_javascript_typescript",
            "codeql_python",
            "codeql_rust",
            "codeql_java_kotlin",
        ] {
            assert_eq!(out[key], "true", "{event} should enable {key}");
        }
    }
}
