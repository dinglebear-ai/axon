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
    # Comprehensive E2E harness controls and GitHub runner metadata. These are
    # deliberately not Axon runtime configuration and must stay out of the
    # user-facing environment contract.
    "AXON_E2E_ACTIVE_STAGE",
    "AXON_E2E_ALLOWED_ENDPOINTS",
    "AXON_E2E_ATTEMPT_ID",
    "AXON_E2E_CATALOG",
    "AXON_E2E_CHROME_GATEWAY_URL",
    "AXON_E2E_CHROME_TOKEN",
    "AXON_E2E_CLEANUP_REGISTRY",
    "AXON_E2E_EXPECTED_PEERS",
    "AXON_E2E_FAKE_PROVIDER_URL",
    "AXON_E2E_FIXTURE_BASE_URL",
    "AXON_E2E_FIXTURE_JOB_ID",
    "AXON_E2E_HERMETIC",
    "AXON_E2E_LIVE",
    "AXON_E2E_LLM_GATEWAY_URL",
    "AXON_E2E_LLM_TOKEN",
    "AXON_E2E_MANIFEST",
    "AXON_E2E_NAMESPACE",
    "AXON_E2E_NATIVE_ISOLATION",
    "AXON_E2E_NETWORK_CAPABILITY",
    "AXON_E2E_NETWORK_POLICY",
    "AXON_E2E_OWNED_ROOT",
    "AXON_E2E_PERFORMANCE_ONLY",
    "AXON_E2E_PERFORMANCE_TEARDOWN_HANDLE",
    "AXON_E2E_PROCESS_NONCE",
    "AXON_E2E_PROVIDER_MODE",
    "AXON_E2E_QDRANT_GATEWAY_URL",
    "AXON_E2E_QDRANT_TOKEN",
    "AXON_E2E_REAL_AXON_BIN",
    "AXON_E2E_REQUIRE_REAL_SOURCE_JOBS",
    "AXON_E2E_RESOURCE_MANIFEST",
    "AXON_E2E_RUNNER_CLASS",
    "AXON_E2E_RUN_ID",
    "AXON_E2E_STAGE_GATES",
    "AXON_E2E_TEI_GATEWAY_URL",
    "AXON_E2E_TEI_TOKEN",
    "AXON_E2E_TESTED_SHA",
    "AXON_MCP_ALLOWED_ORIGIN",
    "AXON_MCP_AUTH_TOKEN",
    "AXON_MCP_ORIGIN",
    "AXON_MCP_READ_TOKEN",
    "AXON_MCP_TASK_TRANSPORT",
    "AXON_MCP_URL",
    "AXON_REMOTE_URL",
    "GITHUB_ACTIONS",
    "GITHUB_EVENT_NAME",
    "GITHUB_HEAD_REF",
    "GITHUB_REPOSITORY",
    "GITHUB_RUN_ATTEMPT",
    "GITHUB_RUN_ID",
    "AXON_RUST",  # issue id prefix in docs/tests
    "AXON_DEV_BIN",  # local shell variable in scripts/axon
    "AXON_DEV_BIN_DIR",  # local shell variable in scripts/axon
    "AXON_HOME_DIR",  # local shell variable in scripts/axon
    "AXON_BACKUP_DIR",  # operational var in scripts/axon-backup.sh, not axon runtime config
    "AXON_BENCH_AXON_BIN",  # source-pipeline benchmark harness control
    "AXON_BENCH_CONFIG_PATH",  # source-pipeline benchmark harness fixture config
    "AXON_BENCH_COLLECTION",  # source-pipeline benchmark fixture state
    "AXON_BENCH_LIBRARY_MODE",  # source-pipeline benchmark harness control
    "AXON_BENCH_MLX_URL",  # source-pipeline benchmark fixture endpoint
    "AXON_BENCH_OUTPUT",  # source-pipeline benchmark report destination
    "AXON_BENCH_SOURCE",  # source-pipeline benchmark fixture input
    "AXON_BENCH_COMPARISON_ENV_SHA256",
    "AXON_BENCH_CURL_BIN",
    "AXON_BENCH_ENV_FILE",
    "AXON_BENCH_MAX_LOAD",
    "AXON_BENCH_MODE",
    "AXON_BENCH_OWN_COLLECTION",
    "AXON_BENCH_QDRANT_URL",
    "AXON_BENCH_REPLAY_FIXTURE",
    "AXON_BENCH_RETAIN_COLLECTION",
    "AXON_BENCH_RETAIN_WORK_DIR",
    "AXON_BENCH_SKIP_STALE_CHECK",
    "AXON_BENCH_WORK_DIR",
    "AXON_ALLOW_FALLBACK_WEB_ASSETS",  # local/CI build escape hatch, not runtime config
    "AXON_CHANGED_PATHS",  # workflow test fixture variable, not axon runtime config
    "AXON_FULL_PRE_PUSH",  # local hook control variable, not axon runtime config
    "AXON_PRE_PUSH_BASE",  # local hook control variable, not axon runtime config
    "AXON_BIND",  # live-harness local bind address, not runtime config
    "AXON_COMMAND_REGISTRY",  # live-harness fixture override, not runtime config
    "AXON_INCUS_RUN_SERVER",  # Incus bootstrap test control, not runtime config
    "AXON_INCUS_TEI_MODE",  # Incus bootstrap selector, not Axon runtime config
    "AXON_LIVE_COLLECTION",  # live-harness isolated fixture state
    "AXON_LIVE_CLEANUP_TIMEOUT_SECS",  # live-harness cleanup bound
    "AXON_LIVE_COMMAND_TIMEOUT_SECS",  # live-harness timeout
    "AXON_LIVE_DATA_DIR",  # live-harness isolated fixture state
    "AXON_LIVE_FIXTURE_URL",  # live-harness fixture endpoint
    "AXON_LIVE_MAP_FIXTURE_URL",  # live-harness map fixture endpoint
    "AXON_LIVE_PARSER_JOBS",  # live-harness parser concurrency
    "AXON_LIVE_PORT_BASE",  # live-harness per-run port allocation
    "AXON_LIVE_TEST_ROOT",  # live-harness isolated output root
    "AXON_LIVE_USE_PRODUCTION_STATE",  # live-harness opt-in control
    "AXON_MLX_TEST_MODE",  # Apple MLX script test-harness control
    "AXON_LIVE_HARNESS_SET",  # live-harness config mutation fixture
    "AXON_LIVE_HARNESS_TOKEN",  # live-harness config secret fixture
    "AXON_ARTIFACT_ROOT",  # isolated live/stress harness artifact root
    "AXON_STRESS_COLLECTION",  # stress-harness isolated collection override
    "AXON_STRESS_CONCURRENT_JOBS",  # stress-harness workload control
    "AXON_STRESS_CONFIRM",  # stress-harness destructive confirmation guard
    "AXON_STRESS_MAP_MAX_PAGES",  # stress-harness map bound
    "AXON_STRESS_MAP_TIMEOUT_SECS",  # stress-harness map timeout
    "AXON_STRESS_MAX_PAGES",  # stress-harness workload bound
    "AXON_STRESS_MIN_COMPLETION_PERCENT",  # stress-harness acceptance threshold
    "AXON_STRESS_OUTDIR",  # stress-harness report directory
    "AXON_STRESS_SOURCE_CONCURRENCY",  # stress-harness source concurrency
    "AXON_STRESS_TIMEOUT_SECS",  # stress-harness terminal deadline
    "AXON_STRESS_URL",  # stress-harness target override
    "AXON_STRESS_WORKER_CONCURRENCY",  # stress-harness worker concurrency
    "AXON_TEST_FROM_FILE",  # axon-env loader regression fixture
    "AXON_TEST_PRECEDENCE",  # axon-env loader regression fixture
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
    "GITHUB_ENV",  # GitHub Actions command file, not axon runtime config
    "GITHUB_STEP_SUMMARY",  # GitHub Actions job summary file, not axon runtime config
    "GITHUB_REF",  # GitHub Actions runtime variable, not axon runtime config
    "GITHUB_SHA",  # GitHub Actions runtime variable, not axon runtime config
    "TEI_MODE",  # local Incus bootstrap shell variable, not runtime configuration
    "TEI_TUNE_CACHE",  # TEI benchmark harness control
    "TEI_TUNE_CONTAINER",  # TEI benchmark harness control
    "TEI_TUNE_ENTRYPOINT",  # TEI benchmark harness control
    "TEI_TUNE_GPU",  # TEI benchmark harness control
    "TEI_TUNE_HOST",  # TEI benchmark harness control
    "TEI_TUNE_IMAGE",  # TEI benchmark harness control
    "TEI_TUNE_NETWORK",  # TEI benchmark harness control
    "TEI_TUNE_PORT",  # TEI benchmark harness control
    "TEI_TUNE_STATE_DIR",  # TEI benchmark harness control
    "TEI_TUNE_URL",  # TEI benchmark harness control
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
    "pipeline.max-active-source-jobs",
    "jobs.event-retention-days",
    "jobs.failed-event-retention-days",
    "jobs.terminal-retention-days",
    "jobs.provider-health-retention-days",
    "jobs.artifact-retention-days",
    "jobs.retention-sweep-secs",
    "jobs.interactive-starvation-slo-secs",
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
    "providers.embedding.batch-size",
    "providers.embedding.max-retries",
    "providers.embedding.request-timeout-ms",
    "providers.embedding.retry-backoff-ms",
    "providers.embedding.cooldown-after-failures",
    "providers.embedding.cooldown-secs",
    "providers.embedding.interactive-reserved-requests",
    "providers.embedding.background-max-concurrent-requests",
    "qdrant.async-writes",
    "qdrant.quantization-enabled",
    "qdrant.transport",
    "providers.embedding.maintenance-max-concurrent-requests",
    "providers.embedding.query-instruction-enabled",
    "providers.embedding.cache-enabled",
    "providers.embedding.cache-max-entries",
    "providers.embedding.max-concurrent-requests",
    "providers.embedding.max-in-flight-inputs",
    "providers.embedding.pool-max-inputs",
    "providers.embedding.prepared-byte-budget",
    "providers.embedding.scheduler-enabled",
    "providers.embedding.scheduler-flush-ms",
    "providers.embedding.prep-max-in-flight-bytes",
    "providers.embedding.max-batch-tokens",
    "providers.embedding.vector-upsert-overlap-enabled",
    "providers.embedding.prep-concurrency",
    "providers.embedding.max-chunks-per-doc",
    "providers.embedding.max-source-chunks-per-doc",
    "providers.embedding.dedupe-exact-chunks",
    "providers.embedding.openai-model",
    "providers.embedding.openai-max-client-batch-size",
    "providers.embedding.openai-max-concurrent",
    "providers.embedding.openai-max-in-flight-inputs",
    "providers.embedding.openai-pool-max-inputs",
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
