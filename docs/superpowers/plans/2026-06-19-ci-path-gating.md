# CI Path Gating Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Axon's CI run the checks that match the files changed, while keeping scheduled/manual runs broad and preserving a single branch-protection-friendly gate.

**Execution status:** Tasks 1-4 implemented and locally verified on `codex/ci-path-gating`. Task 5 branch-protection mutation is intentionally pending coordinator follow-up after the branch is pushed and a completed `ci-gate` check context exists.

**Architecture:** Add one tested path-classifier script that maps changed files into CI categories, then consume those outputs from GitHub Actions jobs. Keep one always-running aggregate gate (`ci-gate`) so branch protection can require a stable check even when expensive jobs are intentionally skipped. Apply the same path-gating model to `CI`, `CodeQL`, `Compose smoke`, `Docker image`, and GitHub branch protection.

**Tech Stack:** GitHub Actions YAML, Python 3 standard library, Rust workflow-shape tests, `gh` CLI for live GitHub protection configuration.

## Global Constraints

- `CLAUDE.md` is the source of truth for agent memory; do not edit `AGENTS.md` or `GEMINI.md` directly.
- Keep the existing Axon Rust toolchain pins in workflow jobs unless a task explicitly changes them.
- Keep scheduled and manual CI broad: `schedule` and `workflow_dispatch` should default to all categories enabled unless an existing input intentionally narrows a live test.
- Keep branch protection simple: require one stable aggregate check named `ci-gate`, not a list of path-skipped heavyweight jobs.
- Keep fail-safe behavior: if the changed-file diff cannot be resolved, run the full CI category set.
- Do not remove existing release-version gates, MCP smoke coverage, Android verification, Tauri verification, Docker image validation, CodeQL analysis, or compose validation; only route them to relevant changes.

---

## File Structure

- Create `scripts/ci/changed_paths.py`: pure-Python path classifier. It accepts an event name plus changed file list and writes GitHub output booleans.
- Create `tests/ci_changed_paths.rs`: Rust integration tests that execute `scripts/ci/changed_paths.py` against representative changed-file sets.
- Modify `.github/workflows/ci.yml`: add a `changes` job, add job-level `if:` gates, rename/replace `production-gate` with `ci-gate`, and include all important jobs in the aggregate gate.
- Modify `.github/workflows/codeql.yml`: add a `changes` job and generate a language matrix so CodeQL only analyzes languages touched by a PR/push, while schedules/manual runs analyze all languages.
- Modify `.github/workflows/compose-smoke.yml`: path-gate compose config and image smoke jobs independently.
- Modify `.github/workflows/docker-image.yml`: skip image publishing unless container/runtime inputs changed or the run is a release tag/manual dispatch.
- Modify `tests/workflow_shapes.rs`: assert the new classifier, gates, and aggregate check shape remain intact.
- Live configuration step: update GitHub branch protection/ruleset to require `ci-gate`.

---

### Task 1: Add Tested Changed-Path Classifier

**Files:**
- Create: `scripts/ci/changed_paths.py`
- Create: `tests/ci_changed_paths.rs`

**Interfaces:**
- Consumes: changed file paths from a newline-delimited file and an event name string.
- Produces: GitHub Actions output lines with keys `all`, `docs`, `workflow`, `rust`, `web`, `android`, `palette`, `chrome`, `docker`, `compose`, `mcp`, `security`, `release`, `openapi`, `codeql_actions`, `codeql_javascript_typescript`, `codeql_python`, `codeql_rust`, `codeql_java_kotlin`.

- [ ] **Step 1: Write the failing Rust integration tests**

Create `tests/ci_changed_paths.rs`:

```rust
use std::collections::HashMap;
use std::fs;
use std::process::Command;

fn classify(event: &str, files: &[&str]) -> HashMap<String, String> {
    let temp_dir = std::env::temp_dir().join(format!(
        "axon-ci-paths-{}-{}",
        std::process::id(),
        files.len()
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
    let out = classify("pull_request", &["docs/guides/configuration.md", "README.md"]);
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
    let out = classify("pull_request", &["apps/android/app/src/main/java/com/axon/app/MainActivity.kt"]);
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test --locked --test ci_changed_paths
```

Expected: FAIL because `scripts/ci/changed_paths.py` does not exist.

- [ ] **Step 3: Implement the classifier script**

Create `scripts/ci/changed_paths.py`:

