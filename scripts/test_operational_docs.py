#!/usr/bin/env python3
"""Cross-file contracts for active operational and plugin documentation."""

from pathlib import Path
import json
import re
import subprocess

root = Path(__file__).resolve().parents[1]

# Inspect tracked scopes only: unrelated worktrees and runtime caches are not
# documentation inputs. The Rust checker separately discovers orphan aliases.
tracked = subprocess.check_output(
    ["git", "ls-files", "-z"], cwd=root, text=True
).split("\0")
local_names = {"AGENTS.override.md", "CLAUDE.local.md", "CLAUDE.md.local"}
if any(Path(name).name in local_names for name in tracked):
    raise SystemExit("personal agent instructions must not be tracked")
ignore_lines = set((root / ".gitignore").read_text().splitlines())
if not {"AGENTS.override.md", "CLAUDE.local.md"}.issubset(ignore_lines):
    raise SystemExit("repository must ignore both canonical local instruction filenames")

agent_scopes = [root / name for name in tracked if Path(name).name == "AGENTS.md"]
if root / "AGENTS.md" not in agent_scopes:
    raise SystemExit("root AGENTS.md must be tracked")
for canonical in agent_scopes:
    if canonical.is_symlink() or not canonical.is_file():
        raise SystemExit(f"agent guide is not a regular canonical file: {canonical}")
    for name in ("CLAUDE.md", "GEMINI.md"):
        alias = canonical.with_name(name)
        if not alias.is_symlink() or alias.readlink() != Path("AGENTS.md"):
            raise SystemExit(f"agent alias must point directly to AGENTS.md: {alias}")

def validate_instruction_budget(scopes: list[Path], limit_bytes: int = 30 * 1024) -> None:
    """Leave space for loader separators below the default 32 KiB project limit."""
    for canonical in scopes:
        chain_bytes = sum(
            guide.stat().st_size
            for guide in scopes
            if guide == canonical or guide.parent in canonical.parent.parents
        )
        if chain_bytes > limit_bytes:
            raise SystemExit(
                f"instruction chain exceeds {limit_bytes} bytes at {canonical}: {chain_bytes}"
            )


def validate_agent_references(scopes: list[Path]) -> None:
    """Keep scoped instructions navigable without duplicating reference pages."""
    link_pattern = re.compile(r"\[[^\]\n]+\]\(([^)\s]+)\)")
    for guide in scopes:
        text = guide.read_text(encoding="utf-8")
        links = link_pattern.findall(text)
        if len(links) < 3:
            raise SystemExit(f"agent guide needs at least three useful references: {guide}")
        for link in links:
            target = link.split("#", 1)[0]
            if not target or ":" in target:
                continue
            if not (guide.parent / target).exists():
                raise SystemExit(f"broken agent reference in {guide}: {link}")


root_characters = len((root / "AGENTS.md").read_text(encoding="utf-8"))
if root_characters > 7500:
    raise SystemExit(f"root AGENTS.md exceeds 7500 characters: {root_characters}")
validate_instruction_budget(agent_scopes)
validate_agent_references(agent_scopes)

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
    "security/axon-release.minisign.pub",
    "releases through `v7.2.23` do not include signatures",
):
    if required not in readme:
        raise SystemExit(f"README lacks concrete release installer trust path: {required}")
windows_installer = (root / "install.ps1").read_text()
if "raw.githubusercontent.com/dinglebear-ai/axon/main/install.ps1 | iex" in windows_installer:
    raise SystemExit("Windows installer recommends executing a mutable bootstrap directly")
for required in ('-split "`r?`n"', "FromBase64String", "Length -ne 42"):
    if required not in windows_installer:
        raise SystemExit(f"Windows installer does not normalize a trusted minisign .pub file: {required}")
for required in ("$TrustedPublicKeyFile", "$TrustedPublicKey", "minisign -V -P $TrustedPublicKey"):
    if required not in readme:
        raise SystemExit(f"README does not extract the raw key for minisign -P: {required}")
systemd_readme = (root / "deploy/systemd/README.md").read_text()
if "axon --local" in systemd_readme:
    raise SystemExit("systemd runbook uses the nonexistent --local CLI flag")

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

# Include all current tracked prose, not only the curated operation examples.
from audit_docs import audit

doc_audit = audit(root)
if doc_audit["findings"]:
    raise SystemExit("documentation navigation/data failures: " + json.dumps(doc_audit["findings"]))
print(f"ok - {doc_audit['summary']['files']} documentation files inventoried; "
      f"{doc_audit['summary']['links_checked']} current links checked")
print("ok - operational documentation contracts passed")
