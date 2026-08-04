#!/usr/bin/env python3
"""Classify changed files into Axon CI routing categories."""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from collections.abc import Callable
from pathlib import Path


OUTPUT_KEYS = [
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
]


def starts(path: str, *prefixes: str) -> bool:
    return any(path == prefix.rstrip("/") or path.startswith(prefix) for prefix in prefixes)


def any_match(paths: list[str], predicate: Callable[[str], bool]) -> bool:
    return any(predicate(path) for path in paths)


RUST_CI_HELPER_SCRIPTS = {
    "scripts/cargo_test_filter_guard.py",
    "scripts/check_lefthook_pre_commit_speed.py",
    "scripts/check_shell_completions.sh",
    "scripts/refresh_generated_contracts_staged.py",
    "xtask/src/pre_push.rs",
    "scripts/enforce_monoliths.py",
    "scripts/generate_mcp_schema_doc.py",
    "scripts/test-ask-quality-regressions.sh",
    "scripts/test-mcp-oauth-protection.sh",
    "scripts/test-mcp-tools-mcporter.sh",
}

MCP_CI_HELPER_SCRIPTS = {
    "scripts/generate_mcp_schema_doc.py",
    "scripts/test-mcp-oauth-protection.sh",
    "scripts/test-mcp-tools-mcporter.sh",
}

DOC_CI_HELPER_SCRIPTS = {
    "scripts/check_aurora_primitive_inventory.py",
}

FULL_CI_ROUTER_PATHS = {
    "scripts/ci/changed_paths.py",
    "tests/ci_changed_paths.rs",
    "tests/workflow_shapes.rs",
    "xtask/src/pre_push.rs",
}

WORKFLOW_CATEGORY_PATHS = {
    "android": {".github/workflows/android-release.yml"},
    "palette": {".github/workflows/palette-release.yml"},
    "chrome": {".github/workflows/chrome-extension-release.yml"},
    "compose": {".github/workflows/compose-smoke.yml"},
    "docker": {".github/workflows/docker-image.yml"},
}

COMPOSE_INPUTS = {
    ".env.example",
    "docker-compose.yaml",
    "docker-compose.prod.yaml",
    "docker-compose.external-providers.yaml",
    "docker-compose.external-qdrant.yaml",
    "docker-compose.llama.yaml",
    "scripts/build-on-steamy.sh",
    "scripts/test-ask-gemma4.sh",
    "scripts/test-build-on-steamy-safety.sh",
}

VERSION_FILES = {
    "Cargo.toml",
    "Cargo.lock",
    "README.md",
    "CHANGELOG.md",
    "apps/web/package.json",
    "apps/web/package-lock.json",
    "apps/web/openapi/axon.json",
    "apps/palette-tauri/src-tauri/tauri.conf.json",
    "apps/palette-tauri/package.json",
    "apps/palette-tauri/src-tauri/Cargo.toml",
    "apps/palette-tauri/src-tauri/Cargo.lock",
    "apps/palette-tauri/CHANGELOG.md",
    "apps/android/app/build.gradle.kts",
    "apps/android/CHANGELOG.md",
    "apps/chrome-extension/manifest.json",
    "apps/chrome-extension/package.json",
    "apps/chrome-extension/CHANGELOG.md",
}


