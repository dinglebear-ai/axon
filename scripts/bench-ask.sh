#!/usr/bin/env bash
# bench-ask.sh — End-to-end perf harness for `axon ask` across ACP backends.
#
# Statistical rigor (see docs/perf/README.md):
#   - --runs >= 30 (p50 stable), 100+ for p95, 500+ for p99
#   - Bootstrap percentile CI (1000 resamples) per timing field
#   - First 3 warm-mode runs discarded as warmup
#   - LLM bucket variance is high (20-40%); look at retrieval/context buckets for stable signal
#
# Output JSON contains ONLY numerical measurements — no chunk_text, queries, answers, urls.
#
# Prerequisites:
#   - axon binary built (`just build` or `cargo build --release --bin axon`)
#   - Infra services up (`just services-up` — qdrant, tei, chrome)
#   - For warm mode: `axon serve` running on AXON_MCP_HTTP_HOST:PORT
#   - jq, bash 4+

set -euo pipefail

# ── Defaults ─────────────────────────────────────────────────────────────────
BACKEND="acp"
AGENT="all"
RUNS=30
MODE="both"
PROMPTS_FILE=""
OUT=""
WARMUP_DISCARD=3
BOOTSTRAP_RESAMPLES=1000

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Help ─────────────────────────────────────────────────────────────────────
usage() {
    cat <<'EOF'
Usage: bench-ask.sh [options]

Options:
  --backend {acp|headless|all}   Backend type (default: acp)
  --agent {claude|codex|gemini|all}  Agent (default: all)
  --runs N                       Sample count per (agent, prompt, mode); min 30 (default: 30)
  --mode {cold|warm|both}        Cold restarts the in-process state per run; warm reuses serve (default: both)
  --prompts <file>               Prompt file (id|text per line; default: scripts/bench-prompts.txt)
  --out <path>                   Output JSON path (default: docs/perf/results-<timestamp>-<sha>.json)
  -h, --help                     Show this help

Sample-size guidance:
  - p50: --runs 30 (CI ±10%)
  - p95: --runs 100  (CI ±10%)
  - p99: --runs 500+ (CI ±10%)

Examples:
  rtk bash scripts/bench-ask.sh --runs 30 --mode warm
  rtk bash scripts/bench-ask.sh --runs 100 --agent claude --mode both
EOF
}

# ── Parse args ───────────────────────────────────────────────────────────────
while [[ $# -gt 0 ]]; do
    case "$1" in
        --backend) BACKEND="$2"; shift 2 ;;
        --agent) AGENT="$2"; shift 2 ;;
        --runs) RUNS="$2"; shift 2 ;;
        --mode) MODE="$2"; shift 2 ;;
        --prompts) PROMPTS_FILE="$2"; shift 2 ;;
        --out) OUT="$2"; shift 2 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown option: $1" >&2; usage; exit 2 ;;
    esac
done

# ── Validate ─────────────────────────────────────────────────────────────────
if [[ "$RUNS" =~ ^[0-9]+$ ]]; then
    if [[ "$RUNS" -lt 30 ]]; then
        echo "Error: --runs must be >= 30 for stable p50; for stable p95 use --runs 100+; for stable p99 use --runs 500+. See docs/perf/README.md." >&2
        exit 2
    fi
else
    echo "Error: --runs must be a positive integer." >&2
    exit 2
fi

case "$BACKEND" in
    acp|headless|all) ;;
    *) echo "Error: --backend must be one of {acp|headless|all}. Got: $BACKEND" >&2; exit 2 ;;
esac

case "$MODE" in
    cold|warm|both) ;;
    *) echo "Error: --mode must be one of {cold|warm|both}. Got: $MODE" >&2; exit 2 ;;
esac

case "$AGENT" in
    claude|codex|gemini|all) ;;
    *) echo "Error: --agent must be one of {claude|codex|gemini|all}. Got: $AGENT" >&2; exit 2 ;;
esac

# ── Tooling check ────────────────────────────────────────────────────────────
command -v jq >/dev/null 2>&1 || { echo "Error: jq not found in PATH." >&2; exit 2; }

