#!/usr/bin/env python3
"""Validate the bundled Claude Code plugin contract without build tooling."""

import json
from pathlib import Path


root = Path(__file__).resolve().parents[1]
manifest = root / "plugins/axon/.claude-plugin/plugin.json"
if not manifest.exists():
    manifest = root / ".claude-plugin/plugin.json"

plugin = json.loads(manifest.read_text(encoding="utf-8"))
for key in ("name", "description", "author"):
    if not plugin.get(key):
        raise SystemExit(f"MISSING: {manifest.relative_to(root)} {key}")
if "version" in plugin:
    raise SystemExit(f"FORBIDDEN: {manifest.relative_to(root)} version")

monitors = manifest.parent / "monitors/monitors.json"
if not monitors.exists():
    raise SystemExit(f"MISSING: {monitors.relative_to(root)}")
json.loads(monitors.read_text(encoding="utf-8"))

mcp_config = manifest.parent.parent / ".mcp.json"
if not mcp_config.exists():
    raise SystemExit(f"MISSING: {mcp_config.relative_to(root)}")
json.loads(mcp_config.read_text(encoding="utf-8"))

readme = manifest.parent.parent / "README.md"
text = readme.read_text(encoding="utf-8")
if '"action": "crawl", "subaction": "status"' in text:
    raise SystemExit(f"REMOVED PER-FAMILY LIFECYCLE in {readme.relative_to(root)}")
if '"action": "jobs", "subaction": "get"' not in text:
    raise SystemExit(f"MISSING unified jobs example in {readme.relative_to(root)}")

print("OK")
