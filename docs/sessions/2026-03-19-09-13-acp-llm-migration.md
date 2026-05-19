# ACP LLM Migration Session Log
Date: 2026-03-19

## Scope Completed
- Added shared ACP LLM gateway (`crates/services/acp_llm.rs`) and exported it in `crates/services.rs`.
- Migrated vector ask/evaluate paths to ACP-backed completion wrappers.
- Migrated suggest command generation path to ACP.
- Migrated extract fallback path to ACP (engine + deterministic path).
- Migrated debug service analysis path to ACP.
- Migrated research synthesis from spider-agent OpenAI completion to ACP while preserving Tavily search.
- Updated configuration and docs to treat ACP adapter as required for migrated commands.

## Key Decisions
- Centralized completion calls through `acp_llm` so ask/evaluate/suggest/extract fallback/debug/research all use one adapter contract.
- Preserved `OPENAI_MODEL` as the compatibility model-override key for ACP calls.
- Treated `OPENAI_BASE_URL` / `OPENAI_API_KEY` as legacy/compatibility settings in docs and config comments.
- Kept Tavily via `spider_agent` for discovery; only synthesis moved to ACP.

## Usage / Token Accounting Behavior
- ACP usage is optional.
- When usage is missing from adapter events, migrated paths default token fields to `0` and continue.
- Extract fallback and research synthesis preserve prior payload shapes with graceful zero/default usage behavior.

## Runtime / Concurrency Notes
- ACP completion futures are `!Send` due LocalSet-based runtime internals.
- Send-constrained call sites (`extract` fallback task queue, `debug` service, `research` synthesis service) now execute ACP completion inside `spawn_blocking` + `current_thread` runtime wrappers.

## Verification Summary
- Targeted tests executed and passing:
  - `cargo test services_acp_llm -- --nocapture`
  - `cargo test services_query_services -- --nocapture`
  - `cargo test services_discovery_services -- --nocapture`
  - `cargo test streaming:: -- --nocapture`
  - `cargo test suggest:: -- --nocapture`
  - `cargo test deterministic:: -- --nocapture`
  - `cargo test debug:: -- --nocapture`
  - `cargo test search:: -- --nocapture`
  - `cargo test run_research_ -- --nocapture`
- Full gate:
  - `just verify` (passed)

## Manual Smoke Checks
- Ran command smoke checks for `ask`, `evaluate`, `suggest`, `debug`, `research`.
- In this environment they correctly fail fast with ACP configuration errors because `AXON_ACP_ADAPTER_CMD` is unset.
- This validates the new prereq guardrails for migrated commands.

## Residual Risks
- ACP adapter differences can affect response formatting; JSON-summary fallback parsing remains tolerant but may need adapter-specific tuning.
- `OPENAI_*` compatibility fields are still present; future cleanup should remove unused fields once all remaining legacy paths are migrated.
