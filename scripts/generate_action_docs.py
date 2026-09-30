#!/usr/bin/env python3
"""Render action navigation and marked surface blocks from canonical JSON.

The handwritten action body is never regenerated. CLI, MCP, and REST presence
come from their owning registries, not a Markdown table or guessed command name.
"""
from __future__ import annotations
import argparse
import dataclasses
import re
import sys
from pathlib import Path
from doc_schema import ContractError, cell, load_contract, operations

BEGIN = "<!-- BEGIN GENERATED ACTION SURFACES -->"
END = "<!-- END GENERATED ACTION SURFACES -->"
T = chr(96)
# These are navigation joins, not an assertion that every suboperation has parity.
REST_FAMILIES = {"memory": "memories", "watch": "watches", "source": "sources"}
SOURCE_PAGES = {"github", "reddit", "youtube"}


@dataclasses.dataclass(frozen=True)
class Surface:
    name: str
    cli: tuple[str, ...]
    mcp: tuple[str, ...]
    rest: tuple[str, ...]
    notes: str = ""


def canonical_name(name: str) -> str:
    return name.replace("-", "_")


def read_surfaces(root: Path) -> dict[str, Surface]:
    cli = load_contract(root, "docs/reference/cli/commands.json", "cargo xtask schemas cli")
    mcp = load_contract(root, "docs/reference/mcp/tool-schema.json", "cargo xtask schemas mcp")
    rest = load_contract(root, "docs/reference/rest/openapi.json", "cargo xtask schemas openapi")
    commands = cli.get("commands")
    routes = rest.get("routes")
    if not isinstance(commands, list) or not commands or not isinstance(routes, list) or not routes:
        raise ContractError("CLI commands and REST routes must be nonempty registry arrays")
    cli_groups: dict[str, set[str]] = {}
    rest_groups: dict[str, set[str]] = {}
    mcp_groups: dict[str, set[str]] = {}
    for command in commands:
        path = command.get("path") if isinstance(command, dict) else None
        if not isinstance(path, list) or not path or not all(isinstance(p, str) and p for p in path):
            raise ContractError("CLI registry entry has no valid command path")
        cli_groups.setdefault(canonical_name(path[0]), set()).add("axon " + " ".join(path))
    for route in routes:
        if not isinstance(route, dict) or not all(isinstance(route.get(k), str) for k in ("path", "method")):
            raise ContractError("REST registry entry must contain path and method")
        if not route["path"].startswith("/v1/"):
            continue
        segment = canonical_name(route["path"].split("/")[2])
        family = next((k for k, v in REST_FAMILIES.items() if v == segment), segment)
        rest_groups.setdefault(family, set()).add(route["method"].upper() + " " + route["path"])
    for operation in operations(mcp):
        mcp_groups.setdefault(canonical_name(operation["action"]), set()).add(operation["name"])
    names = set(cli_groups) | set(rest_groups) | set(mcp_groups)
    result = {
        name: Surface(name, tuple(sorted(cli_groups.get(name, ()))), tuple(sorted(mcp_groups.get(name, ()))), tuple(sorted(rest_groups.get(name, ()))))
        for name in names
    }
    for page in SOURCE_PAGES:
        if "source" not in result:
            raise ContractError("source compatibility pages require the canonical source surface")
        source = result["source"]
        result[page] = Surface(page, ("axon <source>",), source.mcp, source.rest, "Source-specific guide, not a dedicated provider command. Use unified source acquisition.")
    if "sessions" in result:
        source = result.get("source")
        if source is None:
            raise ContractError("session ingestion requires the canonical source surface")
        result["sessions"] = Surface(
            "sessions", result["sessions"].cli, source.mcp, source.rest,
            "Transcript ingestion uses session:<provider>:<path> through source acquisition; mobile chat sessions are a different resource.",
        )
    return result


def entries(values: tuple[str, ...]) -> str:
    return "<br>".join(f"<code>{cell(value)}</code>" for value in values) if values else "Not exposed in this registry"


def generated_block(surface: Surface) -> str:
    return "\n".join([
        BEGIN, "## Surfaces", "", "| Surface | Entry point |", "|---|---|",
        f"| CLI | {entries(surface.cli)} |",
        f"| REST | {entries(surface.rest)} |",
        f"| MCP atomic tools | {entries(surface.mcp)} |",
        "| Shared service ownership | [axon-services](../../../crates/axon-services/src/lib.rs) and the owning domain crate; see [crate ownership](../../architecture/crate-ownership.md) |",
        "",
        f"MCP names describe the atomic projection. The legacy {T}axon{T} tool uses the corresponding action/subaction selectors; {T}both{T} exposes both projections. Discover the running server before calling. [MCP contract](../mcp/tool-schema.md) owns exact schemas and selectors.",
        "",
        ("Family-level navigation does not imply identical suboperations or request shapes across transports. " + surface.notes).rstrip(),
        END,
    ])


