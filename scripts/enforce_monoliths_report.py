#!/usr/bin/env python3
"""Whole-repo monolith report -- informational, non-blocking.

Walks all checkable source files under REPO_ROOT (respecting EXCLUDED_GLOBS and
the allowlist) and prints any that exceed file/function size limits. Always
exits 0 -- this is for visibility, not enforcement. Existing oversized files
already in `.monolith-allowlist` are skipped, since they're tracked there.

Use ``--include-allowlisted`` to include them in the report (useful when
auditing the allowlist itself).
"""

from __future__ import annotations

import fnmatch
from pathlib import Path

from enforce_monoliths_helpers import (
    CHECKABLE_EXTENSIONS,
    EXCLUDED_GLOBS,
    REPO_ROOT,
    RUST_EXTENSIONS,
    count_effective_lines,
    file_line_count,
    is_text_file,
    load_allowlist,
    parse_rust_functions,
)


# Directories never worth scanning even if not gitignored.
SKIP_DIRS = {
    ".git",
    "target",
    "node_modules",
    ".cache",
    ".venv",
    "dist",
    "build",
    ".next",
    ".worktrees",
}


def _iter_source_files(root: Path) -> list[str]:
    """Yield repo-relative paths to checkable source files."""
    out: list[str] = []
    for path in root.rglob("*"):
        if path.is_dir():
            continue
        # Cheap pruning: skip anything under SKIP_DIRS at any depth.
        rel_parts = path.relative_to(root).parts
        if any(part in SKIP_DIRS for part in rel_parts):
            continue
        if path.suffix not in CHECKABLE_EXTENSIONS:
            continue
        rel = "/".join(rel_parts)
        if any(fnmatch.fnmatch(rel, pattern) for pattern in EXCLUDED_GLOBS):
            continue
        out.append(rel)
    out.sort()
    return out


def whole_repo_report(
    file_max: int,
    fn_warn: int,
    fn_max: int,
    include_allowlisted: bool = False,
) -> int:
    """Print a sorted list of oversized files / overlong functions. Exit 0."""
    allowlist = load_allowlist()
    files = _iter_source_files(REPO_ROOT)

    oversized_files: list[tuple[str, int]] = []
    overlong_fns: list[tuple[str, str, int, int]] = []
    allowlisted_oversized: list[tuple[str, int]] = []

    for rel in files:
        full = REPO_ROOT / rel
        if not is_text_file(full):
            continue

        is_allowlisted = rel in allowlist
        if is_allowlisted and not include_allowlisted:
            # Track separately for the summary line.
            try:
                lc = file_line_count(full)
            except OSError:
                continue
            if lc > file_max:
                allowlisted_oversized.append((rel, lc))
            continue

        try:
            line_count = file_line_count(full)
        except OSError:
            continue
        if line_count > file_max:
            oversized_files.append((rel, line_count))

        if full.suffix in RUST_EXTENSIONS:
            rust_lines = full.read_text(encoding="utf-8", errors="ignore").splitlines()
            for fn in parse_rust_functions(full):
                fn_len = count_effective_lines(
                    rust_lines[fn.start - 1 : fn.end], ".rs"
                )
                if fn_len > fn_warn:
                    overlong_fns.append((rel, fn.name, fn.start, fn_len))

    print("Monolith whole-repo report")
    print(f"  scanned {len(files)} files under {REPO_ROOT}")
    print(
        f"  thresholds: file>{file_max} lines, "
        f"function>{fn_warn} (warn), function>{fn_max} (fail)"
    )
    print(f"  allowlist entries: {len(allowlist)}")
    print(f"  allowlisted-and-still-oversized: {len(allowlisted_oversized)}")
    print()

    if oversized_files:
        print(f"Oversized files (NOT in allowlist) -- {len(oversized_files)}:")
        oversized_files.sort(key=lambda x: -x[1])
        for path, lc in oversized_files:
            print(f"  {lc:>5} lines  {path}")
        print()
    else:
        print("No un-allowlisted oversized files. Nice.")
        print()

    if overlong_fns:
        fail_fns = [f for f in overlong_fns if f[3] > fn_max]
        warn_fns = [f for f in overlong_fns if f[3] <= fn_max]
        if fail_fns:
            print(f"Functions over hard limit ({fn_max} lines) -- {len(fail_fns)}:")
            fail_fns.sort(key=lambda x: -x[3])
            for path, name, start, fn_len in fail_fns:
                print(f"  {fn_len:>4} lines  {path}:{start} {name}()")
            print()
        if warn_fns:
            print(
                f"Functions in warn band ({fn_warn}<n<={fn_max} lines) -- "
                f"{len(warn_fns)}:"
            )
            warn_fns.sort(key=lambda x: -x[3])
            for path, name, start, fn_len in warn_fns:
                print(f"  {fn_len:>4} lines  {path}:{start} {name}()")
            print()

    if include_allowlisted and allowlisted_oversized:
        print(f"Allowlisted oversized files -- {len(allowlisted_oversized)}:")
        allowlisted_oversized.sort(key=lambda x: -x[1])
        for path, lc in allowlisted_oversized:
            print(f"  {lc:>5} lines  {path}")
        print()

    print("Report complete (informational, exit 0).")
    return 0
