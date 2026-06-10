#!/usr/bin/env bash
set -euo pipefail

# Ask-quality regression gate.
#
# Every test is run through cargo_test_filter_guard.py, which lists matching
# tests first and FAILS if a filter matches zero tests. This closes the
# silent-pass hole: `cargo test <filter>` exits 0 when the filter matches
# nothing, so a renamed/deleted test would let the gate go green while
# asserting nothing. The guard turns a zero-match filter into a hard error.
#
# NOTE: four former filters were removed because their target tests/functions
# do not exist in the tree (they matched zero tests and were silently passing):
#   - ask_quality_regression_fixtures_five_queries
#   - procedural_query_requires_official_docs_citation
#   - config_schema_query_requires_exact_page_citation
#   - authoritative_allowlist_matches_exact_and_suffix_hosts
# These represent intended coverage (citation grounding, authoritative-host
# allowlisting, a five-query fixture set) that should be authored as a
# follow-up — together with the underlying allowlist/policy code they assert.
# Do NOT re-add a filter here until the matching test exists, or the guard
# will (correctly) fail the gate.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
GUARD="python3 ${SCRIPT_DIR}/cargo_test_filter_guard.py"

echo "[ask-quality] Running regression fixtures and policy tests..."

${GUARD} -- cargo test -q --locked normalize_ask_answer_dedupes_sources_by_url
${GUARD} -- cargo test -q --locked normalize_ask_answer_formats_insufficient_evidence_when_uncited
${GUARD} -- cargo test -q --locked normalize_ask_answer_formats_insufficient_evidence_when_flagged_in_body
${GUARD} -- cargo test -q --locked non_trivial_answer_requires_minimum_citation_count

echo "[ask-quality] All regression checks passed."
