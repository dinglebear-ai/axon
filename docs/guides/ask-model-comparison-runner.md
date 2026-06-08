# Ask Model Comparison Runner

`scripts/run-ask-model-comparison.sh` orchestrates repeatable `axon ask` comparisons across multiple LLM configurations. It runs the same question set against each selected profile, stores every answer, captures stderr separately, captures `axon ask --explain --diagnostics --json` retrieval traces, and writes a JSON run artifact with timing plus the effective Axon config for each model.

The runner was created for the indexed general-knowledge question set in `reports/llm-ask-comparison-2026-06-07/questions-indexed-general.md`, but it can run any markdown file that follows the same `## Questions` / `## Answer Key` structure.

## Quick Start

```bash
# Validate script behavior without network or model calls.
scripts/run-ask-model-comparison.sh --self-test

# Check which questions/models/output path would be used.
scripts/run-ask-model-comparison.sh --dry-run --skip-preflight

# Run the full default comparison.
scripts/run-ask-model-comparison.sh
```

The full default run executes 10 questions against five profiles. With explain capture enabled, that is 50 answer calls plus 50 explain calls, for 100 total `axon ask` invocations. With `--no-explain`, it runs only the 50 answer calls.

## Prerequisites

- `bash`
- `jq`
- `awk`, `sed`, `date`, and `mktemp`
- a runnable Axon binary
- populated Axon config and secrets, normally in `~/.axon/.env`
- reachable Qdrant and TEI services for retrieval
- reachable LLM providers for the selected profiles
- for `gemma-local`, a llama.cpp OpenAI-compatible server reachable at `http://127.0.0.1:8080/v1` by default

The script resolves the Axon executable in this order:

1. `--axon-bin PATH`
2. `AXON_BIN`
3. `target/release/axon`
4. `scripts/axon`
5. `axon` on `PATH`

For reliable comparisons, prefer a fresh release build:

```bash
cargo build --release --bin axon
```

## Question File Format

The runner extracts questions only from the numbered list between `## Questions` and `## Answer Key`.

Example:

```markdown
## Questions

1. First question?

2. Second question?

## Answer Key
```

Each extracted question receives an ID based on list order: `Q01`, `Q02`, and so on. Multi-line questions are not currently supported; keep each question on one numbered line.

Default question file:

```text
reports/llm-ask-comparison-2026-06-07/questions-indexed-general.md
```

## Profiles

The default profile list is:

```text
current,gemini-flash,gpt-5.4-mini,gemini-3.1-flash-lite,gemma-local
```

Use `--models` to run a subset:

```bash
scripts/run-ask-model-comparison.sh --models current,gemma-local
```

Profiles run in parallel by default, with each profile answering the question set sequentially. Use `--serial` when you want one provider/model at a time.

Explain traces are captured by default before each answer call. Use `--no-explain` only when you intentionally want the lighter answer-only run.

Each `QNN.explain.json` includes the exact context block Axon would inject into synthesis at:

```bash
jq -r '.explain.context.rendered_context' QNN.explain.json
```

When explain capture is enabled, each explain trace must be nonempty valid JSON with `.explain.context`. Invalid explain traces are recorded in `run.json` with `explain_valid: false`, included in the final failure count, and cause the runner to exit nonzero after writing the accountable run artifact.

The runner rejects duplicate profile names and duplicate computed labels before running. Computed labels include model-name overrides after slugification, so two override profiles that collapse to the same output directory label fail preflight instead of sharing files.

### `current`

Runs `axon ask` with the active environment exactly as Axon would normally load it.

No temporary override env file is used. The script records the effective config by running:

```bash
axon config list --json
```

The JSON label for this profile is `current-config`.

### `gemini-flash`

Copies the base env file, removes existing LLM and ask-tuning keys, then applies these overrides:

```text
AXON_LLM_BACKEND=openai-compat
AXON_OPENAI_BASE_URL=${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}
AXON_OPENAI_MODEL=${GEMINI_FLASH_MODEL:-gemini-3.5-flash-low}
AXON_LLM_COMPLETION_CONCURRENCY=1
```

The default JSON label for this profile is `cli-api-gemini-3.5-flash-low`.

### `gpt-5.4-mini`

Copies the base env file, removes existing LLM and ask-tuning keys, then applies these overrides:

```text
AXON_LLM_BACKEND=openai-compat
AXON_OPENAI_BASE_URL=${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}
AXON_OPENAI_MODEL=${GPT_5_4_MINI_MODEL:-gpt-5.4-mini}
AXON_LLM_COMPLETION_CONCURRENCY=1
```

The default JSON label for this profile is `cli-api-gpt-5.4-mini`.