```python
#!/usr/bin/env python3
"""Classify changed files into Axon CI routing categories."""

from __future__ import annotations

import argparse
from pathlib import Path


OUTPUT_KEYS = [
    "all",
    "docs",
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
    "openapi",
    "codeql_actions",
    "codeql_javascript_typescript",
    "codeql_python",
    "codeql_rust",
    "codeql_java_kotlin",
]


def starts(path: str, *prefixes: str) -> bool:
    return any(path == prefix.rstrip("/") or path.startswith(prefix) for prefix in prefixes)


def any_match(paths: list[str], predicate) -> bool:
    return any(predicate(path) for path in paths)


def classify(event: str, paths: list[str]) -> dict[str, bool]:
    if event in {"schedule", "workflow_dispatch"}:
        return {key: True for key in OUTPUT_KEYS}

    if not paths:
        return {key: True for key in OUTPUT_KEYS}

    workflow = any_match(paths, lambda p: starts(p, ".github/workflows/") or p == "tests/workflow_shapes.rs")
    docs = any_match(paths, lambda p: starts(p, "docs/") or p in {"README.md", "CHANGELOG.md"})
    openapi = any_match(paths, lambda p: starts(p, "apps/web/openapi/"))
    web = any_match(paths, lambda p: starts(p, "apps/web/")) or openapi
    android = any_match(paths, lambda p: starts(p, "apps/android/")) or openapi
    palette = any_match(paths, lambda p: starts(p, "apps/palette-tauri/")) or openapi
    chrome = any_match(paths, lambda p: starts(p, "apps/chrome-extension/", "assets/"))
    mcp = any_match(
        paths,
        lambda p: starts(p, "src/mcp/", "docs/reference/mcp/")
        or p in {"scripts/generate_mcp_schema_doc.py", "tests/workflow_shapes.rs"},
    )
    rust = any_match(
        paths,
        lambda p: starts(
            p,
            "src/",
            "xtask/",
            "benches/",
            "tests/",
            "migrations/",
            "vendor/",
            ".cargo/",
            ".config/",
        )
        or p in {"Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "Justfile"},
    )
    release = rust or web or any_match(paths, lambda p: starts(p, "release/") or p in {"README.md", "CHANGELOG.md"})
    compose = any_match(
        paths,
        lambda p: starts(p, "config/", "scripts/")
        or p in {".env.example", "docker-compose.yaml", "docker-compose.prod.yaml", "docker-compose.llama.yaml"},
    )
    docker = rust or web or compose or any_match(paths, lambda p: p == "config/Dockerfile")
    security = any_match(paths, lambda p: p in {"Cargo.lock", "deny.toml"} or starts(p, ".cargo/", "vendor/")) or rust

    codeql_actions = workflow
    codeql_javascript_typescript = web or palette or any_match(paths, lambda p: p.endswith((".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs")))
    codeql_python = any_match(paths, lambda p: p.endswith(".py") or starts(p, "scripts/"))
    codeql_rust = rust or palette
    codeql_java_kotlin = android or any_match(paths, lambda p: p.endswith((".java", ".kt", ".kts")))

    result = {
        "all": False,
        "docs": docs,
        "workflow": workflow,
        "rust": rust,
        "web": web,
        "android": android,
        "palette": palette,
        "chrome": chrome,
        "docker": docker,
        "compose": compose,
        "mcp": mcp,
        "security": security,
        "release": release,
        "openapi": openapi,
        "codeql_actions": codeql_actions,
        "codeql_javascript_typescript": codeql_javascript_typescript,
        "codeql_python": codeql_python,
        "codeql_rust": codeql_rust,
        "codeql_java_kotlin": codeql_java_kotlin,
    }

    if workflow:
        for key in OUTPUT_KEYS:
            result[key] = True

    return result


def read_paths(path: Path) -> list[str]:
    if not path.exists():
        return []
    return [line.strip() for line in path.read_text().splitlines() if line.strip()]


def write_outputs(path: Path, values: dict[str, bool]) -> None:
    lines = [f"{key}={'true' if values[key] else 'false'}" for key in OUTPUT_KEYS]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--event", required=True)
    parser.add_argument("--changed-files", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    values = classify(args.event, read_paths(args.changed_files))
    write_outputs(args.output, values)
    for key in OUTPUT_KEYS:
        print(f"{key}={str(values[key]).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
```

- [ ] **Step 4: Make the script executable**

Run:

```bash
chmod +x scripts/ci/changed_paths.py
```

