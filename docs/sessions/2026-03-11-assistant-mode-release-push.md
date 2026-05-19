# Session: Assistant Mode Release Push
Date: 2026-03-11
Branch: feat/github-code-aware-chunking

## Summary
Completed assistant-mode implementation across Rust + web, resolved pre-existing lint/test gate failures, bumped crate version to `0.18.0`, updated changelog, and pushed the branch.

## Version
- Cargo package: `0.17.0` -> `0.18.0` (minor bump, `feat` release commit)

## Verification
- `just verify` passed before release commit.
- `pnpm lint` (apps/web) passed after scoped Biome overrides for upstream PlateJS-derived files.

## Commits Pushed (oldest -> newest)
- `df0f0ffe` feat(web): add assistant_mode to ALLOWED_FLAGS (1 file)
- `05d13ba5` test(services): align scrape payload contract assertion (1 file)
- `9c7e6a5f` feat(web): add assistant_mode to DirectParams and extract from flags (3 files)
- `c2d414c8` feat(web): use assistant CWD when assistant_mode=true (2 files)
- `e7271b23` feat(web): add assistant sessions API route and scanner (2 files)
- `17a6d231` feat(web): add assistant rail mode to config (1 file)
- `c54de559` feat(web): render assistant session list in sidebar (1 file)
- `aef2014f` test(web): fix cortex route mock arg typing (1 file)
- `93537231` feat(web): wire assistant mode sessions through shell and ACP (4 files)
- `98e7b96e` feat(release): ship assistant mode and stabilize verification gates (v0.18.0) (73 files)
- `5682daa2` fix(mcp): align config path to mcp.json across web/api/docs (7 files)

## Docs Updated
- `CHANGELOG.md` (highlights + commit table refresh)
- `docs/REBOOT-UI.md` (assistant rail mode and assistant_mode/CWD behavior)

## Push
- Remote: `git@github.com:jmagar/axon.git`
- Destination: `origin/feat/github-code-aware-chunking`
- Range: `4fdc70be` -> `5682daa2`
