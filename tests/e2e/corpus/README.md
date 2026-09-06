# Axon canonical E2E corpus

This directory is the synthetic, license-safe oracle for Axon's end-to-end
tests. `manifest.json` is the machine-readable entry point. Corpus releases are
immutable directories; changing bytes or expectations requires a new release
and the update procedure below.

The tiers have deliberately different jobs:

- `micro` is the small, deterministic PR gate.
- `representative` adds broader semantic cases for scheduled and live runs.
- `stress` is generated from a checked-in recipe only during explicit capacity
  runs. It must never be selected by ordinary PR or platform-smoke workflows.

## Updating the corpus

1. Copy the current version directory to the next semantic version.
2. Make the fixture or expectation change and preserve stable source IDs for
   revisions of the same logical source.
3. Classify the change in `CHANGELOG.md`: bytes, semantic expectations,
   chunking expectations, or retrieval tolerances. Bump each affected component
   version independently. The checksum rewriter compares changes with
   `release-baseline.json` and refuses to bless changed bytes under an unchanged
   relevant component version.
4. Update lineage and the tier bounds. Exact chunk boundaries belong only in
   chunking-specific expectations.
5. Run `python3 tests/e2e/corpus/validate.py --rewrite-checksums`, review the
   checksum diff, then run `python3 tests/e2e/corpus_tests.py`. Once reviewed,
   run the validator with `--accept-release` to advance the checked baseline in
   the same commit.

The rewrite command updates file hashes and the aggregate checksum. CI invokes
the validator without `--rewrite-checksums`, so an unreviewed byte change fails.
Reports must copy `corpus_version` and `corpus_checksum` from the validation
result.