Expected: no output and executable bit set.

- [ ] **Step 5: Run the classifier tests**

Run:

```bash
cargo test --locked --test ci_changed_paths
```

Expected: PASS for all six tests.

- [ ] **Step 6: Commit**

Run:

```bash
git add scripts/ci/changed_paths.py tests/ci_changed_paths.rs
git commit -m "ci: add changed path classifier"
```

Expected: commit succeeds with only the classifier and tests staged.

---

### Task 2: Path-Gate Main CI Jobs and Replace Production Gate

**Files:**
- Modify: `.github/workflows/ci.yml`
- Modify: `tests/workflow_shapes.rs`

**Interfaces:**
- Consumes: outputs from `jobs.changes.outputs.*` created by Task 1.
- Produces: job-level `if:` gates and a stable aggregate job named `ci-gate`.

- [ ] **Step 1: Add workflow-shape tests before editing YAML**

Append these tests to `tests/workflow_shapes.rs`:

```rust
#[test]
fn ci_has_changed_path_classifier_and_stable_gate() {
    let workflow = include_str!("../.github/workflows/ci.yml");
    assert!(workflow.contains("changes:"), "CI must define a changes job");
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
        "release",
        "release-smoke",
    ] {
        assert!(gate.contains(&format!("- {job}")), "ci-gate must need {job}");
    }
}
```

- [ ] **Step 2: Run the workflow-shape tests to verify they fail**

Run:

```bash
cargo test --locked --test workflow_shapes ci_has_changed_path_classifier_and_stable_gate ci_gate_covers_expensive_and_contract_jobs
```

Expected: FAIL because `ci.yml` does not yet define `changes` or `ci-gate`.

- [ ] **Step 3: Add a `changes` job near the top of `.github/workflows/ci.yml`**

Insert this job before `mcp-transport-modes`:

```yaml
  changes:
    name: changes
    runs-on: ubuntu-latest
    outputs:
      all: ${{ steps.classify.outputs.all }}
      docs: ${{ steps.classify.outputs.docs }}
      workflow: ${{ steps.classify.outputs.workflow }}
      rust: ${{ steps.classify.outputs.rust }}
      web: ${{ steps.classify.outputs.web }}
      android: ${{ steps.classify.outputs.android }}
      palette: ${{ steps.classify.outputs.palette }}
      chrome: ${{ steps.classify.outputs.chrome }}
      docker: ${{ steps.classify.outputs.docker }}
      compose: ${{ steps.classify.outputs.compose }}
      mcp: ${{ steps.classify.outputs.mcp }}
      security: ${{ steps.classify.outputs.security }}
      release: ${{ steps.classify.outputs.release }}
      openapi: ${{ steps.classify.outputs.openapi }}
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
      - name: Resolve changed files
        env:
          EVENT_NAME: ${{ github.event_name }}
          PR_BASE_SHA: ${{ github.event.pull_request.base.sha }}
          PR_HEAD_SHA: ${{ github.event.pull_request.head.sha }}
          PUSH_BEFORE_SHA: ${{ github.event.before }}
          HEAD_SHA: ${{ github.sha }}
        run: |
          set -euo pipefail
          if [[ "$EVENT_NAME" == "pull_request" ]]; then
            base="$PR_BASE_SHA"
            head="$PR_HEAD_SHA"
          elif [[ "$EVENT_NAME" == "push" ]]; then
            base="$PUSH_BEFORE_SHA"
            head="$HEAD_SHA"
          else
            : > changed-files.txt
            exit 0
          fi
          if [[ -z "${base:-}" || "$base" =~ ^0+$ ]] || ! git cat-file -e "$base" 2>/dev/null; then
            base="$(git rev-parse HEAD^ 2>/dev/null || true)"
          fi
          if [[ -z "${base:-}" ]]; then
            echo "Could not resolve base; leaving changed-files.txt empty so classifier runs fail-safe."
            : > changed-files.txt
          else
            git diff --name-only "$base" "$head" > changed-files.txt
          fi
          cat changed-files.txt
      - name: Classify changed paths
        id: classify
        run: python3 scripts/ci/changed_paths.py --event "${{ github.event_name }}" --changed-files changed-files.txt --output "$GITHUB_OUTPUT"
```

- [ ] **Step 4: Add job-level gates**

For each job below, add `needs: [changes]` unless it already has a `needs` list. If it already has `needs`, include `changes` in the list. Then add the specified `if:` expression:

