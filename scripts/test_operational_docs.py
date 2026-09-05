#!/usr/bin/env python3
"""Cross-file contracts for active operational and plugin documentation."""

from pathlib import Path
import json
import re

root = Path(__file__).resolve().parents[1]
active = [
    root / "README.md",
    root / "docs/operations/operations.md",
    root / "docs/operations/deployment.md",
    root / "docs/architecture/overview.md",
    root / "docs/guides/ingest/sessions.md",
    root / "docs/reference/runtime/memory.md",
    root / "docs/reference/actions/setup.md",
    root / "plugins/axon/README.md",
    root / "plugins/axon/CHANGELOG.md",
]
joined = "\n".join(path.read_text(encoding="utf-8") for path in active)

retired = re.compile(r"(?:axon|scripts/axon) (?:crawl|embed|ingest|extract) (?:list|status|errors|recover|cancel|cleanup|clear)")
match = retired.search(joined)
if match:
    raise SystemExit(f"retired per-family lifecycle in active docs: {match.group(0)}")

positive_hook_claims = (
    "Its `SessionStart` hook runs",
    "plugin's `SessionStart` hook calls",
    "Run by the plugin's SessionStart hook",
)
for claim in positive_hook_claims:
    if claim in joined:
        raise SystemExit(f"inactive plugin hook documented as active: {claim}")

manifest = json.loads((root / "plugins/axon/.claude-plugin/plugin.json").read_text())
if manifest["license"] != "AGPL-3.0-only":
    raise SystemExit("plugin license must match the repository AGPL-only contract")
if manifest["userConfig"]["server_url"]["default"] != "http://localhost:8001":
    raise SystemExit("plugin default URL must match axon serve")

configuration = (root / "docs/guides/configuration.md").read_text()
if "AXON_HTTP_PUBLISH=127.0.0.1:8001" in configuration:
    raise SystemExit("AXON_HTTP_PUBLISH must be documented as a numeric port")

for path in (root / "docs/guides/getting-started.md", root / "docs/architecture/stack/tech.md", root / "docs/architecture/stack/pre-reqs.md"):
    if "1.94" in path.read_text():
        raise SystemExit(f"stale Rust version in {path.relative_to(root)}")

integrity = (root / "crates/axon-cli/src/commands/update/integrity.rs").read_text()
for stale in ("Inert otherwise", "signature verification is optional"):
    if stale in integrity:
        raise SystemExit(f"updater integrity comment describes fail-open behavior: {stale}")

print("ok - operational documentation contracts passed")