if [[ -z "$PROMPTS_FILE" ]]; then
    PROMPTS_FILE="$SCRIPT_DIR/bench-prompts.txt"
fi
if [[ ! -f "$PROMPTS_FILE" ]]; then
    echo "Error: prompts file not found: $PROMPTS_FILE" >&2
    exit 2
fi

# Locate axon binary
AXON_BIN="${AXON_BIN:-}"
if [[ -z "$AXON_BIN" ]]; then
    if [[ -x "$REPO_ROOT/target/release/axon" ]]; then
        AXON_BIN="$REPO_ROOT/target/release/axon"
    elif [[ -x "$REPO_ROOT/target/debug/axon" ]]; then
        AXON_BIN="$REPO_ROOT/target/debug/axon"
    elif command -v axon >/dev/null 2>&1; then
        AXON_BIN="$(command -v axon)"
    else
        echo "Error: axon binary not found. Build with 'just build' or set AXON_BIN." >&2
        exit 2
    fi
fi

if [[ -z "$OUT" ]]; then
    TS="$(date +%Y-%m-%d-%H%M%S)"
    SHA="$(git -C "$REPO_ROOT" rev-parse --short HEAD 2>/dev/null || echo nogit)"
    mkdir -p "$REPO_ROOT/docs/perf"
    OUT="$REPO_ROOT/docs/perf/results-${TS}-${SHA}.json"
fi

# ── Determine agents ─────────────────────────────────────────────────────────
if [[ "$AGENT" == "all" ]]; then
    AGENTS=(claude codex gemini)
else
    AGENTS=("$AGENT")
fi

if [[ "$MODE" == "both" ]]; then
    MODES=(cold warm)
else
    MODES=("$MODE")
fi

if [[ "$BACKEND" == "all" ]]; then
    BACKENDS=(acp headless)
else
    BACKENDS=("$BACKEND")
fi

# ── Bootstrap percentile helper (jq) ─────────────────────────────────────────
# Computes p50/p95/p99 with a 95% bootstrap CI by resampling with replacement.
# Input: array of numbers (samples). Output: object with p50/p95/p99 + CI lo/hi.
bootstrap_percentiles() {
    local samples_json="$1"
    local resamples="$2"
    jq -n --argjson s "$samples_json" --argjson r "$resamples" '
        def pct(arr; p):
            (arr | sort) as $sorted
            | ($sorted | length) as $n
            | if $n == 0 then null
              else
                ((p / 100) * ($n - 1)) as $idx
                | ($idx | floor) as $lo
                | ($idx | ceil) as $hi
                | if $lo == $hi then $sorted[$lo]
                  else
                    ($idx - $lo) as $w
                    | ($sorted[$lo] * (1 - $w)) + ($sorted[$hi] * $w)
                  end
              end;
        # Use a simple LCG for deterministic-ish resampling without /dev/urandom
        def lcg(seed):
            (((seed * 1103515245) + 12345) % 2147483648);
        def resample(arr; seed):
            (arr | length) as $n
            | reduce range(0; $n) as $i ([seed, []];
                .[0] as $cur
                | lcg($cur) as $next
                | [$next, (.[1] + [arr[($next % $n)]])])
            | .[1];
        def boot(arr; reps; p):
            [reduce range(0; reps) as $i ([12345, []];
                .[0] as $cur
                | lcg($cur) as $next
                | [$next, (.[1] + [pct(resample(arr; $next); p)])])
            | .[1] | .[]] | sort;
        def ci(boots):
            (boots | length) as $n
            | { lo: boots[(($n * 0.025) | floor)],
                hi: boots[(($n * 0.975) | floor)] };
        ($s | length) as $n
        | if $n == 0 then
            { count: 0, p50: null, p95: null, p99: null,
              p50_ci: null, p95_ci: null, p99_ci: null }
          else
            (boot($s; $r; 50)) as $b50
            | (boot($s; $r; 95)) as $b95
            | (boot($s; $r; 99)) as $b99
            | { count: $n,
                p50: pct($s; 50),
                p95: pct($s; 95),
                p99: pct($s; 99),
                p50_ci: ci($b50),
                p95_ci: ci($b95),
                p99_ci: ci($b99) }
          end
    '
}