```yaml
mcp-transport-modes:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' || needs.changes.outputs.mcp == 'true' }}

version-sync:
  needs: [changes]
  if: ${{ needs.changes.outputs.release == 'true' || needs.changes.outputs.android == 'true' || needs.changes.outputs.palette == 'true' || needs.changes.outputs.chrome == 'true' || needs.changes.outputs.docs == 'true' }}

aurora-primitive-inventory:
  needs: [changes]
  if: ${{ needs.changes.outputs.android == 'true' || needs.changes.outputs.palette == 'true' || needs.changes.outputs.docs == 'true' }}

android:
  needs: [changes]
  if: ${{ needs.changes.outputs.android == 'true' }}

android-openapi-client:
  needs: [changes]
  if: ${{ needs.changes.outputs.android == 'true' || needs.changes.outputs.openapi == 'true' }}

no-mod-rs:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

toml-fmt:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' || needs.changes.outputs.workflow == 'true' || needs.changes.outputs.release == 'true' }}

lefthook-pre-commit-speed:
  needs: [changes]
  if: ${{ needs.changes.outputs.workflow == 'true' || needs.changes.outputs.rust == 'true' }}

palette-tauri:
  needs: [changes]
  if: ${{ needs.changes.outputs.palette == 'true' }}

windows-check:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

windows-build:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' || needs.changes.outputs.web == 'true' }}

shell-completions-smoke:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

web-panel:
  needs: [changes]
  if: ${{ needs.changes.outputs.web == 'true' }}

mcp-schema-doc-sync:
  needs: [changes]
  if: ${{ needs.changes.outputs.mcp == 'true' }}

rest-api-parity:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' || needs.changes.outputs.openapi == 'true' || needs.changes.outputs.web == 'true' || needs.changes.outputs.android == 'true' || needs.changes.outputs.palette == 'true' }}

mcp-oauth-smoke:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' || needs.changes.outputs.mcp == 'true' }}

advisory-lock-policy:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

ban-skip-validation:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

monolith:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

fmt:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

check:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

msrv:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

clippy:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

test:
  needs: [changes]
  if: ${{ needs.changes.outputs.rust == 'true' }}

security:
  needs: [changes]
  if: ${{ needs.changes.outputs.security == 'true' }}

release:
  needs: [changes]
  if: ${{ needs.changes.outputs.release == 'true' }}

release-smoke:
  needs: [changes, release]
  if: ${{ needs.release.result == 'success' }}

mcp-smoke:
  needs: [changes, release]
  if: ${{ needs.release.result == 'success' && (needs.changes.outputs.mcp == 'true' || needs.changes.outputs.rust == 'true') }}
```

Keep the existing `if:` expressions for `test-infra`, `live-qdrant`, `rag-changes`, and `live-rag-pr`; those are already intentionally scheduled/manual/path gated.

- [ ] **Step 5: Replace `production-gate` with `ci-gate`**

Replace the current `production-gate` job with:

```yaml
  ci-gate:
    name: ci-gate
    runs-on: ubuntu-latest
    if: always()
    needs:
      - changes
      - mcp-transport-modes
      - version-sync
      - aurora-primitive-inventory
      - android
      - android-openapi-client
      - no-mod-rs
      - toml-fmt
      - lefthook-pre-commit-speed
      - palette-tauri
      - windows-check
      - windows-build
      - shell-completions-smoke
      - web-panel
      - mcp-schema-doc-sync
      - rest-api-parity
      - mcp-oauth-smoke
      - advisory-lock-policy
      - ban-skip-validation
      - monolith
      - fmt
      - check
      - msrv
      - clippy
      - test
      - security
      - mcp-smoke
      - release
      - release-smoke
    steps:
      - name: verify required jobs passed or were intentionally skipped
        run: |
          set -euo pipefail
          require_success_or_skipped() {
            local name="$1"
            local result="$2"
            case "$result" in
              success|skipped) printf '%s=%s\n' "$name" "$result" ;;
              *) echo "::error::$name concluded $result" >&2; exit 1 ;;
            esac
          }
          require_success_or_skipped changes "${{ needs.changes.result }}"
          require_success_or_skipped mcp-transport-modes "${{ needs.mcp-transport-modes.result }}"
          require_success_or_skipped version-sync "${{ needs.version-sync.result }}"
          require_success_or_skipped aurora-primitive-inventory "${{ needs.aurora-primitive-inventory.result }}"
          require_success_or_skipped android "${{ needs.android.result }}"
          require_success_or_skipped android-openapi-client "${{ needs.android-openapi-client.result }}"
          require_success_or_skipped no-mod-rs "${{ needs.no-mod-rs.result }}"
          require_success_or_skipped toml-fmt "${{ needs.toml-fmt.result }}"
          require_success_or_skipped lefthook-pre-commit-speed "${{ needs.lefthook-pre-commit-speed.result }}"
          require_success_or_skipped palette-tauri "${{ needs.palette-tauri.result }}"
          require_success_or_skipped windows-check "${{ needs.windows-check.result }}"
          require_success_or_skipped windows-build "${{ needs.windows-build.result }}"
          require_success_or_skipped shell-completions-smoke "${{ needs.shell-completions-smoke.result }}"
          require_success_or_skipped web-panel "${{ needs.web-panel.result }}"
          require_success_or_skipped mcp-schema-doc-sync "${{ needs.mcp-schema-doc-sync.result }}"
          require_success_or_skipped rest-api-parity "${{ needs.rest-api-parity.result }}"
          require_success_or_skipped mcp-oauth-smoke "${{ needs.mcp-oauth-smoke.result }}"
          require_success_or_skipped advisory-lock-policy "${{ needs.advisory-lock-policy.result }}"
          require_success_or_skipped ban-skip-validation "${{ needs.ban-skip-validation.result }}"
          require_success_or_skipped monolith "${{ needs.monolith.result }}"
          require_success_or_skipped fmt "${{ needs.fmt.result }}"
          require_success_or_skipped check "${{ needs.check.result }}"
          require_success_or_skipped msrv "${{ needs.msrv.result }}"
          require_success_or_skipped clippy "${{ needs.clippy.result }}"
          require_success_or_skipped test "${{ needs.test.result }}"
          require_success_or_skipped security "${{ needs.security.result }}"
          require_success_or_skipped mcp-smoke "${{ needs.mcp-smoke.result }}"
          require_success_or_skipped release "${{ needs.release.result }}"
          require_success_or_skipped release-smoke "${{ needs.release-smoke.result }}"
```

- [ ] **Step 6: Run local validation**

Run:

```bash
cargo test --locked --test ci_changed_paths
cargo test --locked --test workflow_shapes
```

Expected: both commands pass.

- [ ] **Step 7: Commit**

Run:

```bash
git add .github/workflows/ci.yml tests/workflow_shapes.rs
git commit -m "ci: route main checks by changed paths"
```

Expected: commit succeeds with CI workflow and workflow-shape tests staged.

---

### Task 3: Path-Gate Compose Smoke and Docker Publishing

**Files:**
- Modify: `.github/workflows/compose-smoke.yml`
- Modify: `.github/workflows/docker-image.yml`
- Modify: `tests/workflow_shapes.rs`

**Interfaces:**
- Consumes: `scripts/ci/changed_paths.py` from Task 1.
- Produces: compose and Docker workflows that skip irrelevant PR/push changes.

- [ ] **Step 1: Add workflow-shape tests**

Append to `tests/workflow_shapes.rs`:

```rust
#[test]
fn compose_and_docker_workflows_use_changed_path_classifier() {
    let compose = include_str!("../.github/workflows/compose-smoke.yml");
    let docker = include_str!("../.github/workflows/docker-image.yml");
    assert!(compose.contains("scripts/ci/changed_paths.py"));
    assert!(compose.contains("needs.changes.outputs.compose == 'true'"));
    assert!(compose.contains("needs.changes.outputs.docker == 'true'"));
    assert!(docker.contains("scripts/ci/changed_paths.py"));
    assert!(docker.contains("needs.changes.outputs.docker == 'true'"));
    assert!(docker.contains("startsWith(github.ref, 'refs/tags/v')"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --locked --test workflow_shapes compose_and_docker_workflows_use_changed_path_classifier
```

Expected: FAIL because these workflows do not yet call the classifier.

- [ ] **Step 3: Add `changes` to `.github/workflows/compose-smoke.yml`**

Insert this job before `compose-config`:

```yaml
  changes:
    name: changes
    runs-on: ubuntu-latest
    outputs:
      compose: ${{ steps.classify.outputs.compose }}
      docker: ${{ steps.classify.outputs.docker }}
    steps:
      - uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5
        with:
          fetch-depth: 0
      - name: Resolve changed files
        env:
          EVENT_NAME: ${{ github.event_name }}
          PR_BASE_SHA: ${{ github.event.pull_request.base.sha }}
          PR_HEAD_SHA: ${{ github.event.pull_request.head.sha }}
          HEAD_SHA: ${{ github.sha }}
        run: |
          set -euo pipefail
          if [[ "$EVENT_NAME" == "pull_request" ]]; then
            git diff --name-only "$PR_BASE_SHA" "$PR_HEAD_SHA" > changed-files.txt
          else
            : > changed-files.txt
          fi
          cat changed-files.txt
      - name: Classify changed paths
        id: classify
        run: python3 scripts/ci/changed_paths.py --event "${{ github.event_name }}" --changed-files changed-files.txt --output "$GITHUB_OUTPUT"
```

Add to `compose-config`:

```yaml
    needs: [changes]
    if: ${{ needs.changes.outputs.compose == 'true' }}
```

Add to `image-build-smoke`:

```yaml
    needs: [changes]
    if: ${{ needs.changes.outputs.docker == 'true' }}
```

- [ ] **Step 4: Add `changes` to `.github/workflows/docker-image.yml`**

Insert before `build`:

```yaml
  changes:
    name: changes
    runs-on: ubuntu-latest
    outputs:
      docker: ${{ steps.classify.outputs.docker }}
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
      - name: Resolve changed files
        env:
          EVENT_NAME: ${{ github.event_name }}
          PUSH_BEFORE_SHA: ${{ github.event.before }}
          HEAD_SHA: ${{ github.sha }}
        run: |
          set -euo pipefail
          if [[ "$EVENT_NAME" == "push" && "${GITHUB_REF}" != refs/tags/v* ]]; then
            git diff --name-only "$PUSH_BEFORE_SHA" "$HEAD_SHA" > changed-files.txt
          else
            : > changed-files.txt
          fi
          cat changed-files.txt
      - name: Classify changed paths
        id: classify
        run: python3 scripts/ci/changed_paths.py --event "${{ github.event_name }}" --changed-files changed-files.txt --output "$GITHUB_OUTPUT"
```

Change the `build` job header to:

```yaml
  build:
    name: build-and-push
    runs-on: ubuntu-latest
    needs: [changes]
    if: ${{ startsWith(github.ref, 'refs/tags/v') || github.event_name == 'workflow_dispatch' || needs.changes.outputs.docker == 'true' }}
```

- [ ] **Step 5: Run local validation**

Run:

```bash
cargo test --locked --test workflow_shapes compose_and_docker_workflows_use_changed_path_classifier
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add .github/workflows/compose-smoke.yml .github/workflows/docker-image.yml tests/workflow_shapes.rs
git commit -m "ci: skip compose and docker workflows for unrelated changes"
```

Expected: commit succeeds with compose, Docker, and workflow-shape test changes staged.

---

### Task 4: Path-Gate CodeQL by Language

**Files:**
- Modify: `.github/workflows/codeql.yml`
- Modify: `tests/workflow_shapes.rs`

**Interfaces:**
- Consumes: `scripts/ci/changed_paths.py` outputs from Task 1.
- Produces: CodeQL language jobs that run only when their language changed, with full matrix preserved for schedule/manual runs.

- [ ] **Step 1: Add workflow-shape test**

Append to `tests/workflow_shapes.rs`:

```rust
#[test]
fn codeql_workflow_routes_language_matrix_by_changed_paths() {
    let workflow = include_str!("../.github/workflows/codeql.yml");
    assert!(workflow.contains("scripts/ci/changed_paths.py"));
    assert!(workflow.contains("codeql_actions"));
    assert!(workflow.contains("codeql_javascript_typescript"));
    assert!(workflow.contains("codeql_python"));
    assert!(workflow.contains("codeql_rust"));
    assert!(workflow.contains("codeql_java_kotlin"));
    assert!(workflow.contains("fromJson(needs.changes.outputs.matrix)"));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --locked --test workflow_shapes codeql_workflow_routes_language_matrix_by_changed_paths
```

Expected: FAIL because `codeql.yml` does not have a classifier-backed matrix.

- [ ] **Step 3: Add a `changes` job to `codeql.yml`**

Insert before `analyze`:

```yaml
  changes:
    name: changes
    runs-on: ubuntu-latest
    outputs:
      matrix: ${{ steps.matrix.outputs.matrix }}
    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
          persist-credentials: false
      - name: Resolve changed files
        env:
          EVENT_NAME: ${{ github.event_name }}
          PR_BASE_SHA: ${{ github.event.pull_request.base.sha }}
          PR_HEAD_SHA: ${{ github.event.pull_request.head.sha }}
          PUSH_BEFORE_SHA: ${{ github.event.before }}
          HEAD_SHA: ${{ github.sha }}
        run: |
          set -euo pipefail
          if [[ "$EVENT_NAME" == "pull_request" ]]; then
            git diff --name-only "$PR_BASE_SHA" "$PR_HEAD_SHA" > changed-files.txt
          elif [[ "$EVENT_NAME" == "push" ]]; then
            git diff --name-only "$PUSH_BEFORE_SHA" "$HEAD_SHA" > changed-files.txt
          else
            : > changed-files.txt
          fi
          cat changed-files.txt
      - name: Classify changed paths
        id: classify
        run: python3 scripts/ci/changed_paths.py --event "${{ github.event_name }}" --changed-files changed-files.txt --output changed-paths.out
      - name: Build CodeQL matrix
        id: matrix
        run: |
          set -euo pipefail
          source changed-paths.out
          jq -nc \
            --arg actions "$codeql_actions" \
            --arg js "$codeql_javascript_typescript" \
            --arg py "$codeql_python" \
            --arg rust "$codeql_rust" \
            --arg kotlin "$codeql_java_kotlin" \
            '{
              include:
                ([]
                + (if $actions == "true" then [{language:"actions", build_mode:"none"}] else [] end)
                + (if $js == "true" then [{language:"javascript-typescript", build_mode:"none"}] else [] end)
                + (if $py == "true" then [{language:"python", build_mode:"none"}] else [] end)
                + (if $rust == "true" then [{language:"rust", build_mode:"none"}] else [] end)
                + (if $kotlin == "true" then [{language:"java-kotlin", build_mode:"manual"}] else [] end))
            }' > matrix.json
          if [[ "$(jq '.include | length' matrix.json)" == "0" ]]; then
            jq -nc '{include: [{language:"actions", build_mode:"none"}]}' > matrix.json
          fi
          echo "matrix=$(cat matrix.json)" >> "$GITHUB_OUTPUT"
          cat matrix.json
```

- [ ] **Step 4: Change the `analyze` job matrix**

Change `analyze` to:

```yaml
  analyze:
    name: analyze (${{ matrix.language }})
    runs-on: ubuntu-latest
    needs: [changes]
    permissions:
      security-events: write

    strategy:
      fail-fast: false
      matrix: ${{ fromJson(needs.changes.outputs.matrix) }}
```

Change CodeQL init to use `matrix.build_mode`:

```yaml
      - name: Initialize CodeQL
        uses: github/codeql-action/init@v4
        with:
          languages: ${{ matrix.language }}
          build-mode: ${{ matrix.build_mode }}
          dependency-caching: true
```

- [ ] **Step 5: Run local validation**

Run:

```bash
cargo test --locked --test workflow_shapes codeql_workflow_routes_language_matrix_by_changed_paths
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add .github/workflows/codeql.yml tests/workflow_shapes.rs
git commit -m "ci: route codeql by changed language paths"
```

Expected: commit succeeds with CodeQL workflow and workflow-shape test changes staged.

---

### Task 5: Enable Branch Protection for the Stable Gate

**Files:**
- No repository files required.

**Interfaces:**
- Consumes: GitHub check context `ci-gate` created by Task 2 after one pushed CI run.
- Produces: main-branch protection requiring `ci-gate`.

- [ ] **Step 1: Confirm the current protection state**

Run:

```bash
gh api repos/jmagar/axon/branches/main/protection --jq '{required_status_checks: .required_status_checks.contexts, strict: .required_status_checks.strict}' || true
gh api repos/jmagar/axon/rulesets --jq '.[] | {id, name, enforcement, target}'
```

Expected: branch protection may be absent; current ruleset may show `review` with `enforcement: disabled`.

- [ ] **Step 2: Wait for one pushed run to create the `ci-gate` check context**

Run:

```bash
gh run list --workflow=ci.yml --limit 1 --json databaseId,status,conclusion,headSha,url
```

Expected: latest run exists for the branch containing Task 2 and includes a completed `ci-gate` job.

- [ ] **Step 3: Require the stable gate on `main`**

