#!/usr/bin/env python3
"""Generate action reference index and per-action surface blocks.

Sources:
- docs/reference/api-parity.md for CLI/service/MCP/REST parity
- docs/reference/actions/*.md as the existing human-authored body pages

The generator intentionally owns only the index page and the marked "Surfaces"
block in each action page. Command-specific examples and operational notes stay
handwritten below the generated block.
"""

from __future__ import annotations

import argparse
import dataclasses
import html
import json
import re
from pathlib import Path


BEGIN = "<!-- BEGIN GENERATED ACTION SURFACES -->"
END = "<!-- END GENERATED ACTION SURFACES -->"


@dataclasses.dataclass
class Surface:
    name: str
    service: str
    mcp: str
    rest: str
    notes: str


REMOVED_CLI_ENTRIES = {
    "crawl": "`axon crawl` reserved; use `axon <url> --scope site|docs`",
    "embed": "Removed; use `axon <path-or-source>`",
    "ingest": "Removed; use `axon <source>`",
    "code-search": "Removed; use `axon <path> --scope directory`",
    "code-search-watch": "Removed; use `axon <path> --watch`",
}


def strip_ticks(value: str) -> str:
    value = re.sub(r"<br\s*/?>", ", ", value)
    value = re.sub(r"\s+", " ", value).strip()
    return value


def split_markdown_row(line: str) -> list[str]:
    raw = line.strip().strip("|")
    return [cell.strip() for cell in raw.split("|")]


def parse_api_parity(path: Path) -> dict[str, Surface]:
    rows: dict[str, Surface] = {}
    in_matrix = False
    for line in path.read_text().splitlines():
        if line.startswith("| Operation |"):
            in_matrix = True
            continue
        if in_matrix and (line.startswith("## ") or line.startswith("**Notes.")):
            break
        if not in_matrix or not line.startswith("| `"):
            continue
        cells = split_markdown_row(line)
        if len(cells) != 4:
            continue
        match = re.fullmatch(r"`([^`]+)`", cells[0])
        if not match:
            continue
        name = match.group(1)
        rows[name] = Surface(
            name=name,
            service="Shared domain/service implementation",
            mcp=f"`{name}`" if cells[2] == "✓" else "no dedicated action",
            rest="See docs/reference/rest/routes.md" if cells[3] == "✓" else "Not exposed",
            notes="",
        )
    # CLI action names and REST resource names differ for these shared
    # collection endpoints.
    rest_aliases = {
        "source": "`POST /v1/sources`",
        "watch": "`/v1/watches` routes",
        "memory": "`/v1/memories` routes",
    }
    for action, route in rest_aliases.items():
        if action in rows:
            rows[action].rest = route
    if not rows:
        raise RuntimeError(f"no parity rows parsed from {path}")
    return rows


def cli_entry(action: str, surface: Surface) -> str:
    if action in REMOVED_CLI_ENTRIES:
        return REMOVED_CLI_ENTRIES[action]
    return f"`axon {action} ...`"


def mcp_entry(surface: Surface) -> str:
    if surface.mcp in {"no action", "no dedicated action"}:
        return "Not exposed as a dedicated MCP action."
    if "." in surface.mcp and "," in surface.mcp:
        return f"`{{ \"action\": \"{surface.name}\", \"subaction\": \"...\" }}` ({surface.mcp})"
    if "." in surface.mcp:
        first = surface.mcp.split(",", 1)[0].strip("` ")
        action, subaction = first.split(".", 1)
        return f"`{{ \"action\": \"{action}\", \"subaction\": \"{subaction}\" }}` ({surface.mcp})"
    return f"`{{ \"action\": \"{surface.mcp.strip('`')}\" }}`"


def rest_entry(surface: Surface) -> str:
    text = surface.rest
    text = re.sub(r"\s*=\s*(Implemented|Partial|Missing|Deferred)", r" (\1)", text)
    if text in {"Missing", "Deferred"}:
        return text
    return text


def generated_block(surface: Surface) -> str:
    rows = [
        ("CLI", cli_entry(surface.name, surface)),
        ("REST", rest_entry(surface)),
        ("MCP", mcp_entry(surface)),
        ("Service", f"`{surface.service}`" if not surface.service.startswith("`") else surface.service),
    ]
    lines = [
        BEGIN,
        "## Surfaces",
        "",
        "| Surface | Entry point |",
        "|---|---|",
    ]
    lines.extend(f"| {label} | {entry} |" for label, entry in rows)
    if surface.notes:
        lines.extend(["", f"Parity notes: {surface.notes}"])
    lines.append(END)
    return "\n".join(lines)