### `gemini-3.1-flash-lite`

Copies the base env file, removes existing LLM and ask-tuning keys, then applies these overrides:

```text
AXON_LLM_BACKEND=openai-compat
AXON_OPENAI_BASE_URL=${CLI_API_BASE_URL:-https://cli-api.tootie.tv/v1}
AXON_OPENAI_MODEL=${GEMINI_3_1_FLASH_LITE_MODEL:-gemini-3.1-flash-lite}
AXON_LLM_COMPLETION_CONCURRENCY=1
```

The default JSON label for this profile is `cli-api-gemini-3.1-flash-lite`.

### `gemma-local`

Copies the base env file, removes existing LLM and ask-tuning keys, then applies conservative local-model overrides:

```text
AXON_LLM_BACKEND=openai-compat
AXON_OPENAI_BASE_URL=${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}
AXON_OPENAI_MODEL=${GEMMA_MODEL:-ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M}
AXON_OPENAI_API_KEY=
AXON_LLM_COMPLETION_CONCURRENCY=1
AXON_ASK_MAX_CONTEXT_CHARS=${GEMMA_CONTEXT_CHARS:-300000}
AXON_ASK_CHUNK_LIMIT=${GEMMA_CHUNK_LIMIT:-20}
AXON_ASK_CANDIDATE_LIMIT=${GEMMA_CANDIDATE_LIMIT:-120}
AXON_ASK_HYBRID_CANDIDATES=${GEMMA_HYBRID_CANDIDATES:-100}
AXON_ASK_DOC_FETCH_CONCURRENCY=${GEMMA_DOC_FETCH_CONCURRENCY:-1}
```

The JSON label for this profile is `llamacpp-gemma-4-e4b-q4`.

Before running this profile, the script checks:

```text
${GEMMA_OPENAI_BASE_URL:-http://127.0.0.1:8080/v1}/models
```

Skip that check with `--skip-preflight`.

## llama.cpp Setup

The local Gemma profile expects the llama.cpp compose service to be online. From the repo root:

```bash
LLAMA_CPP_HF_MODEL=ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M \
docker compose --env-file ~/.axon/.env -f docker-compose.llama.yaml up -d
```

Useful checks:

```bash
curl -fsS http://127.0.0.1:8080/health
curl -fsS http://127.0.0.1:8080/v1/models | jq
```

The compose default context is `131072` tokens. The runner caps Axon ask context for Gemma with `AXON_ASK_MAX_CONTEXT_CHARS=300000` unless overridden.

## Base Env Handling

Override profiles are built from a temporary copy of the base env file.

Default base env:

```text
~/.axon/.env
```

Override it with either:

```bash
scripts/run-ask-model-comparison.sh --base-env /path/to/env
```

or:

```bash
AXON_BASE_ENV_FILE=/path/to/env scripts/run-ask-model-comparison.sh
```

For override profiles, the script removes these keys from the copied env before appending profile-specific values:

```text
AXON_LLM_BACKEND
AXON_OPENAI_BASE_URL
AXON_OPENAI_MODEL
AXON_OPENAI_API_KEY
AXON_ASK_*
AXON_LLM_COMPLETION_*
```

Temporary env files are created under `mktemp -d`, chmodded `600`, and removed on exit. They are not written into the report directory.

Remote `cli-api` override profiles preserve the base `AXON_OPENAI_API_KEY` by copying it from the base env file after removing stale LLM keys. `run.json` records that preserved key only as `***` in `env_overrides`; the raw value is not written to runner metadata.

## Labels and Paths

Profile output directories are derived from profile labels. Labels from environment-controlled model names are slugified before use, so a model override such as `../bad/model` becomes a safe directory name like `cli-api-bad-model`.

The original provider and model strings are still recorded in `run.json`; only the filesystem label is normalized.

Labels must be unique after slugification. For example, if two model overrides both produce `cli-api-same-model`, the runner exits before creating a misleading result set.

## Output Layout

Default output directory:

```text
reports/llm-ask-comparison-2026-06-07/run-YYYYmmdd-HHMMSS/
```

Example layout:

```text
run-20260607-190000/
  README.md
  questions.tsv
  run.json
  current-config/
    Q01.md
    Q01.stderr.log
    Q01.explain.json
    Q01.explain.stderr.log
    ...
  cli-api-gemini-3.5-flash-low/
    Q01.md
    Q01.stderr.log
    ...
  cli-api-gpt-5.4-mini/
    Q01.md
    Q01.stderr.log
    ...
  cli-api-gemini-3.1-flash-lite/
    Q01.md
    Q01.stderr.log
    ...
  llamacpp-gemma-4-e4b-q4/
    Q01.md
    Q01.stderr.log
    ...
```

