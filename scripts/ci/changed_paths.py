#!/usr/bin/env python3
"""Classify changed files into Axon CI routing categories."""

from __future__ import annotations

import argparse
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
