#!/usr/bin/env python3
from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MATRIX = ROOT / "docs/reference/env-matrix.toml"

ENV_RE = re.compile(r"\b[A-Z][A-Z0-9_]{2,}\b")
SCAN_GLOBS = [
    "src/**/*.rs",
    "tests/**/*.rs",
    "scripts/**",
    "docker-compose.prod.yaml",
    ".env.example",
    "config.example.toml",
    "docs/guides/configuration.md",
    "docs/reference/mcp/env.md",
    "docs/operations/auth/mcp-auth.md",
    "docs/guides/getting-started.md",
    "docs/operations/deployment.md",
    "docs/operations/security.md",
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
    "AXON_DEV_BIN",  # local shell variable in scripts/axon
    "AXON_DEV_BIN_DIR",  # local shell variable in scripts/axon
    "AXON_HOME_DIR",  # local shell variable in scripts/axon
    "AXON_BACKUP_DIR",  # operational var in scripts/axon-backup.sh, not axon runtime config
    "AXON_ALLOW_FALLBACK_WEB_ASSETS",  # local/CI build escape hatch, not runtime config
    "AXON_CHANGED_PATHS",  # workflow test fixture variable, not axon runtime config
    "AXON_FULL_PRE_PUSH",  # local hook control variable, not axon runtime config
    "AXON_PRE_PUSH_BASE",  # local hook control variable, not axon runtime config
    "QDRANT_DEST",  # local shell variable in scripts/axon-backup.sh
    "QDRANT_DIR",  # local shell variable in scripts/axon-backup.sh
    "QDRANT_SHA256",  # local shell variable in scripts/axon-backup.sh
    "QDRANT_SIZE",  # local shell variable in scripts/axon-backup.sh
    "AXON_API_UA",  # Rust User-Agent const, not an env var
    "AXON_FULL_ACCESS_SCOPE",  # Rust authz const, not an env var
    "AXON_API_UA",  # Rust const (User-Agent string), not an env var
    "AXON_READ_SCOPE",  # Rust authz const, not an env var
    "AXON_WRITE_SCOPE",  # Rust authz const, not an env var
    "REDDIT_UA",  # Rust const (User-Agent string), not an env var; lives in src/extract/verticals/reddit.rs
    "TAVILY_BACKOFF_BASE",  # Rust const, not an env var
    "TAVILY_MAX_ATTEMPTS",  # Rust const, not an env var
    "GEMINI_SKILL_INVOCATION",  # Rust prompt const, not an env var
    "GOOGLE_OAUTH_COLORS",  # Rust const (color hex list for brand filtering), not an env var
    "GEMINI_SKILL_INVOCATION",  # Rust const (ask synthesis prompt fragment), not an env var
    "OPENAI_COMPAT_SECRET",  # fake secret string literal in runners_tests.rs redaction test, not an env var
    "GEMINI_DEFAULT_COMPLETION_CONCURRENCY",  # Rust const (default concurrency) in core/llm/types.rs, not an env var
    "OPENAI_DEFAULT_COMPLETION_CONCURRENCY",  # Rust const (default concurrency) in core/llm/types.rs, not an env var
    "GITHUB_REF",  # GitHub Actions runtime variable, not axon runtime config
    "GITHUB_SHA",  # GitHub Actions runtime variable, not axon runtime config
}

VALID_CLASSIFICATIONS = {
    "keep-env",
    "compose-env",
    "move-toml",
    "hard-default",
    "trusted-operator-bootstrap",
    "codex-child-auth",
    "external/test-only",
}

VALID_PLACEMENTS = {
    "host-only",
    "container-required",
    "compose-interpolation",
    "child-only",
    "both",
    "not-runtime",
}

ENV_ONLY_CLASSIFICATIONS = {
    "keep-env",
    "compose-env",
    "trusted-operator-bootstrap",
}

MIGRATION_ACTION_CLASSIFICATIONS = {
    "move-toml",
    "hard-default",
    "compose-env",
    "trusted-operator-bootstrap",
}