`questions.tsv` contains the extracted question IDs and text. It is a convenience file, not the timing artifact.

`run.json` is the canonical machine-readable output. Each result includes answer timing and, when explain capture is enabled, `explain_elapsed_seconds`, `explain_exit_code`, `explain_valid`, `explain_error`, `explain_file`, and `explain_stderr_file`.

Raw answer markdown, stderr logs, and explain JSON traces can include retrieved source snippets, internal URLs, local paths, and provider diagnostics. Treat raw run directories as review-before-commit artifacts. Commit or share only after checking whether those traces should be redacted.

## Terminal Progress

The script prints human-readable progress to stderr while keeping stdout reserved for the final output directory path. This makes it easy to capture the path programmatically without losing live progress in the terminal.

At startup it prints a plan summary:

```text
Planned comparison run
  axon: /home/jmagar/workspace/axon/target/release/axon
  questions: /home/jmagar/workspace/axon/reports/.../questions-indexed-general.md (10 questions)
  out_dir: /home/jmagar/workspace/axon/reports/.../run-YYYYmmdd-HHMMSS
  models: current,gemini-flash,gpt-5.4-mini,gemini-3.1-flash-lite,gemma-local
```

Before each profile runs, it prints the selected provider, model, and relevant model settings from the captured effective Axon config:

```text
running profile: http://127.0.0.1:8080/v1 / ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M
  label: llamacpp-gemma-4-e4b-q4
  settings: backend=openai-compat, completion_concurrency=1, ask_max_context_chars=300000, ask_chunk_limit=20, ask_candidate_limit=120, ask_hybrid_candidates=100
```

Each question prints a start line and a completion line with elapsed wall-clock time:

```text
  Q01: starting
  Q01: explain=0.138s explain_exit=0 explain_valid=1 answer=9.234s exit=0 file=/home/jmagar/workspace/axon/reports/.../llamacpp-gemma-4-e4b-q4/Q01.md
```

At the end it prints a compact summary:

```text
Run complete
  out_dir: /home/jmagar/workspace/axon/reports/.../run-YYYYmmdd-HHMMSS
  run_json: /home/jmagar/workspace/axon/reports/.../run-YYYYmmdd-HHMMSS/run.json
  result_count: 50
  failures: 0
```

## `run.json` Schema

Top-level shape:

```json
{
  "schema": "axon-ask-model-comparison/v2",
  "created_at": "2026-06-07T19:00:00-04:00",
  "questions_file": "/home/jmagar/workspace/axon/reports/...",
  "out_dir": "/home/jmagar/workspace/axon/reports/...",
  "axon_bin": "/home/jmagar/workspace/axon/target/release/axon",
  "capture_explain": true,
  "result_schema_features": [
    "per_result_explain_valid",
    "per_result_explain_error",
    "top_level_capture_explain",
    "execution_mode"
  ],
  "execution_mode": "parallel",
  "selected_models": ["current", "gemini-flash", "gemma-local"],
  "profiles": [],
  "results": []
}
```

### `profiles[]`

Each profile records both the intended overrides and Axon's effective config snapshot:

```json
{
  "profile": "gemma-local",
  "label": "llamacpp-gemma-4-e4b-q4",
  "provider": "http://127.0.0.1:8080/v1",
  "model": "ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M",
  "env_overrides": {
    "AXON_LLM_BACKEND": "openai-compat",
    "AXON_OPENAI_BASE_URL": "http://127.0.0.1:8080/v1",
    "AXON_OPENAI_MODEL": "ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M",
    "AXON_OPENAI_API_KEY": "",
    "AXON_ASK_MAX_CONTEXT_CHARS": "300000"
  },
  "effective_config": {
    "env": {
      "AXON_LLM_BACKEND": "openai-compat",
      "AXON_OPENAI_API_KEY": "***"
    },
    "toml": {}
  }
}
```

`effective_config` comes from `axon config list --json`. Axon redacts known secret values as `***`. The runner itself does not manually inspect or publish secret values.

Effective config capture is a preflight requirement. If `axon config list --json` exits nonzero or returns invalid JSON for any profile, the runner exits before finalizing `run.json`.

### `results[]`

Each result represents one model/question invocation:

```json
{
  "question_id": "Q01",
  "question": "In Bun's package manager, ...",
  "profile": "gemma-local",
  "profile_label": "llamacpp-gemma-4-e4b-q4",
  "provider": "http://127.0.0.1:8080/v1",
  "model": "ggml-org/gemma-4-E4B-it-GGUF:Q4_K_M",
  "explain_started_at": "2026-06-07T19:00:00-04:00",
  "explain_finished_at": "2026-06-07T19:00:01-04:00",
  "explain_elapsed_seconds": 0.138,
  "explain_exit_code": 0,
  "explain_valid": true,
  "explain_error": null,
  "explain_file": "/home/jmagar/workspace/axon/reports/.../Q01.explain.json",
  "explain_stderr_file": "/home/jmagar/workspace/axon/reports/.../Q01.explain.stderr.log",
  "started_at": "2026-06-07T19:00:01-04:00",
  "finished_at": "2026-06-07T19:00:10-04:00",
  "elapsed_seconds": 9.123,
  "exit_code": 0,
  "stdout_file": "/home/jmagar/workspace/axon/reports/.../Q01.md",
  "stderr_file": "/home/jmagar/workspace/axon/reports/.../Q01.stderr.log"
}
```

The answer body itself is stored in `stdout_file`. The JSON stores paths and timing, not duplicated full answer text. When `--no-explain` is used, all explain-specific result fields are `null`, `capture_explain` is `false`, and no `QNN.explain.json` files are written.

## Answer Markdown

Each `QNN.md` file starts with a small run header:

```markdown
## Q01

**Question:** ...

**Provider:** `...`
**Model:** `...`
**Elapsed:** `...s`
**Exit code:** `0`

---

<axon ask output>
```

This makes individual answer files readable without opening `run.json`.

## Common Commands

Run only the current configuration:

```bash
scripts/run-ask-model-comparison.sh --models current
```

Run only local Gemma, skipping endpoint preflight:

```bash
scripts/run-ask-model-comparison.sh --models gemma-local --skip-preflight
```

Use a custom question file:

```bash
scripts/run-ask-model-comparison.sh --questions reports/my-questions.md
```

Write to a fixed output directory:

```bash
scripts/run-ask-model-comparison.sh --out-dir reports/llm-ask-comparison-2026-06-07/run-manual
```

Use a different Axon binary:

```bash
scripts/run-ask-model-comparison.sh --axon-bin target/debug/axon
```

Override Gemma context and model:

```bash
GEMMA_MODEL=ggml-org/gemma-4-E4B-it-GGUF:Q5_K_M \
GEMMA_CONTEXT_CHARS=250000 \
scripts/run-ask-model-comparison.sh --models gemma-local
```

Inspect elapsed times:

```bash
jq -r '.results[] | [.profile_label, .question_id, .elapsed_seconds, .exit_code] | @tsv' \
  reports/llm-ask-comparison-2026-06-07/run-*/run.json
```

Inspect effective selected model config:

```bash
jq '.profiles[] | {label, provider, model, env_overrides, effective_env: .effective_config.env}' \
  reports/llm-ask-comparison-2026-06-07/run-*/run.json
```

## Verification

The script includes a no-network self-test:

```bash
scripts/run-ask-model-comparison.sh --self-test
```

The self-test creates a temporary fake Axon binary, runs two questions across all five default profiles, verifies `run.json`, verifies that model configs are present, verifies answer markdown and explain traces exist, verifies dynamic model labels are slugified, verifies `--no-explain` serial JSON shape, verifies explain failure accounting, rejects duplicate profiles and duplicate labels, verifies config-capture failure prevents finalizing misleading `run.json`, and verifies env files plus a fake base-env secret are not leaked into the report output.

Also run:

```bash
bash -n scripts/run-ask-model-comparison.sh
```

## Troubleshooting

### `llama.cpp OpenAI-compatible endpoint is not reachable`

Start the llama compose stack and verify `/v1/models`:

```bash
docker compose --env-file ~/.axon/.env -f docker-compose.llama.yaml up -d
curl -fsS http://127.0.0.1:8080/v1/models | jq
```

Use `--skip-preflight` only if the endpoint is intentionally unavailable during planning or if the profile is not actually being run.

### `outdated axon binary`

Rebuild the binary:

```bash
cargo build --release --bin axon
```

Then rerun the comparison. This keeps stderr logs cleaner and ensures prompt/config changes are included.

### `no questions found`

Confirm the question document has:

```markdown
## Questions

1. ...

## Answer Key
```

The parser stops at `## Answer Key`.

### A profile failed for every question

Check that profile's `QNN.stderr.log` files first, then inspect the profile's `effective_config` in `run.json`. The most common causes are wrong `AXON_OPENAI_BASE_URL`, missing API key for remote provider, llama.cpp not running, or Qdrant/TEI unavailable.

### Secrets in output

`run.json` records `axon config list --json`, which redacts known secrets. The runner writes temporary env files outside the output directory and deletes them on exit. Do not change the script to write env files into `reports/`.