# ── Sanitizer: strict allowlist of numerical fields ──────────────────────────
# Walks JSON, ensures strings are numeric and <= 200 chars.
# Forbids top-level forbidden keys: query, answer, chunk_text, url, source.
validate_artifact() {
    local file="$1"
    local violations
    violations=$(jq -r '
        def forbidden_keys: ["query", "answer", "chunk_text", "url", "source"];
        def walk(f): . as $in
            | if type == "object" then
                reduce (keys_unsorted[]) as $k ({}; . + { ($k): ($in[$k] | walk(f)) })
              elif type == "array" then map(walk(f))
              else f end;
        [paths(strings) as $p
         | (getpath($p)) as $v
         | select(($v | tonumber? | not))
         | { path: $p | join("."), value: $v }] as $non_numeric_strings
        | [paths(strings) as $p
         | (getpath($p)) as $v
         | select(($v | length) > 200)
         | { path: $p | join("."), len: ($v | length) }] as $long_strings
        | [paths | select(.[-1] as $k | forbidden_keys | index($k))] as $forbidden_paths
        | { non_numeric_strings: $non_numeric_strings, long_strings: $long_strings, forbidden_paths: $forbidden_paths }
    ' "$file")
    local non_numeric_count long_count forbidden_count
    non_numeric_count=$(jq '.non_numeric_strings | length' <<<"$violations")
    long_count=$(jq '.long_strings | length' <<<"$violations")
    forbidden_count=$(jq '.forbidden_paths | length' <<<"$violations")
    if [[ "$non_numeric_count" -gt 0 || "$long_count" -gt 0 || "$forbidden_count" -gt 0 ]]; then
        echo "Error: bench artifact validation failed:" >&2
        echo "$violations" | jq . >&2
        return 1
    fi
}

# ── Run a single ask, parse timing_ms ────────────────────────────────────────
# Outputs JSON of {field: ms} for each timing_ms key. Errors -> empty {}.
run_one_ask() {
    local prompt_text="$1"
    local agent="$2"
    local backend="$3"
    local mode="$4"
    local extra_args=()

    if [[ "$mode" == "cold" ]]; then
        extra_args+=(--no-server-url)
    else
        if [[ -z "${AXON_ASK_SERVER_URL:-}" ]]; then
            echo "Error: warm mode requires AXON_ASK_SERVER_URL to point at a running axon serve instance." >&2
            return 2
        fi
        extra_args+=(--server-url "$AXON_ASK_SERVER_URL")
    fi

    # AXON_ACP_ADAPTER_CMD is set per-agent for ACP cells. Headless cells
    # select by AXON_ASK_AGENT and must fail closed when an agent is unsafe.
    local adapter
    case "$agent" in
        claude) adapter="claude-agent-acp" ;;
        codex)  adapter="codex" ;;
        gemini) adapter="gemini" ;;
        *) adapter="" ;;
    esac

    local out
    if ! out=$(AXON_ASK_BACKEND="$backend" \
             AXON_ASK_AGENT="$agent" \
             AXON_ACP_ADAPTER_CMD="$adapter" \
             "$AXON_BIN" ask "$prompt_text" --json "${extra_args[@]}" 2>/dev/null); then
        echo "{}"
        return 0
    fi

    # Extract just the timing_ms object (numerical only).
    jq -c '.timing_ms // {}' <<<"$out" 2>/dev/null || echo "{}"
}

# ── Main loop ────────────────────────────────────────────────────────────────
echo "bench-ask: backend=$BACKEND agent=$AGENT runs=$RUNS mode=$MODE prompts=$PROMPTS_FILE" >&2
echo "bench-ask: out=$OUT" >&2
echo "bench-ask: axon=$AXON_BIN" >&2

declare -a RESULTS_JSON=()

