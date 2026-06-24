#!/usr/bin/env bash
set -euo pipefail

# Ask-quality regression gate.
#
# The required test names are checked before running focused filters. This
# closes the silent-pass hole: `cargo test <filter>` exits 0 when the filter
# matches nothing, so a renamed/deleted test would let the gate go green while
# asserting nothing.
#
# CI invokes `--verify-only` after the broad nextest run has already executed
# these tests. That keeps the explicit named-test guard without re-entering
# Cargo test a second time.
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

MODE="${1:-run}"
case "${MODE}" in
  run | --run | --verify-only) ;;
  *)
    echo "usage: $0 [--verify-only]" >&2
    exit 2
    ;;
esac

echo "[ask-quality] Running regression fixtures and policy tests..."

mapfile -t REQUIRED_TESTS <<'TESTS'
normalize_ask_answer_dedupes_sources_by_url
normalize_ask_answer_formats_insufficient_evidence_when_uncited
normalize_ask_answer_formats_insufficient_evidence_when_flagged_in_body
non_trivial_answer_requires_minimum_citation_count
TESTS

if [[ "${MODE}" == "--verify-only" ]]; then
  for test_name in "${REQUIRED_TESTS[@]}"; do
    pattern="fn[[:space:]]+${test_name}\\b"
    if command -v rg >/dev/null 2>&1; then
      found="$(rg -l "${pattern}" crates src tests || true)"
    elif git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
      found="$(git grep -l -E "${pattern}" -- crates src tests || true)"
    else
      found="$(grep -R -l -E "${pattern}" crates src tests || true)"
    fi
    if [[ -z "${found}" ]]; then
      echo "[ask-quality] required test function is missing: ${test_name}" >&2
      exit 1
    fi
  done
  echo "[ask-quality] Required regression tests are present; execution covered by cargo nextest."
  exit 0
fi

LIST_OUTPUT="$(cargo test --locked --workspace -- --list)"
for test_name in "${REQUIRED_TESTS[@]}"; do
  if ! grep -Fq "${test_name}:" <<<"${LIST_OUTPUT}"; then
    echo "[ask-quality] required cargo test is missing: ${test_name}" >&2
    exit 1
  fi
done

cargo test -q --locked --workspace normalize_ask_answer
cargo test -q --locked --workspace non_trivial_answer_requires_minimum_citation_count

echo "[ask-quality] All regression checks passed."
