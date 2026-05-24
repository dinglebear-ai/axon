#!/usr/bin/env python3
"""Fail if lefthook.yml's pre-commit stage has any workspace-scale command.

Pre-commit must stay fast (target <30s) so we keep the "commit early, commit
often" habit. Heavy gates (full workspace clippy / test / build) belong in
pre-push or CI, not pre-commit.

This script is run by lefthook's own pre-commit (gated on lefthook.yml
changes) and by GitHub Actions on every push, so any regression of the
pre-commit shape is caught before it lands.

The full list of forbidden substrings lives in `FORBIDDEN_SUBSTRINGS`
below — keep that as the single source of truth, not this docstring.

If you need to add a new pre-commit check, scope it to staged files only
(via lefthook's `glob:` + `{staged_files}` substitution) or move the
expensive variant to pre-push.

Exits 0 on success, 1 with a list of violations on failure.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

FORBIDDEN_SUBSTRINGS = (
    "cargo clippy --workspace",
    "cargo clippy --all",
    "cargo test --workspace",
    "cargo nextest run --workspace",
    "cargo build --workspace",
    "cargo check --workspace",
    "cargo nextest run --all",
    "pnpm -r test",
    "npm run test",
    "pytest",
)


def parse_pre_commit_runs(yaml_text: str) -> list[tuple[str, str]]:
    """Return [(command_name, run_block)] for every command under pre-commit:.

    Hand-rolled to avoid a PyYAML dependency. lefthook.yml's structure is
    stable enough to scan line-by-line: 2-space indent under `pre-commit:`
    starts the commands map, 4-space indent names each command, 6-space
    indent holds the command's fields including `run:`.
    """
    lines = yaml_text.splitlines()
    out: list[tuple[str, str]] = []
    in_pre_commit = False
    in_commands = False
    current_name: str | None = None
    current_run: list[str] = []
    current_run_active = False

    def flush() -> None:
        nonlocal current_run, current_run_active
        if current_name is not None and current_run:
            out.append((current_name, "\n".join(current_run).strip()))
        current_run = []
        current_run_active = False

    for raw in lines:
        line = raw.rstrip()

        # New top-level stage block ends pre-commit scanning.
        if re.match(r"^[A-Za-z][\w-]*:\s*$", line) and not line.startswith(" "):
            flush()
            in_pre_commit = line.startswith("pre-commit:")
            in_commands = False
            current_name = None
            continue

        if not in_pre_commit:
            continue

        # `  commands:` opens the commands map.
        if re.match(r"^  commands:\s*$", line):
            in_commands = True
            continue

        if not in_commands:
            continue

        # `    foo:` names a new command — flush previous, start fresh.
        m = re.match(r"^    ([A-Za-z][\w-]*):\s*$", line)
        if m:
            flush()
            current_name = m.group(1)
            continue

        # `      run: …` opens a run block. Capture inline value if present.
        m = re.match(r"^      run:\s*(.*)$", line)
        if m:
            current_run_active = True
            value = m.group(1).strip()
            if value and value not in {">", "|"}:
                current_run.append(value)
            continue

        # Continuation of a folded/literal run block (`>` or `|`).
        if current_run_active and line.startswith("        "):
            current_run.append(line.strip())
            continue

        # Any other 6-space-indented key ends the run block but stays in the
        # same command.
        if current_run_active and re.match(r"^      [A-Za-z]", line):
            current_run_active = False

    flush()
    return out


def find_violations(commands: list[tuple[str, str]]) -> list[tuple[str, str, str]]:
    violations: list[tuple[str, str, str]] = []
    for name, run in commands:
        # YAML folded scalars (`run: >`) collapse newlines to spaces at
        # runtime, but our parser stores continuation lines joined with
        # `\n`. Normalize whitespace BEFORE the substring check so a
        # multi-line `cargo test\n--workspace` doesn't sneak past a
        # `"cargo test --workspace"` needle.
        normalized = re.sub(r"\s+", " ", run.lower()).strip()
        for needle in FORBIDDEN_SUBSTRINGS:
            if needle in normalized:
                violations.append((name, needle, run))
                break
    return violations


def main(argv: list[str]) -> int:
    path = Path(argv[1]) if len(argv) > 1 else Path("lefthook.yml")
    if not path.exists():
        print(f"ERROR: {path} not found", file=sys.stderr)
        return 1

    commands = parse_pre_commit_runs(path.read_text())

    # Sentinel-floor self-check. The parser is hand-rolled (no PyYAML
    # dependency) and assumes lefthook.yml's current indentation shape.
    # If the file is restructured (different indent, anchors, extends,
    # new run-block synonyms) the parser may yield nothing and
    # find_violations would happily return [] — silently disabling the
    # speed gate. Require the parser to find at least one well-known
    # command from this repo; if not, the YAML changed shape and the
    # parser needs updating, not the check.
    expected_sentinels = {"rustfmt", "lefthook-speed"}
    found_names = {name for name, _ in commands}
    missing = expected_sentinels - found_names
    if missing:
        print(
            f"ERROR: parser found only {len(commands)} pre-commit commands and "
            f"is missing well-known sentinel(s): {sorted(missing)}\n"
            f"       {path} likely changed shape — update "
            f"parse_pre_commit_runs() in this script to match.\n"
            f"       Found commands: {sorted(found_names)}",
            file=sys.stderr,
        )
        return 1

    violations = find_violations(commands)

    if violations:
        print(
            f"ERROR: {path}'s pre-commit stage has workspace-scale commands.\n"
            "       Pre-commit must stay fast (target <30s). Move heavy gates\n"
            "       to pre-push or CI, or scope them to staged files via\n"
            "       lefthook's glob: + {staged_files} substitution.\n",
            file=sys.stderr,
        )
        for name, needle, run in violations:
            print(f"  - {name}: matches forbidden pattern {needle!r}", file=sys.stderr)
            print(f"      run: {run}", file=sys.stderr)
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