while IFS='|' read -r prompt_id prompt_text || [[ -n "$prompt_id" ]]; do
    [[ -z "$prompt_id" || "$prompt_id" =~ ^# ]] && continue
    for backend in "${BACKENDS[@]}"; do
      for agent in "${AGENTS[@]}"; do
        for mode in "${MODES[@]}"; do
            echo "→ $backend / $agent / $prompt_id / $mode  (n=$RUNS)" >&2
            timings_array="[]"
            kept=0
            warmup_count=0
            target_warmup=0
            if [[ "$mode" == "warm" && "$backend" == "acp" ]]; then
                target_warmup="$WARMUP_DISCARD"
            fi

            for ((i=0; i<RUNS + target_warmup; i++)); do
                t_json=$(run_one_ask "$prompt_text" "$agent" "$backend" "$mode")
                if [[ "$warmup_count" -lt "$target_warmup" ]]; then
                    warmup_count=$((warmup_count + 1))
                    continue
                fi
                if [[ "$t_json" == "{}" ]]; then
                    continue
                fi
                timings_array=$(jq -c --argjson t "$t_json" '. + [$t]' <<<"$timings_array")
                kept=$((kept + 1))
            done

            # Pivot: per-field samples
            metrics_json=$(jq -n --argjson all "$timings_array" '
                $all | map(keys) | flatten | unique
            ')

            per_field=$(jq -n --argjson all "$timings_array" '
                ($all | map(keys) | flatten | unique) as $fields
                | reduce $fields[] as $f ({};
                    .[$f] = ($all | map(.[$f] // null) | map(select(. != null and (type == "number")))))
            ')

            # Bootstrap each field
            metrics="{}"
            for field in $(jq -r 'keys[]' <<<"$per_field"); do
                samples=$(jq -c --arg f "$field" '.[$f]' <<<"$per_field")
                stats=$(bootstrap_percentiles "$samples" "$BOOTSTRAP_RESAMPLES")
                metrics=$(jq --arg f "$field" --argjson s "$stats" '. + { ($f): $s }' <<<"$metrics")
            done

            entry=$(jq -n \
                --arg backend "$backend" \
                --arg agent "$agent" \
                --arg prompt_id "$prompt_id" \
                --arg mode "$mode" \
                --arg status "$(if [[ "$kept" -gt 0 ]]; then echo measured; else echo unavailable_or_failed; fi)" \
                --argjson samples "$kept" \
                --argjson warmup "$target_warmup" \
                --argjson runs_requested "$RUNS" \
                --argjson metrics "$metrics" \
                '{
                    backend: $backend,
                    agent: $agent,
                    prompt_id: $prompt_id,
                    mode: $mode,
                    status: $status,
                    runs_requested: $runs_requested,
                    samples: $samples,
                    warmup_discarded: $warmup,
                    metrics: $metrics
                }')
            RESULTS_JSON+=("$entry")
        done
      done
    done
done < "$PROMPTS_FILE"

# ── Assemble final report ────────────────────────────────────────────────────
final_results="[]"
for r in "${RESULTS_JSON[@]}"; do
    final_results=$(jq --argjson r "$r" '. + [$r]' <<<"$final_results")
done

GIT_SHA="$(git -C "$REPO_ROOT" rev-parse HEAD 2>/dev/null || echo unknown)"
GIT_BRANCH="$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo unknown)"
TIMESTAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"

jq -n \
    --arg git_sha "$GIT_SHA" \
    --arg git_branch "$GIT_BRANCH" \
    --arg timestamp "$TIMESTAMP" \
    --arg backend "$BACKEND" \
    --argjson runs "$RUNS" \
    --argjson warmup "$WARMUP_DISCARD" \
    --argjson resamples "$BOOTSTRAP_RESAMPLES" \
    --argjson results "$final_results" \
    '{
        schema: "axon-bench-ask/v1",
        timestamp_utc: $timestamp,
        git_sha: $git_sha,
        git_branch: $git_branch,
        backend: $backend,
        runs_per_combination: $runs,
        warmup_discarded_per_warm_combination: $warmup,
        bootstrap_resamples: $resamples,
        results: $results
    }' > "$OUT"

# ── Validate output: numerical-only enforcement ──────────────────────────────
if ! validate_artifact "$OUT"; then
    echo "Error: refusing to keep artifact with non-numerical fields. Removing $OUT." >&2
    rm -f "$OUT"
    exit 1
fi

echo "✓ wrote $OUT" >&2
echo "$OUT"
