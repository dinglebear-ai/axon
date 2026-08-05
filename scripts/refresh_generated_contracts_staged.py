#!/usr/bin/env python3
"""Refresh deterministic generated contracts before a commit is created.

The contract artifacts include provenance hashes of implementation inputs. This
hook keeps those artifacts in the same commit as their source change while
failing closed around partial staging and unexpected generator output.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GENERATOR_BINARY = ROOT / "target" / "debug" / ("xtask.exe" if os.name == "nt" else "xtask")

GENERATED_PREFIXES = (
    "docs/reference/",
    "xtask/tests/fixtures/schemas/",
    "crates/axon-mcp/tests/golden/",
)

CONTRACT_DOC_PREFIXES = (
    "docs/pipeline-unification/configuration/",
    "docs/pipeline-unification/runtime/",
    "docs/pipeline-unification/schemas/",
    "docs/pipeline-unification/sources/",
)


def normalized(path: str) -> str:
    return path.replace("\\", "/").lstrip("./")


def is_generated_output(path: str) -> bool:
    path = normalized(path)
    return path.startswith(GENERATED_PREFIXES)


def affects_generated_contracts(path: str) -> bool:
    path = normalized(path)
    if is_generated_output(path):
        return False
    if path == "Cargo.toml" or path.endswith("/Cargo.toml"):
        return True
    if path.startswith(CONTRACT_DOC_PREFIXES):
        return True
    if path.endswith(".rs") and path.startswith(("src/", "crates/", "xtask/src/schemas/")):
        return True
    if path.startswith("crates/") and ("/src/migrations/" in path or "/fixtures/" in path):
        return True
    return False


def git_prefix() -> list[str]:
    dot_git = ROOT / ".git"
    if dot_git.is_file():
        line = dot_git.read_text().strip()
        prefix = "gitdir: "
        if not line.startswith(prefix):
            raise RuntimeError(f"unexpected worktree metadata in {dot_git}")
        git_dir = Path(line[len(prefix) :])
        if not git_dir.is_absolute():
            git_dir = (ROOT / git_dir).resolve()
    elif dot_git.is_dir():
        git_dir = dot_git
    else:
        raise RuntimeError(f"{ROOT} is not a Git worktree")
    return ["git", f"--git-dir={git_dir}", f"--work-tree={ROOT}"]


def run(command: list[str], *, capture: bool = False) -> subprocess.CompletedProcess[str]:
    return subprocess.run(
        command,
        cwd=ROOT,
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )


def git_paths(*args: str) -> set[str]:
    completed = run([*git_prefix(), *args, "-z"], capture=True)
    return {normalized(path) for path in completed.stdout.split("\0") if path}


def staged_paths() -> set[str]:
    return git_paths("diff", "--cached", "--name-only", "--diff-filter=ACMRD")


def working_tree_paths() -> set[str]:
    tracked = git_paths("diff", "--name-only", "--diff-filter=ACMRD")
    untracked = git_paths("ls-files", "--others", "--exclude-standard")
    return tracked | untracked


def build_generator() -> None:
    run(
        [
            "cargo",
            "build",
            "--locked",
            "--manifest-path",
            "xtask/Cargo.toml",
            "--no-default-features",
        ]
    )
    if not GENERATOR_BINARY.is_file():
        raise RuntimeError(f"generated-contract builder did not create {GENERATOR_BINARY}")


def generator(action: str) -> None:
    run([str(GENERATOR_BINARY), "generated-contracts", action])


def fail_paths(message: str, paths: set[str]) -> int:
    print(f"ERROR: {message}", file=sys.stderr)
    for path in sorted(paths):
        print(f"  - {path}", file=sys.stderr)
    return 1


def refresh_staged_contracts() -> int:
    staged = staged_paths()
    staged_inputs = {path for path in staged if affects_generated_contracts(path)}
    staged_outputs = {path for path in staged if is_generated_output(path)}
    if not staged_inputs and not staged_outputs:
        return 0

    baseline_dirty = working_tree_paths()
    unstaged_inputs = {path for path in baseline_dirty if affects_generated_contracts(path)}
    if unstaged_inputs:
        return fail_paths(
            "generated-contract inputs have unstaged changes; stage or stash them before committing",
            unstaged_inputs,
        )
    unstaged_outputs = {path for path in baseline_dirty if is_generated_output(path)}
    if unstaged_outputs:
        return fail_paths(
            "generated-contract outputs were already dirty; stage or restore them before committing",
            unstaged_outputs,
        )

    build_generator()
    generator("refresh")
    after_refresh = working_tree_paths()
    generated_delta = after_refresh - baseline_dirty
    unexpected = {path for path in generated_delta if not is_generated_output(path)}
    if unexpected:
        return fail_paths(
            "generated-contract refresh changed paths outside the generated-output allowlist",
            unexpected,
        )

    generated_delta = {path for path in generated_delta if is_generated_output(path)}
    if generated_delta:
        run([*git_prefix(), "add", "-A", "--", *sorted(generated_delta)])

    # A second pass must not create any working-tree delta. This catches an
    # unstable generator before it can create an endless commit loop.
    generator("refresh")
    second_pass_dirty = working_tree_paths()
    if second_pass_dirty != baseline_dirty:
        return fail_paths(
            "generated-contract refresh is not idempotent on the staged inputs",
            second_pass_dirty.symmetric_difference(baseline_dirty),
        )

    run([*git_prefix(), "diff", "--cached", "--check"])
    generator("check")

    if generated_delta:
        print(
            f"Generated contracts refreshed and staged ({len(generated_delta)} file(s))."
        )
    return 0


def self_test() -> int:
    relevant = {
        "Cargo.toml",
        "crates/axon-adapters/Cargo.toml",
        "src/web/map.rs",
        "crates/axon-adapters/src/web.rs",
        "xtask/src/schemas/adapters.rs",
        "crates/axon-memory/src/migrations/001.sql",
        "crates/axon-adapters/fixtures/provider-variant-exceptions.json",
        "docs/pipeline-unification/sources/adapter-scopes.md",
    }
    irrelevant = {
        "README.md",
        "apps/web/src/app.tsx",
        "docs/guides/operators.md",
        "scripts/refresh_generated_contracts_staged.py",
    }
    generated = {
        "docs/reference/sources/adapter-scopes.json",
        "xtask/tests/fixtures/schemas/adapters/snapshots/adapter-scopes.json",
        "crates/axon-mcp/tests/golden/tool-schema.json",
    }

    assert all(affects_generated_contracts(path) for path in relevant)
    assert not any(affects_generated_contracts(path) for path in irrelevant)
    assert all(is_generated_output(path) for path in generated)
    assert not any(affects_generated_contracts(path) for path in generated)
    print("generated-contract staged-path classifier: OK")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--self-test",
        action="store_true",
        help="validate path classification without reading or modifying Git state",
    )
    args = parser.parse_args()
    try:
        return self_test() if args.self_test else refresh_staged_contracts()
    except subprocess.CalledProcessError as error:
        return error.returncode or 1
    except (OSError, RuntimeError) as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
