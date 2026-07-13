# Operations Notes

## Local build/tooling adjustments

- `Justfile` removed `nextest-install` and `llvm-cov-install` recipes, shifting install guidance toward `mise`-installed tools.
- `taplo-check`, `taplo-fmt`, and coverage commands now emit actionable `mise` install commands:
  - `mise install cargo:taplo-cli`
  - `mise install cargo:cargo-llvm-cov`
- `cargo-machete` and other optional helpers remain optional and are now referenced with `mise` install guidance.

## Script/tooling guardrails

- `scripts/cargo-rustc-wrapper` now first checks:
  1. `CARGO_BIN_ARTIFACT_WRAPPER_HELPER`
  2. `cargo-bin-artifact-wrapper`
  3. `sccache-wrapper`
  4. `sccache`
  5. rustc fallback

This preserves current artifact-install behavior while allowing helper-driven replacement.

## OpenWiki/Docs control surfaces

- `CLAUDE.md` now includes an OpenWiki section with a concise pointer to `openwiki/quickstart.md`.
- `README.md` has new “Related Servers” links in this snapshot.

## Recommended operational checks

1. If CI helper-related failures appear, inspect job logs around `Install` steps in `.github/workflows/ci.yml` and `openwiki-update.yml`.
2. Validate wrapper behavior by running a normal local build path that exercises `RUSTC_WRAPPER`.
3. If action docs are stale, regenerate via:
   - `python3 scripts/generate_action_docs.py` and review generated surface blocks in `docs/reference/actions/*.md`.