Run:

```bash
gh api -X PUT repos/jmagar/axon/branches/main/protection \
  --input - <<'JSON'
{
  "required_status_checks": {
    "strict": true,
    "contexts": ["ci-gate"]
  },
  "enforce_admins": false,
  "required_pull_request_reviews": null,
  "restrictions": null,
  "required_linear_history": false,
  "allow_force_pushes": false,
  "allow_deletions": false,
  "block_creations": false,
  "required_conversation_resolution": false,
  "lock_branch": false,
  "allow_fork_syncing": true
}
JSON
```

Expected: GitHub returns a JSON branch protection object with `required_status_checks.contexts` containing `ci-gate`.

- [ ] **Step 4: Verify protection**

Run:

```bash
gh api repos/jmagar/axon/branches/main/protection --jq '{contexts: .required_status_checks.contexts, strict: .required_status_checks.strict}'
```

Expected:

```json
{"contexts":["ci-gate"],"strict":true}
```

---

### Task 6: End-to-End Verification Pull Requests

**Files:**
- No permanent repository files required beyond the tasks above.

**Interfaces:**
- Consumes: completed Tasks 1-5.
- Produces: live proof that docs-only, Android-only, Rust-only, and Docker-only changes route correctly.

- [ ] **Step 1: Create a docs-only verification branch**

Run:

```bash
git switch -c codex/ci-docs-only-routing
printf '\n<!-- ci routing smoke: docs only -->\n' >> docs/guides/configuration.md
git add docs/guides/configuration.md
git commit -m "test: verify docs-only ci routing"
git push -u origin codex/ci-docs-only-routing
gh pr create --draft --title "CI routing smoke: docs only" --body "Temporary draft PR to verify docs-only CI routing."
```

Expected: PR opens as draft.

- [ ] **Step 2: Verify docs-only CI skips expensive jobs**

Run:

```bash
RUN_ID=$(gh run list --workflow=ci.yml --branch codex/ci-docs-only-routing --event pull_request --limit 1 --json databaseId --jq '.[0].databaseId')
gh run view "$RUN_ID" --json jobs --jq '.jobs[] | {name, conclusion}'
```

Expected: `ci-gate` succeeds; Rust, Android, Tauri, release, and MCP smoke jobs are skipped unless their paths were touched by workflow changes.

- [ ] **Step 3: Close the docs-only verification PR**

Run:

```bash
gh pr close --delete-branch
git switch main
git pull --ff-only
```

Expected: draft PR closed and remote verification branch deleted.

- [ ] **Step 4: Verify classifier locally for representative changes**

Run:

```bash
printf 'src/vector/ops/query.rs\n' > /tmp/axon-rust-change.txt
python3 scripts/ci/changed_paths.py --event pull_request --changed-files /tmp/axon-rust-change.txt --output /tmp/axon-rust-output.txt
cat /tmp/axon-rust-output.txt

printf 'apps/android/app/build.gradle.kts\n' > /tmp/axon-android-change.txt
python3 scripts/ci/changed_paths.py --event pull_request --changed-files /tmp/axon-android-change.txt --output /tmp/axon-android-output.txt
cat /tmp/axon-android-output.txt

printf 'config/Dockerfile\n' > /tmp/axon-docker-change.txt
python3 scripts/ci/changed_paths.py --event pull_request --changed-files /tmp/axon-docker-change.txt --output /tmp/axon-docker-output.txt
cat /tmp/axon-docker-output.txt
```

Expected:

```text
# rust sample includes rust=true, release=true, docker=true, codeql_rust=true
# android sample includes android=true, codeql_java_kotlin=true
# docker sample includes compose=true or docker=true depending on the file, and does not enable android/palette unless related files changed
```

- [ ] **Step 5: Run final local test suite for CI routing**

Run:

```bash
cargo test --locked --test ci_changed_paths
cargo test --locked --test workflow_shapes
```

Expected: both commands pass.

---

## Self-Review

**Spec coverage:** This plan covers docs-only and general path-sensitive CI routing, Docker image publishing, CodeQL routing, compose smoke routing, aggregate gate coverage, and branch protection. It keeps scheduled/manual runs broad.

**Placeholder scan:** The plan contains no forbidden placeholder markers, no unfinished implementation step, and no instruction that asks an implementer to invent missing behavior.

**Type consistency:** The classifier output keys in Task 1 match the workflow outputs and job gates used in Tasks 2-4.
