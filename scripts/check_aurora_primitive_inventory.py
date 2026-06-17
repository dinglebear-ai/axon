#!/usr/bin/env python3
"""Validate Aurora primitive inventory and guard audited raw-control smells."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
INVENTORY_PATH = ROOT / "docs/reference/aurora-primitive-inventory.json"
WEB_TAG_RE = re.compile(r"<(button|input|select|textarea|kbd)\b")
ANDROID_PATTERNS = (
    "BasicTextField",
    "IconButton",
    "ToolbarIconButton",
    "SendButton",
    "CompactActionButton",
    "SettingsTabButton",
    "SuggestionChip",
    "SwitchRow",
    "AxonCompactTabs",
    "AuroraProgressBar",
    "AuroraStatusDot",
    "ProgressBar",
    "AxonSidebarRow",
    "Sidebar",
)


def rel(path: Path) -> str:
    return path.relative_to(ROOT).as_posix()


def line_number(text: str, index: int) -> int:
    return text.count("\n", 0, index) + 1


def load_inventory() -> dict:
    with INVENTORY_PATH.open(encoding="utf-8") as fh:
        return json.load(fh)


def validate_schema(inventory: dict) -> list[str]:
    errors: list[str] = []
    required = {
        "id",
        "platform",
        "file_paths",
        "symbols",
        "current_primitive_smell",
        "classification",
        "migration_owner",
        "upstream_requirement",
        "app_specific_exception",
        "secret_classification",
        "expected_verification",
        "stale_bead_links",
    }
    valid_classes = set(inventory["classifications"].keys())
    rows = inventory.get("rows", [])
    row_ids = set()
    for row in rows:
        missing = sorted(required.difference(row))
        if missing:
            errors.append(f"{row.get('id', '<missing-id>')}: missing fields {', '.join(missing)}")
        row_id = row.get("id")
        if row_id in row_ids:
            errors.append(f"{row_id}: duplicate row id")
        row_ids.add(row_id)
        if row.get("classification") not in valid_classes:
            errors.append(f"{row_id}: invalid classification {row.get('classification')!r}")
        for file_path in row.get("file_paths", []):
            if file_path and not (ROOT / file_path).exists():
                errors.append(f"{row_id}: file path does not exist: {file_path}")

    for section in ("web_raw_control_allowlist", "android_reusable_control_allowlist"):
        for entry in inventory.get(section, []):
            if entry.get("row_id") not in row_ids:
                errors.append(f"{section}: unknown row_id {entry.get('row_id')!r}")
            file_path = entry.get("file_path")
            if file_path and not (ROOT / file_path).exists():
                errors.append(f"{section}: file path does not exist: {file_path}")

    for rec in inventory.get("stale_bead_recommendations", []):
        for row_id in rec.get("row_ids", []):
            if row_id not in row_ids:
                errors.append(f"stale recommendation {rec.get('bead_id')}: unknown row_id {row_id}")
    return errors


def scan_web(inventory: dict) -> list[str]:
    allow = {
        (entry["file_path"], entry["tag"])
        for entry in inventory.get("web_raw_control_allowlist", [])
    }
    errors: list[str] = []
    for path in sorted((ROOT / "apps/palette-tauri/src").rglob("*.tsx")):
        path_rel = rel(path)
        if "/components/ui/aurora/" in f"/{path_rel}" or path.name.endswith(".test.tsx"):
            continue
        text = path.read_text(encoding="utf-8")
        for match in WEB_TAG_RE.finditer(text):
            tag = match.group(1)
            if (path_rel, tag) not in allow:
                errors.append(f"web raw <{tag}> not inventoried: {path_rel}:{line_number(text, match.start())}")
    return errors


def scan_android(inventory: dict) -> list[str]:
    allow = {
        (entry["file_path"], entry["pattern"])
        for entry in inventory.get("android_reusable_control_allowlist", [])
    }
    errors: list[str] = []
    app_src = ROOT / "apps/android/app/src/main/java"
    if not app_src.exists():
        return errors
    for path in sorted(app_src.rglob("*.kt")):
        path_rel = rel(path)
        text = path.read_text(encoding="utf-8")
        for pattern in ANDROID_PATTERNS:
            for match in re.finditer(rf"\b{re.escape(pattern)}\b", text):
                if (path_rel, pattern) not in allow:
                    errors.append(
                        f"android reusable control smell not inventoried ({pattern}): "
                        f"{path_rel}:{line_number(text, match.start())}"
                    )
    return errors


def main() -> int:
    inventory = load_inventory()
    errors = []
    errors.extend(validate_schema(inventory))
    errors.extend(scan_web(inventory))
    errors.extend(scan_android(inventory))
    if errors:
        print("Aurora primitive inventory check failed:", file=sys.stderr)
        for error in errors:
            print(f"  - {error}", file=sys.stderr)
        return 1
    print("Aurora primitive inventory check passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