VALID_TOML_DESTINATIONS = {
    "search.hybrid-enabled",
    "search.hybrid-candidates",
    "search.ask-hybrid-candidates",
    "search.hnsw-ef",
    "search.hnsw-ef-legacy",
    "search.collection",
    "ask.chunk-limit",
    "ask.candidate-limit",
    "ask.min-relevance-score",
    "ask.cache.enabled",
    "ask.cache.max-capacity-bytes",
    "ask.cache.ttl-secs",
    "ask.adaptive.fulldoc-skip-enabled",
    "ask.adaptive.fulldoc-skip-min-urls",
    "ask.adaptive.fulldoc-skip-min-chars",
    "ask.adaptive.fulldoc-skip-score-delta",
    "tei.max-retries",
    "tei.request-timeout-ms",
    "tei.max-client-batch-size",
    "scrape.batch-timeout-secs",
    "workers.ingest-lanes",
    "workers.embed-lanes",
    "workers.embed-doc-timeout-secs",
    "workers.unified-worker-concurrency",
    "workers.queue-summary-secs",
    "workers.qdrant-point-buffer",
    "workers.max-pending-crawl-jobs",
    "workers.max-pending-embed-jobs",
    "workers.max-pending-extract-jobs",
    "workers.max-pending-ingest-jobs",
    "workers.job-wait-timeout-secs",
    "chrome.user-agent",
    "ask.max-context-chars",
    "ask.full-docs",
    "ask.backfill-chunks",
    "ask.doc-fetch-concurrency",
    "ask.doc-chunk-limit",
    "ask.authoritative-domains",
    "ask.authoritative-boost",
    "ask.min-citations-nontrivial",
    "logging.max-bytes",
    # Webclaw feature destinations
    "scrape.allow-unbounded-broad-crawl",
    "scrape.crawl-memory-abort-percent",
    "verticals.enabled",
    "verticals.auto-dispatch-skip",
    "payload.structured-data-max-bytes",
    "scrape.ladder-strategy1-threshold",
    "scrape.ladder-strategy2-threshold",
    "scrape.ladder-body-multiplier",
    "antibot.cookie-warmup",
    "antibot.max-body-scan-bytes",
    "freshness.tick-secs",
    "freshness.lease-secs",
    "freshness.max-due-per-tick",
    "freshness.max-concurrent-runs",
    "freshness.run-retention-days",
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
            if any(part in {".git", ".worktrees", "__pycache__", "target"} for part in rel.parts):
                continue
            if str(rel) == "scripts/check_legacy_runtime_terms.sh":
                continue
            text = path.read_text(errors="ignore")
            for token in ENV_RE.findall(text):
                if token in IGNORED_TOKENS or token.endswith("_"):
                    continue
                if token.startswith(PREFIXES):
                    found.setdefault(token, set()).add(str(rel))
    return found


def load_rust_registry_keys() -> set[str]:
    registry_root = ROOT / "crates/axon-core/src/config/parse"
    texts = [registry_root.joinpath("env_registry.rs").read_text()]
    texts.extend(path.read_text() for path in registry_root.glob("env_registry/*.rs"))
    return set(re.findall(r'spec\(\s*"([A-Z0-9_]+)"', "\n".join(texts)))


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
    if (
        classification == "move-toml"
        and toml_destination
        and toml_destination not in VALID_TOML_DESTINATIONS
    ):
        errors.append(
            f"{key}: unsupported toml_destination {toml_destination!r}; add a typed config.toml field first"
        )
    if classification in ENV_ONLY_CLASSIFICATIONS and toml_destination:
        errors.append(f"{key}: env/bootstrap key must not have toml_destination")

    return errors


def registry_parity_errors(matrix: dict[str, dict[str, object]]) -> list[str]:
    registry_keys = load_rust_registry_keys()
    missing = sorted(
        key
        for key, entry in matrix.items()
        if entry.get("classification") in MIGRATION_ACTION_CLASSIFICATIONS
        and key not in registry_keys
    )
    if not missing:
        return []
    return [
        "Migration-actionable matrix keys missing from Rust ENV_KEY_SPECS:",
        *[f"  {key}" for key in missing],
    ]


def main() -> int:
    matrix = load_matrix()
    found = scan_env_tokens()
    missing = sorted(set(found) - set(matrix))

    errors = missing_key_errors(missing, found)
    for key, entry in sorted(matrix.items()):
        errors.extend(entry_errors(key, entry))
    errors.extend(registry_parity_errors(matrix))

    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"env/config boundary ok: {len(matrix)} classified keys")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
