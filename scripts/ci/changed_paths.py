#!/usr/bin/env python3
"""Classify changed files into Axon CI routing categories."""

from __future__ import annotations

import argparse
import os
import subprocess
from collections.abc import Callable
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


def any_match(paths: list[str], predicate: Callable[[str], bool]) -> bool:
    return any(predicate(path) for path in paths)


RUST_CI_HELPER_SCRIPTS = {
    "scripts/cargo_test_filter_guard.py",
    "scripts/check_lefthook_pre_commit_speed.py",
    "scripts/check_shell_completions.sh",
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


def classify(event: str, paths: list[str]) -> dict[str, bool]:
    if event in {"schedule", "workflow_dispatch"}:
        return {key: True for key in OUTPUT_KEYS}

    if not paths:
        return {key: True for key in OUTPUT_KEYS}

    workflow = any_match(
        paths,
        lambda p: starts(p, ".github/workflows/")
        or p in {"scripts/ci/changed_paths.py", "tests/workflow_shapes.rs", "tests/ci_changed_paths.rs"},
    )
    docs = any_match(
        paths,
        lambda p: starts(p, "docs/") or p in {"README.md", "CHANGELOG.md"} or p in DOC_CI_HELPER_SCRIPTS,
    )
    openapi = any_match(paths, lambda p: starts(p, "apps/web/openapi/"))
    web = any_match(paths, lambda p: starts(p, "apps/web/", "assets/")) or openapi
    android = any_match(paths, lambda p: starts(p, "apps/android/")) or openapi
    palette = any_match(paths, lambda p: starts(p, "apps/palette-tauri/")) or openapi
    chrome = any_match(paths, lambda p: starts(p, "apps/chrome-extension/", "assets/"))
    mcp = any_match(
        paths,
        lambda p: starts(p, "src/mcp/", "docs/reference/mcp/")
        or p in MCP_CI_HELPER_SCRIPTS
        or p == "tests/workflow_shapes.rs",
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
        or p in {"Cargo.toml", "Cargo.lock", "build.rs", "rust-toolchain.toml", "Justfile"}
        or p in RUST_CI_HELPER_SCRIPTS,
    )
    release = rust or web or any_match(paths, lambda p: starts(p, "release/"))
    compose = any_match(
        paths,
        lambda p: starts(p, "config/", "scripts/")
        or p
        in {".dockerignore", ".env.example", "docker-compose.yaml", "docker-compose.prod.yaml", "docker-compose.llama.yaml"},
    )
    docker = rust or web or compose or any_match(paths, lambda p: p in {".dockerignore", "config/Dockerfile"})
    security = any_match(paths, lambda p: p in {"Cargo.lock", "deny.toml"} or starts(p, ".cargo/", "vendor/")) or rust

    codeql_actions = workflow
    codeql_javascript_typescript = web or palette or any_match(
        paths, lambda p: p.endswith((".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs"))
    )
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

    paths = read_paths(args.changed_files) if args.changed_files else resolve_paths(args.event)
    if args.write_changed_files:
        args.write_changed_files.write_text("\n".join(paths) + ("\n" if paths else ""))

    values = classify(args.event, paths)
    write_outputs(args.output, values)
    for key in OUTPUT_KEYS:
        print(f"{key}={str(values[key]).lower()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