def insert_block(text: str, block: str) -> str:
    pattern = re.compile(rf"{re.escape(BEGIN)}.*?{re.escape(END)}", re.DOTALL)
    if pattern.search(text):
        return pattern.sub(block, text)

    lines = text.splitlines()
    insert_at = 1
    for index, line in enumerate(lines[:8]):
        if line.startswith("Last Modified:"):
            insert_at = index + 1
            break
    lines[insert_at:insert_at] = ["", block, ""]
    return "\n".join(lines).rstrip() + "\n"


def page_title(path: Path) -> str:
    for line in path.read_text().splitlines():
        if line.startswith("# "):
            return line[2:].strip()
    return path.stem


def generate_index(actions_dir: Path, surfaces: dict[str, Surface]) -> str:
    pages = [p for p in sorted(actions_dir.glob("*.md")) if p.name != "README.md"]
    lines = [
        "# Action Reference",
        "Last Modified: 2026-08-02",
        "",
        "<!-- AUTO-GENERATED by scripts/generate_action_docs.py; edit action pages below their generated surface block. -->",
        "",
        "Axon exposes one logical action layer through three adapters: local CLI commands, direct `/v1` REST routes, and the single MCP `axon` tool routed by `action` plus optional `subaction`. The CLI always runs in-process; client/server callers should use direct REST or MCP rather than the removed `/v1/actions` envelope.",
        "",
        "## Dispatch Layer",
        "",
        "| Layer | Contract | Source |",
        "|---|---|---|",
        "| CLI | `axon <action> ...` and lifecycle subcommands | `crates/axon-cli/src/`, `crates/axon-core/src/config/cli.rs` |",
        "| Service | Typed request/result structs shared by adapters | `crates/axon-services/src/` |",
        "| REST | Direct `/v1` routes and lifecycle job routes | `crates/axon-web/src/server/routing.rs`, `docs/reference/api-parity.md` |",
        "| MCP | One `axon` tool with `action` and optional `subaction` | `crates/axon-mcp/src/`, `docs/reference/mcp/tool-schema.md` |",
        "",
        "## Actions",
        "",
        "| Action | CLI | REST | MCP | Doc |",
        "|---|---|---|---|---|",
    ]
    for page in pages:
        action = page.stem
        surface = surfaces.get(action)
        title = html.escape(page_title(page))
        if surface is None:
            surface = Surface(
                name=action,
                service="Not inventoried",
                mcp="no dedicated action",
                rest="Not inventoried",
                notes="",
            )
            cli = cli_entry(action, surface)
            rest = "Not inventoried"
            mcp = "Not inventoried"
        else:
            cli = cli_entry(action, surface)
            rest = rest_entry(surface)
            mcp = surface.mcp
        lines.append(f"| `{action}` | {cli} | {rest} | {mcp} | [{title}]({page.name}) |")
    lines.extend(
        [
            "",
            "## Generation",
            "",
            "Regenerate this index and every per-page `Surfaces` block after CLI, REST, MCP, or service dispatch changes:",
            "",
            "```bash",
            "python3 scripts/generate_action_docs.py",
            "```",
            "",
            "The generator reads `docs/reference/api-parity.md`, then updates only this index and the marked generated blocks in `docs/reference/actions/*.md`.",
        ]
    )
    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="fail if generated docs are stale")
    args = parser.parse_args()

    repo = Path(__file__).resolve().parents[1]
    actions_dir = repo / "docs" / "reference" / "actions"
    surfaces = parse_api_parity(repo / "docs" / "reference" / "api-parity.md")
    commands_payload = json.loads(
        (repo / "docs" / "reference" / "cli" / "commands.json").read_text()
    )
    command_records = (
        commands_payload["commands"]
        if isinstance(commands_payload, dict)
        else commands_payload
    )
    expected_pages = {str(record["group"]) for record in command_records}
    page_paths = sorted(path for path in actions_dir.glob("*.md") if path.name != "README.md")
    actual_pages = {path.stem for path in page_paths}
    missing = sorted(expected_pages - actual_pages)
    obsolete = sorted(actual_pages - expected_pages)
    if missing or obsolete:
        for action in missing:
            print(f"missing action page: docs/reference/actions/{action}.md")
        for action in obsolete:
            print(f"obsolete action page: docs/reference/actions/{action}.md")
        return 1

    updates: dict[Path, str] = {}
    for path in page_paths:
        surface = surfaces.get(path.stem)
        if surface is None:
            surface = Surface(
                name=path.stem,
                service="Not inventoried",
                mcp="no dedicated action",
                rest="Not inventoried",
                notes="This action page is missing from docs/reference/api-parity.md.",
            )
        updates[path] = insert_block(path.read_text(), generated_block(surface))
    updates[actions_dir / "README.md"] = generate_index(actions_dir, surfaces)

    stale = [path for path, new_text in updates.items() if path.read_text() != new_text]
    if args.check:
        if stale:
            for path in stale:
                print(f"stale: {path.relative_to(repo)}")
            return 1
        return 0

    for path, new_text in updates.items():
        if path.read_text() != new_text:
            path.write_text(new_text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
