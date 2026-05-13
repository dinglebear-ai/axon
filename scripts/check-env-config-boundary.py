#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/config/env-migration-matrix.toml"

ENV_RE = re.compile(r"\b[A-Z][A-Z0-9_]{2,}\b")
SCAN_GLOBS = [
    "src/**/*.rs",
    "tests/**/*.rs",
    "scripts/**",
    "docker-compose.yaml",
    ".env.example",
    "config.example.toml",
    "docs/CONFIG.md",
    "docs/mcp/ENV.md",
    "docs/auth/MCP-AUTH.md",
    "docs/SETUP.md",
    "docs/DEPLOYMENT.md",
    "docs/SECURITY.md",
]

PREFIXES = (
    "AXON_",
    "OPENAI_",
    "TEI_",
    "QDRANT_",
    "TAVILY_",
    "GITHUB_",
    "REDDIT_",
    "HF_",
    "GEMINI_",
    "GOOGLE_",
    "CUDA_",
    "NVIDIA_",
)

IGNORED_TOKENS = {
    "AXON_RUST",  # issue id prefix in docs/tests
}

VALID_CLASSIFICATIONS = {
    "keep-env",
    "compose-env",
    "move-toml",
    "delete",
    "hard-default",
    "trusted-operator-bootstrap",
    "compatibility-shim",
    "external/test-only",
}

VALID_PLACEMENTS = {
    "host-only",
    "container-required",
    "compose-interpolation",
    "both",
    "not-runtime",
}

ENV_ONLY_CLASSIFICATIONS = {
    "keep-env",
    "compose-env",
    "trusted-operator-bootstrap",
}


def load_matrix() -> dict[str, dict[str, object]]:
    data = tomllib.loads(MATRIX.read_text())
    entries = data.get("env", [])
    by_key: dict[str, dict[str, object]] = {}
    for entry in entries:
        key = str(entry["key"])
        if key in by_key:
            raise SystemExit(f"duplicate matrix key: {key}")
        by_key[key] = entry
    return by_key


def scan_env_tokens() -> dict[str, set[str]]:
    found: dict[str, set[str]] = {}
    for pattern in SCAN_GLOBS:
        for path in ROOT.glob(pattern):
            if path.is_dir():
                continue
            rel = path.relative_to(ROOT)
            if any(part in {".git", ".worktrees", "target"} for part in rel.parts):
                continue
            text = path.read_text(errors="ignore")
            for token in ENV_RE.findall(text):
                if token in IGNORED_TOKENS or token.endswith("_"):
                    continue
                if token.startswith(PREFIXES):
                    found.setdefault(token, set()).add(str(rel))
    return found


def missing_key_errors(missing: list[str], found: dict[str, set[str]]) -> list[str]:
    errors: list[str] = []
    if not missing:
        return errors

    errors.append("Env keys missing from migration matrix:")
    for key in missing:
        errors.append(f"  {key}: {', '.join(sorted(found[key])[:8])}")
    return errors


def entry_errors(key: str, entry: dict[str, object]) -> list[str]:
    errors: list[str] = []
    classification = entry.get("classification")
    placement = entry.get("runtime_placement")
    toml_destination = entry.get("toml_destination")

    if classification not in VALID_CLASSIFICATIONS:
        errors.append(f"{key}: invalid classification {classification!r}")
    if placement not in VALID_PLACEMENTS:
        errors.append(f"{key}: invalid runtime_placement {placement!r}")
    if classification == "move-toml" and not toml_destination:
        errors.append(f"{key}: move-toml requires toml_destination")
    if classification in ENV_ONLY_CLASSIFICATIONS and toml_destination:
        errors.append(f"{key}: env/bootstrap key must not have toml_destination")

    return errors


def main() -> int:
    matrix = load_matrix()
    found = scan_env_tokens()
    missing = sorted(set(found) - set(matrix))

    errors = missing_key_errors(missing, found)
    for key, entry in sorted(matrix.items()):
        errors.extend(entry_errors(key, entry))

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"env/config boundary ok: {len(matrix)} classified keys")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