def insert_block(text: str, block: str) -> str:
    """Replace exactly one owned block; preserve all bytes outside it."""
    starts, ends = text.count(BEGIN), text.count(END)
    if starts or ends:
        if starts != 1 or ends != 1 or text.index(BEGIN) >= text.index(END):
            raise ContractError("action page has malformed or duplicate generated markers; repair the marker pair without losing prose")
        start, end = text.index(BEGIN), text.index(END) + len(END)
        return text[:start] + block + text[end:]
    lines = text.splitlines(keepends=True)
    start = 0
    if lines and lines[0].strip() == "---":
        closing = next((i for i in range(1, len(lines)) if lines[i].strip() == "---"), None)
        if closing is None:
            raise ContractError("action page has unclosed YAML frontmatter")
        start = closing + 1
    for i in range(start, len(lines)):
        if lines[i].startswith("# "):
            start = i + 1
            if start < len(lines) and lines[start].startswith("Last Modified:"):
                start += 1
            break
    return "".join(lines[:start]) + "\n" + block + "\n\n" + "".join(lines[start:])


def generate_index(actions_dir: Path, surfaces: dict[str, Surface]) -> str:
    pages = {canonical_name(p.stem): p for p in sorted(actions_dir.glob("*.md")) if p.name != "README.md"}
    lines = [
        "# Action Reference", "",
        "<!-- AUTO-GENERATED by scripts/generate_action_docs.py; edit source action pages, not this index. -->", "",
        "CLI, REST, and MCP are projections over shared Rust services. This index reads the generated CLI, REST, and MCP JSON registries; it does not parse the presentation-only parity matrix.", "",
        "## Dispatch Layer", "", "| Layer | Source of truth |", "|---|---|",
        "| CLI | [Generated command registry](../cli/commands.json), exported from axon-cli/axon-core |",
        "| Service | [axon-services](../../../crates/axon-services/src/lib.rs) and [crate ownership](../../architecture/crate-ownership.md) |",
        "| REST | [Generated route registry](../rest/openapi.json), exported from axon-web |",
        "| MCP | [Operation schemas and projection metadata](../mcp/tool-schema.json), exported from axon-mcp |", "",
        "## Actions", "", "| Family | CLI | REST | MCP atomic tools | Guide |", "|---|---|---|---|---|",
    ]
    for name in sorted(set(pages) | set(surfaces)):
        surface = surfaces.get(name, Surface(name, (), (), (), "No canonical surface has this page's name."))
        page = pages.get(name)
        guide = f"[{cell(name)}]({page.name})" if page else "See linked generated registries; no dedicated action guide"
        lines.append(f"| {cell(name)} | {entries(surface.cli)} | {entries(surface.rest)} | {entries(surface.mcp)} | {guide} |")
    lines += ["", "## Generation", "",
        "~~~bash", "cargo xtask generated-contracts refresh", "cargo xtask generated-contracts check", "~~~", "",
        "This generator owns this index and only the marked Surfaces block in each action page. Narrative, examples, and dates outside those blocks remain handwritten. Missing registry inputs, bad provenance, and malformed markers fail before any output is written.", "",
        "The index includes canonical families without a dedicated prose page instead of silently omitting them. A family join is navigation, not a per-operation parity guarantee. MCP atomic names apply only when that projection is enabled; the aggregate tool and auxiliary presentation/resource/task surfaces remain distinct."]
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail on drift without writing")
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.repo_root.resolve()
    try:
        surfaces = read_surfaces(root)
        directory = root / "docs/reference/actions"
        if not directory.is_dir():
            raise ContractError("missing docs/reference/actions directory")
        updates: dict[Path, str] = {}
        for page in sorted(directory.glob("*.md")):
            if page.name == "README.md":
                continue
            name = canonical_name(page.stem)
            surface = surfaces.get(name, Surface(name, (), (), (), "This guide has no matching canonical operation family; do not infer a command from its filename."))
            updates[page] = insert_block(page.read_text(encoding="utf-8"), generated_block(surface))
        updates[directory / "README.md"] = generate_index(directory, surfaces)
        stale = [path for path, text in updates.items() if not path.exists() or path.read_text(encoding="utf-8") != text]
        if args.check:
            for path in stale:
                print(f"stale: {path.relative_to(root)}; run cargo xtask generated-contracts refresh", file=sys.stderr)
            return int(bool(stale))
        for path in stale:
            path.write_text(updates[path], encoding="utf-8")
        print(f"Action documentation: refreshed {len(stale)} owned outputs/blocks")
        return 0
    except (ContractError, OSError) as error:
        print(f"Action documentation input error: {error}; no output was written during input validation", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
