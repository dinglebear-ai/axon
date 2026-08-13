#!/usr/bin/env python3
"""Require an explicit build target for every workflow that builds a Dockerfile.

`docker build` with no `--target` selects the LAST stage in the file. When a
Dockerfile ends with a development stage, an untargeted build silently ships
that stage: config/Dockerfile's `dev-runtime` stage carries no binary of its
own and execs a bind-mounted host build, so the published image crash-looped
for anyone who merely pulled it while every build job stayed green.

Pinning `target:` by hand in each workflow does not stop the next workflow from
omitting it, which is exactly how it happened. This check is the gate: any
docker/build-push-action step whose `file:` is a multi-stage Dockerfile must
name the stage it wants.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

WORKFLOW_DIR = Path(".github/workflows")
BUILD_ACTION = "docker/build-push-action"
STAGE_RE = re.compile(r"^\s*FROM\s+.+\s+AS\s+(\S+)", re.IGNORECASE | re.MULTILINE)


def dockerfile_stages(path: Path) -> list[str]:
    try:
        return STAGE_RE.findall(path.read_text(encoding="utf-8"))
    except OSError:
        return []


def step_blocks(text: str) -> list[tuple[int, list[str]]]:
    """Split a workflow into `- uses:`/`- name:` step blocks with 1-based starts.

    Steps are found by indentation rather than parsed as YAML so this stays a
    dependency-free pre-commit check. A step runs until the next line at the
    same indentation that starts a new `- ` item.
    """
    lines = text.splitlines()
    blocks: list[tuple[int, list[str]]] = []
    start: int | None = None
    indent = 0

    for number, line in enumerate(lines, start=1):
        stripped = line.lstrip(" ")
        if stripped.startswith("- "):
            current_indent = len(line) - len(stripped)
            if start is not None and current_indent <= indent:
                blocks.append((start, lines[start - 1 : number - 1]))
                start = None
            if start is None:
                start = number
                indent = current_indent
    if start is not None:
        blocks.append((start, lines[start - 1 :]))
    return blocks


def check_workflow(path: Path, repo_root: Path) -> list[str]:
    failures: list[str] = []
    text = path.read_text(encoding="utf-8")
    if BUILD_ACTION not in text:
        return failures

    for start, block in step_blocks(text):
        body = "\n".join(block)
        if BUILD_ACTION not in body:
            continue

        file_match = re.search(r"^\s*file:\s*(\S+)", body, re.MULTILINE)
        # No `file:` means the default ./Dockerfile; resolve it the same way.
        dockerfile = repo_root / (file_match.group(1) if file_match else "Dockerfile")
        stages = dockerfile_stages(dockerfile)
        if len(stages) < 2:
            continue

        if not re.search(r"^\s*target:\s*(\S+)", body, re.MULTILINE):
            failures.append(
                f"{path}:{start}: {BUILD_ACTION} builds {dockerfile.name} "
                f"({len(stages)} stages, last is '{stages[-1]}') without a "
                f"`target:`. An untargeted build ships '{stages[-1]}'."
            )
            continue

        target = re.search(r"^\s*target:\s*(\S+)", body, re.MULTILINE).group(1)
        if target not in stages:
            failures.append(
                f"{path}:{start}: {BUILD_ACTION} sets `target: {target}`, which "
                f"is not a stage in {dockerfile.name}. Known stages: "
                f"{', '.join(stages)}."
            )

    return failures


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "files",
        nargs="*",
        help="workflow files to check (default: all of .github/workflows)",
    )
    args = parser.parse_args()

    repo_root = Path.cwd()
    if args.files:
        paths = [Path(name) for name in args.files]
    else:
        paths = sorted(WORKFLOW_DIR.glob("*.yml")) + sorted(WORKFLOW_DIR.glob("*.yaml"))

    failures: list[str] = []
    for path in paths:
        if path.is_file():
            failures.extend(check_workflow(path, repo_root))

    if failures:
        print("Dockerfile build steps must pin an explicit stage:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        return 1

    print("OK: every workflow Dockerfile build pins a valid target stage.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
