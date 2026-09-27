#!/usr/bin/env python3
"""Run the pinned fleet contract with Axon's canonical AGENTS.md policy.

Only the legacy symlink-direction check is replaced. All other upstream
checks execute unchanged, and every finding still fails the contract.
"""
from __future__ import annotations

import argparse
import importlib.util
from pathlib import Path, PurePosixPath
import subprocess
import sys
from types import ModuleType
from typing import Any, Callable


def git(repo: Path, *args: str) -> str:
    return subprocess.check_output(["git", "-C", str(repo), *args], text=True)


def check_agent_symlinks(repo: Path, finding: Callable[..., Any]) -> list[Any]:
    """Validate tracked index modes and blobs, not just filesystem aliases."""
    findings = []
    entries: dict[str, tuple[str, str]] = {}
    for record in git(repo, "ls-files", "-s", "-z").split("\0"):
        if not record:
            continue
        metadata, path = record.split("\t", 1)
        mode, blob, stage = metadata.split()
        if stage != "0":
            findings.append(finding("symlink-convention", path, "unmerged index entry"))
        entries[path] = (mode, blob)

    names = {"AGENTS.md", "CLAUDE.md", "GEMINI.md"}
    directories = {PurePosixPath(".")} | {
        PurePosixPath(path).parent
        for path in entries
        if PurePosixPath(path).name in names
    }
    blobs: dict[str, str] = {}
    for directory in sorted(directories):
        canonical = (directory / "AGENTS.md").as_posix()
        if entries.get(canonical, (None, None))[0] not in {"100644", "100755"}:
            findings.append(finding(
                "symlink-convention", canonical, "must be a tracked regular canonical file"
            ))
        for name in ("CLAUDE.md", "GEMINI.md"):
            alias = (directory / name).as_posix()
            mode, blob = entries.get(alias, (None, None))
            if mode != "120000":
                findings.append(finding(
                    "symlink-convention", alias, "must be a tracked symlink to AGENTS.md"
                ))
                continue
            if blob not in blobs:
                blobs[blob] = git(repo, "cat-file", "-p", blob)
            if blobs[blob] != "AGENTS.md":
                findings.append(finding(
                    "symlink-convention", alias, "must target AGENTS.md directly and relatively"
                ))
    return findings


def load_fleet(path: Path) -> ModuleType:
    name = "_axon_pinned_fleet_contract"
    spec = importlib.util.spec_from_file_location(name, path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load fleet implementation: {path}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    spec.loader.exec_module(module)
    for attribute in ("check", "check_symlinks", "Finding"):
        if not callable(getattr(module, attribute, None)):
            raise RuntimeError(f"unsupported fleet implementation: missing {attribute}")
    return module


def check(repo: Path, profile: str, fleet: ModuleType) -> list[Any]:
    original = fleet.check_symlinks
    fleet.check_symlinks = lambda root: check_agent_symlinks(root, fleet.Finding)
    try:
        # Do not filter findings or copy the upstream check list: additions to
        # that list must continue to run automatically after a reviewed pin bump.
        return fleet.check(repo, profile)
    finally:
        fleet.check_symlinks = original


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--implementation", type=Path, required=True)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--profile", choices=("rust", "python", "node", "go", "ops"), required=True)
    args = parser.parse_args()
    findings = check(args.repo.resolve(), args.profile, load_fleet(args.implementation.resolve()))
    for finding in findings:
        print(finding.render())
    if findings:
        return 1
    print(f"fleet contract valid with canonical AGENTS.md: {args.repo.resolve()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
