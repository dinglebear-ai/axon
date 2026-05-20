#!/usr/bin/env python3
"""Generate docs/MCP-TOOL-SCHEMA.md from src/mcp/schema.rs.

Parses the Rust source for struct/enum definitions across the schema module and
produces a markdown document that stays in sync with the actual wire contract.
Run with --check in CI to detect drift.

Exit codes:
    0 — success (or --check passed)
    1 — --check detected a diff
    2 — parse error or missing source file
"""

from __future__ import annotations

import argparse
import difflib
import re
import sys
from pathlib import Path

from mcp_doc_renderer import generate_markdown
from mcp_schema_parser import parse_schema, validate_parsed


# ---------------------------------------------------------------------------
# Repo root detection
# ---------------------------------------------------------------------------


def find_repo_root(start: Path | None = None) -> Path | None:
    """Walk up from start looking for the axon_rust repo root."""
    current = (start or Path.cwd()).resolve()
    for directory in [current, *current.parents]:
        if (directory / ".git").is_dir():
            return directory
        cargo = directory / "Cargo.toml"
        if cargo.is_file() and 'name = "axon"' in cargo.read_text(encoding="utf-8"):
            return directory
    return None


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate docs/MCP-TOOL-SCHEMA.md from src/mcp/schema.rs",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="Compare generated output against existing file; exit 1 on diff",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print generated markdown to stdout without writing",
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=None,
        help="Override repo root detection",
    )
    args = parser.parse_args()

    # Resolve paths
    repo_root = args.repo_root or find_repo_root()
    if repo_root is None:
        print("ERROR: Could not find repo root. Pass --repo-root.", file=sys.stderr)
        return 2

    schema_path = repo_root / "src" / "mcp" / "schema.rs"
    doc_path = repo_root / "docs" / "MCP-TOOL-SCHEMA.md"

    if not schema_path.is_file():
        print(f"ERROR: Schema file not found: {schema_path}", file=sys.stderr)
        return 2

    # Parse. The schema module keeps AxonRequest in schema.rs and request models
    # in child files to stay under the Rust file-size policy, so concatenate the
    # module sources before feeding the lightweight parser.
    source = read_schema_sources(schema_path)
    structs, enums = parse_schema(source)

    errors = validate_parsed(structs, enums)
    if errors:
        for err in errors:
            print(f"PARSE ERROR: {err}", file=sys.stderr)
        return 2

    # Generate
    generated = generate_markdown(structs, enums)

    # --dry-run: print and exit
    if args.dry_run:
        print(generated)
        return 0

    # --check: compare against existing
    if args.check:
        if not doc_path.is_file():
            print(f"ERROR: Doc file not found for --check: {doc_path}", file=sys.stderr)
            return 1

        existing = doc_path.read_text(encoding="utf-8")
        # Normalize: ignore the date line for comparison (it changes daily)
        existing_normalized = _normalize_for_check(existing)
        generated_normalized = _normalize_for_check(generated)

        if existing_normalized == generated_normalized:
            print(f"OK: {doc_path.relative_to(repo_root)} is up to date")
            return 0

        diff = difflib.unified_diff(
            existing_normalized.splitlines(keepends=True),
            generated_normalized.splitlines(keepends=True),
            fromfile=str(doc_path.relative_to(repo_root)),
            tofile="(generated)",
        )
        print(f"DRIFT DETECTED in {doc_path.relative_to(repo_root)}:")
        sys.stdout.writelines(diff)
        return 1

    # Write
    doc_path.parent.mkdir(parents=True, exist_ok=True)
    doc_path.write_text(generated, encoding="utf-8")
    print(f"Wrote {doc_path.relative_to(repo_root)} ({len(generated)} bytes)")
    return 0


def read_schema_sources(schema_path: Path) -> str:
    schema_dir = schema_path.with_suffix("")
    parts = [schema_path.read_text(encoding="utf-8")]
    if schema_dir.is_dir():
        for child in sorted(schema_dir.glob("*.rs")):
            parts.append(child.read_text(encoding="utf-8"))
    return "\n\n".join(parts)


def _normalize_for_check(text: str) -> str:
    """Strip the date line so daily regeneration does not cause false diffs."""
    return re.sub(
        r"^Last Modified: .*$", "Last Modified: <date>", text, flags=re.MULTILINE
    )


if __name__ == "__main__":
    sys.exit(main())
