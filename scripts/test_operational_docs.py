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

security = (root / "docs/operations/security.md").read_text()
for stale in (
    "Published on all interfaces.",
    "forbids adding such a prefix",
    "do not rely on the compose file to loopback-bind them",
    "Do **not** add `127.0.0.1:` prefixes",
):
    if stale in security:
        raise SystemExit(f"security guide contradicts loopback-only compose policy: {stale}")

deployment = (root / "docs/operations/deployment.md").read_text()
plugin_readme = (root / "plugins/axon/README.md").read_text()
if "deploy and roll back Axon safely in self-hosted environments using Docker Compose" in deployment:
    raise SystemExit("deployment guide treats Docker Compose as a supported Axon production runtime")
for stale in (
    "Code-only rollback is compose-based and image-based.",
    "docker-compose.prod.yaml up -d\n",
    "only then syncs the Compose service",
):
    if stale in deployment:
        raise SystemExit(f"deployment guide retains unsupported Compose Axon lifecycle: {stale}")
if "Axon supports Docker Compose, bare-metal systemd" in plugin_readme:
    raise SystemExit("plugin guide contradicts the root production deployment contract")

readme = (root / "README.md").read_text()
if "110 commands across 49" in readme:
    raise SystemExit("README command total is stale")
if "raw.githubusercontent.com/dinglebear-ai/axon/main/install.ps1 | iex" in readme:
    raise SystemExit("README executes a mutable Windows installer directly")
for required in (
    "releases/download/vX.Y.Z/install.sh",
    "releases/download/vX.Y.Z/install.ps1",
    "environments/release-signing/variables/AXON_UPDATE_MINISIGN_PUBKEY",
):
    if required not in readme:
        raise SystemExit(f"README lacks concrete release installer trust path: {required}")
windows_installer = (root / "install.ps1").read_text()
if "raw.githubusercontent.com/dinglebear-ai/axon/main/install.ps1 | iex" in windows_installer:
    raise SystemExit("Windows installer recommends executing a mutable bootstrap directly")

overview = (root / "docs/reference/cli/overview.md").read_text()
if "110 commands" in overview:
    raise SystemExit("CLI overview command total is stale")

env_matrix = (root / "docs/reference/env-matrix.toml").read_text()
minisign_entry = env_matrix.split('key = "AXON_UPDATE_MINISIGN_PUBKEY"', 1)[1].split("[[env]]", 1)[0]
if "Optional minisign public key" in minisign_entry or "SHA256-only" in minisign_entry:
    raise SystemExit("env matrix describes mandatory updater authentication as optional")

integrity = (root / "crates/axon-cli/src/commands/update/integrity.rs").read_text()
for stale in ("Inert otherwise", "signature verification is optional"):
    if stale in integrity:
        raise SystemExit(f"updater integrity comment describes fail-open behavior: {stale}")

print("ok - operational documentation contracts passed")