def classify(event: str, paths: list[str]) -> dict[str, bool]:
    if event in {"schedule", "workflow_dispatch"}:
        result = {key: True for key in OUTPUT_KEYS}
        result["routing_fallback"] = False
        return result

    if not paths:
        result = {key: True for key in OUTPUT_KEYS}
        result["routing_fallback"] = True
        return result

    workflow = any_match(
        paths,
        lambda p: starts(p, ".github/workflows/") or p in FULL_CI_ROUTER_PATHS,
    )
    full_ci = any_match(paths, lambda p: p in FULL_CI_ROUTER_PATHS)
    ci_all = any_match(paths, lambda p: p == ".github/workflows/ci.yml")
    codeql_all = any_match(paths, lambda p: p == ".github/workflows/codeql.yml")
    docs = any_match(
        paths,
        lambda p: starts(p, "docs/", "openwiki/")
        or p in {"README.md", "CHANGELOG.md"}
        or p in DOC_CI_HELPER_SCRIPTS,
    )
    docs_contracts = any_match(
        paths,
        lambda p: starts(p, "docs/reference/")
        or p in DOC_CI_HELPER_SCRIPTS,
    )
    aurora_inventory = any_match(
        paths,
        lambda p: p
        in {
            "docs/reference/aurora-primitive-inventory.json",
            "scripts/check_aurora_primitive_inventory.py",
        },
    )
    # Release builds are reserved for component version changes, explicit
    # release configuration, main, scheduled runs, or a full-CI PR label.
    version_files = any_match(paths, lambda p: p in VERSION_FILES)
    openapi = any_match(paths, lambda p: starts(p, "apps/web/openapi/"))
    web = any_match(paths, lambda p: starts(p, "apps/web/", "assets/")) or openapi
    android = any_match(
        paths,
        lambda p: starts(p, "apps/android/") or p in WORKFLOW_CATEGORY_PATHS["android"],
    ) or openapi
    palette = any_match(
        paths,
        lambda p: starts(p, "apps/palette-tauri/") or p in WORKFLOW_CATEGORY_PATHS["palette"],
    ) or openapi
    chrome = any_match(
        paths,
        lambda p: starts(p, "apps/chrome-extension/", "assets/")
        or p in WORKFLOW_CATEGORY_PATHS["chrome"],
    )
    mcp = any_match(
        paths,
        lambda p: starts(
            p,
            "src/mcp/",
            "crates/axon-mcp/",
            "crates/axon-api/src/mcp_schema/",
            "docs/reference/mcp/",
        )
        or p == "crates/axon-api/src/mcp_schema.rs"
        or p in MCP_CI_HELPER_SCRIPTS
        or p == "tests/workflow_shapes.rs",
    )
    rust = any_match(
        paths,
        lambda p: starts(
            p,
            "src/",
            "crates/",
            "xtask/",
            "benches/",
            "tests/",
            "migrations/",
            "vendor/",
            ".cargo/",
            ".config/",
        )
        or p in {"Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "Justfile"}
        or p in RUST_CI_HELPER_SCRIPTS,
    )
    release = version_files or any_match(
        paths,
        lambda p: starts(p, "release/")
        or p in {"release-please-config.json", ".release-please-manifest.json"},
    )
    compose = any_match(
        paths,
        lambda p: p in COMPOSE_INPUTS
        or starts(p, "config/chrome/")
        or p in WORKFLOW_CATEGORY_PATHS["compose"],
    )
    docker = any_match(
        paths,
        lambda p: p
        in {".dockerignore", "Cargo.toml", "Cargo.lock", "rust-toolchain.toml"}
        or p.endswith("/Cargo.toml")
        or p.endswith("/build.rs")
        or p == "build.rs"
        or starts(p, "config/Dockerfile")
        or p in WORKFLOW_CATEGORY_PATHS["docker"],
    )
    security = any_match(
        paths,
        lambda p: p in {"Cargo.lock", "deny.toml"}
        or starts(p, ".cargo/", "vendor/"),
    ) or rust

    codeql_actions = workflow
    codeql_javascript_typescript = web or palette or any_match(
        paths, lambda p: p.endswith((".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"))
    )
    # Python CodeQL only analyzes .py sources; `scripts/` is mostly shell, so
    # gating on the prefix triggered a full Python analyze on unrelated changes.
    codeql_python = any_match(paths, lambda p: p.endswith(".py"))
    codeql_rust = rust or palette
    codeql_java_kotlin = android or any_match(
        paths, lambda p: p.endswith((".java", ".kt", ".kts"))
    )

    if codeql_all or full_ci:
        codeql_actions = True
        codeql_javascript_typescript = True
        codeql_python = True
        codeql_rust = True
        codeql_java_kotlin = True

    result = {
        "all": False,
        "routing_fallback": False,
        "full_ci": full_ci,
        "ci_all": ci_all,
        "codeql_all": codeql_all,
        "docs": docs,
        "docs_contracts": docs_contracts,
        "aurora_inventory": aurora_inventory,
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
        "version_files": version_files,
        "openapi": openapi,
        "codeql_actions": codeql_actions,
        "codeql_javascript_typescript": codeql_javascript_typescript,
        "codeql_python": codeql_python,
        "codeql_rust": codeql_rust,
        "codeql_java_kotlin": codeql_java_kotlin,
    }

    return result


def read_paths(path: Path) -> list[str]:
    if not path.exists():
        return []
    return [line.strip() for line in path.read_text().splitlines() if line.strip()]


def git_path_exists(rev: str) -> bool:
    return subprocess.run(
        ["git", "cat-file", "-e", rev],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
        check=False,
    ).returncode == 0


def git_output(*args: str) -> str:
    return subprocess.check_output(["git", *args], text=True, stderr=subprocess.DEVNULL).strip()


def resolve_paths(event: str) -> list[str]:
    if event in {"schedule", "workflow_dispatch"}:
        return []

    env = os.environ
    base = ""
    head = env.get("HEAD_SHA") or env.get("GITHUB_SHA") or "HEAD"

    if event == "pull_request":
        base = env.get("PR_BASE_SHA", "")
        head = env.get("PR_HEAD_SHA") or head
    elif event == "push":
        if env.get("GITHUB_REF", "").startswith("refs/tags/"):
            return []
        base = env.get("PUSH_BEFORE_SHA", "")
    else:
        return []

    if not base or set(base) == {"0"} or not git_path_exists(base):
        try:
            base = git_output("rev-parse", "HEAD^")
        except subprocess.CalledProcessError:
            base = ""

    if not base:
        return []

    try:
        raw = git_output("diff", "--name-only", base, head)
    except subprocess.CalledProcessError:
        return []

    return [line.strip() for line in raw.splitlines() if line.strip()]


def write_outputs(path: Path, values: dict[str, bool]) -> None:
    lines = [f"{key}={'true' if values[key] else 'false'}" for key in OUTPUT_KEYS]
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--event", required=True)
    parser.add_argument("--changed-files", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--write-changed-files", type=Path)
    args = parser.parse_args()

    paths = (
        read_paths(args.changed_files)
        if args.changed_files
        else resolve_paths(args.event)
    )
    if not paths and args.event not in {"schedule", "workflow_dispatch"}:
        print(
            "::warning::changed-path resolution returned no files; enabling conservative full CI",
            file=sys.stderr,
        )
    if args.write_changed_files:
        args.write_changed_files.write_text("\n".join(paths) + ("\n" if paths else ""))

    values = classify(args.event, paths)
    write_outputs(args.output, values)
    for key in OUTPUT_KEYS:
        print(f"{key}={str(values[key]).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
