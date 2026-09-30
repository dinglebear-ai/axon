#!/usr/bin/env python3
"""Project canonical MCP JSON into Markdown: 0 current/written, 1 drift, 2 bad input."""
from __future__ import annotations
import argparse
import difflib
import sys
from pathlib import Path
from doc_schema import ContractError, load_contract
from mcp_doc_renderer import generate_markdown

INPUT = "docs/reference/mcp/tool-schema.json"
OUTPUT = "docs/reference/mcp/tool-schema.md"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail on drift without writing")
    parser.add_argument("--dry-run", action="store_true", help="print Markdown without writing")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    if args.check and args.dry_run:
        parser.error("--check and --dry-run are mutually exclusive")
    root = args.repo_root.resolve()
    try:
        schema = load_contract(root, INPUT, "cargo xtask schemas mcp")
        generated = generate_markdown(schema)
        path = root / OUTPUT
        existing = path.read_text(encoding="utf-8") if path.exists() else ""
        if args.dry_run:
            sys.stdout.write(generated)
        elif args.check:
            if existing != generated:
                print(f"DRIFT: {OUTPUT}; run cargo xtask generated-contracts refresh", file=sys.stderr)
                sys.stdout.writelines(difflib.unified_diff(existing.splitlines(True), generated.splitlines(True), fromfile=OUTPUT, tofile="generated"))
                return 1
            print(f"OK: {OUTPUT} is up to date")
        elif existing != generated:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(generated, encoding="utf-8")
            print(f"Wrote {OUTPUT} ({len(generated.encode('utf-8'))} bytes)")
        return 0
    except (ContractError, OSError) as error:
        print(f"MCP documentation input error: {error}. No output was written; correct the source before retrying.", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
