#!/usr/bin/env python3
"""Path-aware local pre-push checks for Axon."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

import changed_paths


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW_ROUTER_PATHS = {
    "lefthook.yml",
    "scripts/ci/changed_paths.py",
    "scripts/ci/pre_push.py",
    "tests/ci_changed_paths.rs",
    "tests/workflow_shapes.rs",
}


def truthy(value: str | None) -> bool:
    return value is not None and value.lower() in {"1", "true", "yes", "on"}


def run_git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True, stderr=subprocess.DEVNULL).strip()


def git_ref_exists(ref: str) -> bool:
    return subprocess.run(
        ["git", "rev-parse", "--verify", "--quiet", ref],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    ).returncode == 0


def resolve_base() -> str:
    override = os.environ.get("AXON_PRE_PUSH_BASE")
    if override:
        return override

    for candidate in ("@{upstream}", "origin/main"):
        if not git_ref_exists(candidate):
            continue
        try:
            return run_git("merge-base", candidate, "HEAD")
        except subprocess.CalledProcessError:
            continue

    try:
        return run_git("rev-parse", "HEAD^")
    except subprocess.CalledProcessError:
        return ""


def changed_files(base: str) -> list[str] | None:
    if not base:
        return None
    try:
        raw = run_git("diff", "--name-only", base, "HEAD")
    except subprocess.CalledProcessError:
        return None
    return [line.strip() for line in raw.splitlines() if line.strip()]


def is_workflow_router_path(path: str) -> bool:
    return path.startswith(".github/workflows/") or path in WORKFLOW_ROUTER_PATHS


def classify(paths: list[str], full: bool) -> dict[str, bool]:
    if full:
        return {key: True for key in changed_paths.OUTPUT_KEYS}

    workflow_paths = [path for path in paths if is_workflow_router_path(path)]
    runtime_paths = [path for path in paths if not is_workflow_router_path(path)]

    if runtime_paths:
        result = changed_paths.classify("pull_request", runtime_paths)
    else:
        result = {key: False for key in changed_paths.OUTPUT_KEYS}

    if workflow_paths:
        result["workflow"] = True
        result["codeql_actions"] = True
        if any(path.endswith(".py") for path in workflow_paths):
            result["codeql_python"] = True

    return result


def any_path(paths: list[str], *prefixes: str) -> bool:
    return any(path == prefix.rstrip("/") or path.startswith(prefix) for path in paths for prefix in prefixes)


def any_file(paths: list[str], *names: str) -> bool:
    wanted = set(names)
    return any(path in wanted for path in paths)


def command_plan(paths: list[str], categories: dict[str, bool], full: bool) -> list[tuple[str, str]]:
    workflow_changed = any_path(paths, ".github/workflows/") or any_file(
        paths,
        "lefthook.yml",
        "scripts/ci/changed_paths.py",
        "scripts/ci/pre_push.py",
        "tests/ci_changed_paths.rs",
        "tests/workflow_shapes.rs",
    )
    env_boundary_changed = any_file(paths, "scripts/check-env-config-boundary.py", "tests/env_config_boundary.rs")
    rust_api_changed = any_path(paths, "src/web/", "src/services/", "src/mcp/", "src/cli/commands/rest/")
    android_app_changed = any_path(paths, "apps/android/")

    plan: list[tuple[str, str]] = [
        ("version-sync", "cargo xtask check-version-sync"),
    ]

    if workflow_changed:
        plan.extend(
            [
                ("python-syntax", "python3 -m py_compile scripts/ci/changed_paths.py scripts/ci/pre_push.py"),
                (
                    "workflow-lint",
                    "actionlint .github/workflows/ci.yml .github/workflows/codeql.yml "
                    ".github/workflows/compose-smoke.yml .github/workflows/docker-image.yml",
                ),
                ("ci-path-tests", "cargo test --locked --test ci_changed_paths"),
                ("workflow-shape-tests", "cargo test --locked --test workflow_shapes"),
            ]
        )

    if env_boundary_changed:
        plan.append(
            (
                "env-boundary-test",
                "cargo test --locked --features test-helpers --test env_config_boundary "
                "env_config_boundary_matrix_is_current -- --nocapture",
            )
        )

    if categories["web"]:
        plan.append(
            (
                "web-assets",
                "if [ ! -d apps/web/node_modules ]; then npm ci --prefix apps/web; fi && "
                "npm --prefix apps/web run build",
            )
        )

    if categories["rust"]:
        plan.append(("web-assets-placeholder", "mkdir -p apps/web/out"))
        plan.append(("clippy", "AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo clippy --workspace --all-targets --locked -- -D warnings"))

    if full:
        plan.append(
            (
                "full-nextest",
                "AXON_ALLOW_FALLBACK_WEB_ASSETS=1 cargo nextest run --workspace --locked --lib "
                "-E 'not test(/worker_e2e/)'",
            )
        )

    if full or categories["openapi"] or rust_api_changed or categories["android"] or categories["palette"]:
        plan.append(("openapi-drift", "cargo xtask check-openapi-drift"))

    if android_app_changed:
        plan.append(
            (
                "android",
                "if [ -z \"${AXON_AURORA_ANDROID_PATH:-}\" ]; then "
                "for candidate in ../aurora-design-system/android ../../../aurora-design-system/android "
                "/home/jmagar/workspace/aurora-design-system/android; do "
                "if [ -d \"$candidate\" ]; then export AXON_AURORA_ANDROID_PATH=\"$candidate\"; break; fi; done; fi; "
                "if [ ! -d \"${AXON_AURORA_ANDROID_PATH:-}\" ]; then "
                "echo 'Set AXON_AURORA_ANDROID_PATH to an Aurora Android checkout before running Android validation.' >&2; exit 1; fi; "
                "apps/android/gradlew -p apps/android :app:testDebugUnitTest :app:lintDebug --no-daemon",
            )
        )

    return dedupe_plan(plan)


def dedupe_plan(plan: list[tuple[str, str]]) -> list[tuple[str, str]]:
    seen: set[str] = set()
    out: list[tuple[str, str]] = []
    for name, command in plan:
        if name in seen:
            continue
        seen.add(name)
        out.append((name, command))
    return out


def run_command(name: str, command: str) -> None:
    print(f"\n==> {name}\n{command}", flush=True)
    env = os.environ.copy()
    env.setdefault("AXON_ALLOW_FALLBACK_WEB_ASSETS", "1")
    subprocess.run(["bash", "-lc", command], cwd=ROOT, env=env, check=True)


def write_classifier_output(paths: list[str], categories: dict[str, bool]) -> None:
    print("Changed files:")
    if paths:
        for path in paths:
            print(f"  {path}")
    else:
        print("  <none relative to selected base>")

    enabled = [key for key, value in categories.items() if value]
    print("Enabled categories: " + ", ".join(enabled))


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    full = truthy(os.environ.get("AXON_FULL_PRE_PUSH"))
    base = resolve_base()
    paths = changed_files(base)
    if paths is None and not full:
        print("Could not determine changed files; set AXON_FULL_PRE_PUSH=1 for full validation.", file=sys.stderr)
        full = True
        paths = []
    elif paths is None:
        paths = []

    categories = classify(paths, full)
    plan = command_plan(paths, categories, full)

    write_classifier_output(paths, categories)
    print("Pre-push plan:")
    for name, command in plan:
        print(f"  {name}: {command}")

    if args.dry_run:
        return 0

    with tempfile.TemporaryDirectory(prefix="axon-pre-push-"):
        for name, command in plan:
            run_command(name, command)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except subprocess.CalledProcessError as exc:
        raise SystemExit(exc.returncode)
