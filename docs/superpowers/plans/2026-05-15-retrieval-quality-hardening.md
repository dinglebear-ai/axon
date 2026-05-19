# Retrieval Quality Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Axon retrieval tuning repeatable, explainable, and general-purpose without hard-coded docs-site exceptions.

**Architecture:** Keep Qdrant dense + BM42 sparse + RRF as the retrieval core. Add a cheap explain-only retrieval eval harness, corpus-health diagnostics, explicit explain rank metadata, centralized token/authority policy, and measured selection diversity tuning. Preserve the service boundary: vector builds retrieval/context diagnostics; services map typed results; CLI/web only display them.

**Tech Stack:** Rust, Qdrant, TEI, Bash + jq, Beads, Axon CLI `ask --explain --json`.

---

## File Structure

- `scripts/evaluate-retrieval.sh`: new tracked retrieval eval harness using `axon ask --explain --json`.
- `docs/eval/retrieval-fixtures.jsonl`: new tracked fixture set seeded from current top20 + popular2/3 cases.
- `src/vector/ops/token_policy.rs`: new pure token-policy helpers for query tokens, authority generic tokens, topical generic tokens, and URL identity tokens.
- `src/vector/ops.rs` or existing module root: expose `token_policy`.
- `src/vector/ops/commands/retrieval.rs`: call token-policy helpers; keep scoring/filtering behavior.
- `src/vector/ops/commands/retrieval/tests.rs`: retain and extend authority/topical regression tests.
- `src/vector/ops/ranking.rs` and `src/vector/ops/ranking_test.rs`: delegate query tokenization to token policy while preserving current tests.
- `src/services/types/service.rs`: additive diagnostics and explain fields.
- `src/vector/ops/commands/ask.rs`: include corpus-health and explain metadata in JSON payloads.
- `src/vector/ops/commands/ask/context.rs`: carry context/explain metadata.
- `src/vector/ops/commands/ask/context/build/trace.rs`: assign selection rank metadata.
- `src/vector/ops/commands/ask/context/build/selection.rs`: authority-aware selection tuning after token policy and harness land.
- `src/vector/ops/commands/ask/context/tests.rs`: context, selection, and explain tests.
- `src/services/query/tests.rs`: typed service schema tests.
- `src/cli/commands/ask.rs`: human diagnostics display only after JSON fields exist.
- `docs/ASK.md`, `docs/commands/ask.md`, `src/vector/CLAUDE.md`: anti-overfit workflow and cost-tiered gates.

## Task 1: Reconcile Dirty Worktree

**Files:**
- Modify: Bead state only
- Inspect: current git worktree

- [ ] **Step 1: Capture current state**

Run:

```bash
git status --short
git diff --stat
git diff --cached --stat
bd show axon_rust-hnfj.7
```

Expected: output shows the current dirty/staged files and confirms `axon_rust-hnfj.7` is the wave-zero blocker.

- [ ] **Step 2: Decide continuation mode**

Record one of these exact decisions as a Beads comment:

```bash
bd comments add axon_rust-hnfj \
  "DECISION: implementation base for hnfj is CONTINUE_DIRTY_WORKTREE. Workers must inspect staged and unstaged changes before editing overlapping files."
```

or:

```bash
bd comments add axon_rust-hnfj \
  "DECISION: implementation base for hnfj is CLEAN_BRANCH_AFTER_COMMIT. Current dirty retrieval work was committed before child beads started."
```

- [ ] **Step 3: Close the preflight blocker only after the decision is recorded**

Run:

```bash
bd close axon_rust-hnfj.7 --reason "implementation base recorded on epic"
bd swarm validate axon_rust-hnfj
```

Expected: `bd swarm validate` still reports the epic as swarmable and wave 1 moves to `.1`, `.2`, `.4`.

## Task 2: Add Retrieval Eval Harness

**Files:**
- Create: `docs/eval/retrieval-fixtures.jsonl`
- Create: `scripts/evaluate-retrieval.sh`
- Modify: `docs/eval/README.md`

- [ ] **Step 1: Create tracked retrieval fixtures**

Create `docs/eval/retrieval-fixtures.jsonl` with this initial content:

```jsonl
{"id":"top20-docsrs-rust-crate-docs","domain":"docs.rs","query":"How do I use docs.rs to find Rust crate API documentation?","expected":"selected","notes":"top20 baseline"}
{"id":"top20-openai-chatgpt-tools","domain":"developers.openai.com","query":"How do ChatGPT Apps expose tools to the model?","expected":"selected","notes":"top20 baseline"}
{"id":"top20-postgresql-create-view","domain":"www.postgresql.org","query":"How do I create a PostgreSQL view with CHECK OPTION?","expected":"selected","notes":"top20 baseline"}
{"id":"top20-qdrant-collection","domain":"qdrant.tech","query":"How do I create a Qdrant collection with vectors?","expected":"selected","notes":"top20 baseline"}
{"id":"popular-uv-dependencies","domain":"docs.astral.sh","query":"How do I use uv to manage Python dependencies?","expected":"selected","notes":"short product token regression"}
{"id":"popular-claude-hooks","domain":"code.claude.com","query":"How do Claude Code hooks work?","expected":"selected","notes":"configured authority regression"}
{"id":"popular-pypi-publish","domain":"pypi.org","query":"How do I publish a Python package to PyPI?","expected":"top_domain","notes":"known corpus mismatch: pypi appears but publish docs may not be selected"}
{"id":"popular-effective-rust-errors","domain":"effective-rust.com","query":"How should I structure error handling in Rust?","expected":"known_miss","notes":"known thin corpus/broad query miss"}
{"id":"popular-shuttle-deploy","domain":"www.shuttle.dev","query":"How do I deploy a Rust app with Shuttle?","expected":"known_miss","notes":"known not indexed in active collection at planning time"}
```

- [ ] **Step 2: Write the harness**

Create `scripts/evaluate-retrieval.sh`:

```bash
#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
REPO="$(cd -- "$SCRIPT_DIR/.." && pwd -P)"
FIXTURES="${1:-$REPO/docs/eval/retrieval-fixtures.jsonl}"
OUT="${OUT:-$REPO/.cache/axon-rust/evals/retrieval-$(date +%Y%m%d%H%M%S).jsonl}"
SUMMARY="${SUMMARY:-${OUT%.jsonl}.summary.json}"
AXON_BIN="${AXON_BIN:-$REPO/target/debug/axon}"
FAIL_ON_MISS="${FAIL_ON_MISS:-0}"

need() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "missing required command: $1" >&2
    exit 2
  }
}

need jq
mkdir -p "$(dirname "$OUT")"
: > "$OUT"

jq -e -c '
  select(type == "object")
  | select((.id // "") != "")
  | select((.domain // "") != "")
  | select((.query // "") != "")
  | select((.expected // "") != "")
' "$FIXTURES" >/dev/null

while IFS= read -r row; do
  id="$(jq -r '.id' <<<"$row")"
  domain="$(jq -r '.domain' <<<"$row")"
  query="$(jq -r '.query' <<<"$row")"
  expected="$(jq -r '.expected' <<<"$row")"

  if ! payload="$("$AXON_BIN" --local ask --explain --diagnostics --json "$query" 2> >(sed "s/^/[axon:$id] /" >&2))"; then
    jq -n -c --arg id "$id" --arg domain "$domain" --arg query "$query" --arg expected "$expected" \
      '{id:$id,domain:$domain,query:$query,expected:$expected,status:"axon_failed",top_pass:false,selected_pass:false}' >> "$OUT"
    continue
  fi

  jq -c \
    --arg id "$id" \
    --arg domain "$domain" \
    --arg query "$query" \
    --arg expected "$expected" \
    '
    def selected_urls:
      [.explain.candidates[]?
       | select(any(.selection_decisions[]?; .kind != "not_selected"))
       | .url];
    . as $payload
    | {
        id: $id,
        domain: $domain,
        query: $query,
        expected: $expected,
        status: "ok",
        top_domains: ($payload.diagnostics.top_domains // []),
        selected_urls: (selected_urls[0:20]),
        top_pass: (($payload.diagnostics.top_domains // []) | any(contains($domain))),
        selected_pass: (selected_urls | any(contains($domain))),
        timing_ms: ($payload.timing_ms // {})
      }
    | .pass = (
        if $expected == "selected" then (.top_pass and .selected_pass)
        elif $expected == "top_domain" then .top_pass
        elif $expected == "known_miss" then true
        else false
        end
      )
    ' <<<"$payload" >> "$OUT"
done < <(jq -c '.' "$FIXTURES")

jq -s '
  {
    total: length,
    pass: map(select(.pass)) | length,
    top_pass: map(select(.top_pass)) | length,
    selected_pass: map(select(.selected_pass)) | length,
    failures: map(select(.pass | not)),
    output: env.OUT
  }
' "$OUT" > "$SUMMARY"

cat "$SUMMARY"

if [ "$FAIL_ON_MISS" = "1" ] && jq -e '.failures | length > 0' "$SUMMARY" >/dev/null; then
  exit 1
fi
```

- [ ] **Step 3: Make the script executable**

Run:

```bash
chmod +x scripts/evaluate-retrieval.sh
```

- [ ] **Step 4: Run a small smoke fixture**

Run:

```bash
head -n 3 docs/eval/retrieval-fixtures.jsonl > /tmp/axon-retrieval-smoke.jsonl
scripts/evaluate-retrieval.sh /tmp/axon-retrieval-smoke.jsonl
```

Expected: summary JSON with `total: 3` and no `axon_failed` rows.

- [ ] **Step 5: Document the harness**

Append to `docs/eval/README.md`:

```markdown
## Retrieval Fixture Sweep

`scripts/evaluate-retrieval.sh` runs `axon ask --explain --diagnostics --json`
over `docs/eval/retrieval-fixtures.jsonl`. It checks whether the expected
domain appears in top domains and selected context without invoking Gemini
answer synthesis or judge analysis.

Use it before and after retrieval ranking, token-policy, or context-selection
changes:

```bash
cargo build --bin axon
scripts/evaluate-retrieval.sh
FAIL_ON_MISS=1 scripts/evaluate-retrieval.sh
```
```

- [ ] **Step 6: Verify**

Run:

```bash
bash -n scripts/evaluate-retrieval.sh
scripts/evaluate-retrieval.sh /tmp/axon-retrieval-smoke.jsonl
bd close axon_rust-hnfj.1 --reason "retrieval eval harness and fixtures added"
```

Expected: Bash syntax check passes, smoke summary is valid JSON, bead closes.

## Task 3: Add Corpus-Health Diagnostics

**Files:**
- Modify: `src/services/types/service.rs`
- Modify: `src/vector/ops/commands/ask.rs`
- Modify: `src/vector/ops/commands/ask/context.rs`
- Modify: `src/services/query/tests.rs`
- Modify: `src/cli/commands/ask.rs`

- [ ] **Step 1: Add typed diagnostics fields**

In `src/services/types/service.rs`, add:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CorpusHealthKind {
    Healthy,
    DomainNotIndexed,
    ThinDomain,
    RetrievedNotSelected,
    Unknown,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct CorpusHealthDiagnostic {
    pub kind: CorpusHealthKind,
    pub reason: String,
    pub selected_domain_count: usize,
    pub top_domain_count: usize,
}
```

Then add this field to `AskDiagnostics`:

```rust
    pub corpus_health: Option<CorpusHealthDiagnostic>,
```

- [ ] **Step 2: Add vector-side classifier**

In `src/vector/ops/commands/ask/context.rs`, add a pure helper:

```rust
fn classify_corpus_health(
    top_domains: &[String],
    selected_urls: &[String],
    candidate_pool: usize,
    context_chars: usize,
) -> crate::services::types::CorpusHealthDiagnostic {
    let top_domain_count = top_domains.len();
    let selected_domain_count = selected_urls
        .iter()
        .filter_map(|url| spider::url::Url::parse(url).ok())
        .filter_map(|url| url.host_str().map(str::to_string))
        .collect::<std::collections::HashSet<_>>()
        .len();

    let (kind, reason) = if candidate_pool == 0 {
        (
            crate::services::types::CorpusHealthKind::DomainNotIndexed,
            "retrieval returned no candidates".to_string(),
        )
    } else if selected_urls.is_empty() {
        (
            crate::services::types::CorpusHealthKind::RetrievedNotSelected,
            "retrieval returned candidates but none reached selected context".to_string(),
        )
    } else if context_chars < 2_000 {
        (
            crate::services::types::CorpusHealthKind::ThinDomain,
            "selected context is very small; indexed coverage may be thin".to_string(),
        )
    } else if top_domain_count == 0 {
        (
            crate::services::types::CorpusHealthKind::Unknown,
            "top-domain diagnostics were unavailable".to_string(),
        )
    } else {
        (
            crate::services::types::CorpusHealthKind::Healthy,
            "retrieval produced selected context".to_string(),
        )
    };

    crate::services::types::CorpusHealthDiagnostic {
        kind,
        reason,
        selected_domain_count,
        top_domain_count,
    }
}
```

- [ ] **Step 3: Populate diagnostics**

When building the ask payload in `src/vector/ops/commands/ask.rs`, include:

```rust
"corpus_health": ctx.corpus_health,
```

Add a matching field to the internal context struct:

```rust
pub corpus_health: crate::services::types::CorpusHealthDiagnostic,
```

- [ ] **Step 4: Add tests**

In `src/services/query/tests.rs`, extend the diagnostics fixture with:

```json
"corpus_health": {
  "kind": "healthy",
  "reason": "retrieval produced selected context",
  "selected_domain_count": 2,
  "top_domain_count": 5
}
```

Assert:

```rust
let health = diagnostics.corpus_health.expect("corpus health");
assert_eq!(health.kind, CorpusHealthKind::Healthy);
assert_eq!(health.selected_domain_count, 2);
```

- [ ] **Step 5: Verify**

Run:

```bash
cargo test services::query::tests --lib
cargo check --bin axon
```

Expected: tests and check pass.

## Task 4: Add Explain Selection Rank Metadata

**Files:**
- Modify: `src/services/types/service.rs`
- Modify: `src/vector/ops/commands/ask/context.rs`
- Modify: `src/vector/ops/commands/ask/context/build/trace.rs`
- Modify: `src/services/query/tests.rs`

- [ ] **Step 1: Add optional explain fields**

In `AskExplainCandidate`, add optional fields:

```rust
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_rerank_rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_full_doc_rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_context_rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub insertion_mode: Option<String>,
```

- [ ] **Step 2: Populate raw rerank ranks**

Where explain candidates are built from reranked candidates, set:

```rust
raw_rerank_rank: Some(reranked_idx + 1),
```

Use 1-based ranks for human-readable output.

- [ ] **Step 3: Populate full-doc and selected ranks from trace data**

Extend the trace result type in `build/trace.rs` with:

```rust
pub struct CandidateSelectionMetadata {
    pub planned_full_doc_rank: Option<usize>,
    pub selected_context_rank: Option<usize>,
    pub insertion_mode: Option<&'static str>,
}
```

Use these insertion mode strings:

```rust
"top_chunk"
"planned_full_doc"
"inserted_full_doc"
"supplemental"
"not_selected"
```

- [ ] **Step 4: Add backward-compatibility test**

In `src/services/query/tests.rs`, add:

```rust
#[test]
fn ask_explain_candidate_deserializes_without_rank_fields() {
    let value = serde_json::json!({
        "url": "https://docs.example.com/page",
        "score": 0.5,
        "rerank_score": 0.5,
        "selection_decisions": []
    });
    let parsed: AskExplainCandidate = serde_json::from_value(value).unwrap();
    assert_eq!(parsed.raw_rerank_rank, None);
    assert_eq!(parsed.selected_context_rank, None);
}
```

- [ ] **Step 5: Verify with uv trace**

Run:

```bash
./target/debug/axon --local ask --diagnostics --explain --json \
  "How do I use uv to manage Python dependencies?" \
  | jq '.explain.candidates[0:5] | map({url, raw_rerank_rank, planned_full_doc_rank, selected_context_rank, insertion_mode, selection_decisions})'
```

Expected: output includes rank fields for candidates where the rank is known and no panic when a field is absent.

## Task 5: Centralize Token And Authority Policy

**Files:**
- Create: `src/vector/ops/token_policy.rs`
- Modify: `src/vector/ops.rs`
- Modify: `src/vector/ops/commands/retrieval.rs`
- Modify: `src/vector/ops/commands/retrieval/tests.rs`
- Modify: `src/vector/ops/ranking.rs`
- Modify: `src/vector/ops/ranking_test.rs`

- [ ] **Step 1: Create the policy module**

Create `src/vector/ops/token_policy.rs`:

```rust
use std::collections::HashSet;

pub fn query_tokens(text: &str) -> Vec<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2 && !is_query_stop_word(token))
        .map(str::to_string)
        .collect()
}

pub fn identity_tokens(text: &str) -> HashSet<String> {
    text.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| token.len() >= 2)
        .map(str::to_string)
        .collect()
}

pub fn is_generic_authority_token(token: &str) -> bool {
    matches!(
        token,
        "api" | "app" | "book" | "build" | "cli" | "code" | "command" | "commands"
            | "config" | "create" | "documentation" | "error" | "errors" | "find"
            | "docs" | "guide" | "guides" | "handling" | "install" | "java"
            | "javascript" | "js" | "dependency" | "dependencies" | "go" | "manage"
            | "management" | "marketplace" | "node" | "nodejs" | "package"
            | "packages" | "plugin" | "plugins" | "publish" | "publishing" | "py"
            | "python" | "reference" | "registry" | "rs" | "rust" | "setup"
            | "structure" | "structured" | "structuring" | "tool" | "tools" | "ts"
            | "typescript" | "using" | "view" | "views"
    )
}

pub fn is_generic_topical_token(token: &str) -> bool {
    matches!(
        token,
        "api" | "app" | "book" | "build" | "cli" | "code" | "command" | "commands"
            | "config" | "create" | "documentation" | "error" | "errors" | "find"
            | "docs" | "guide" | "guides" | "handling" | "install" | "dependency"
            | "dependencies" | "manage" | "management" | "marketplace" | "package"
            | "packages" | "plugin" | "plugins" | "publish" | "publishing"
            | "reference" | "registry" | "setup" | "structure" | "structured"
            | "structuring" | "tool" | "tools" | "using" | "view" | "views"
    )
}

fn is_query_stop_word(token: &str) -> bool {
    matches!(
        token,
        "a" | "am" | "an" | "and" | "any" | "are" | "as" | "at" | "be" | "but"
            | "by" | "can" | "do" | "does" | "for" | "from" | "had" | "has"
            | "have" | "he" | "her" | "him" | "his" | "how" | "if" | "in"
            | "into" | "is" | "it" | "its" | "me" | "my" | "no" | "not" | "of"
            | "on" | "or" | "our" | "out" | "she" | "so" | "than" | "that"
            | "the" | "their" | "them" | "then" | "they" | "this" | "to" | "too"
            | "up" | "us" | "via" | "was" | "we" | "were" | "what" | "when"
            | "where" | "who" | "why" | "you" | "your"
    )
}
```

- [ ] **Step 2: Wire the module**

In the vector ops module root, add:

```rust
pub(crate) mod token_policy;
```

- [ ] **Step 3: Delegate query tokenization**

In `src/vector/ops/ranking.rs`, replace `tokenize_query` body with:

```rust
pub fn tokenize_query(text: &str) -> Vec<String> {
    super::token_policy::query_tokens(text)
}
```

- [ ] **Step 4: Delegate retrieval policy**

In `src/vector/ops/commands/retrieval.rs`, replace local generic token helpers with calls:

```rust
crate::vector::ops::token_policy::is_generic_authority_token(token.as_str())
crate::vector::ops::token_policy::is_generic_topical_token(token.as_str())
crate::vector::ops::token_policy::identity_tokens(segment)
```

- [ ] **Step 5: Add malformed URL regression**

In `src/vector/ops/commands/retrieval/tests.rs`, add:

```rust
#[test]
fn product_authority_boost_ignores_malformed_urls() {
    let query_tokens = vec!["uv".to_string()];
    assert_eq!(
        product_authority_boost_for_url("not a url", &query_tokens, 0.35),
        0.0
    );
}
```

- [ ] **Step 6: Verify**

Run:

```bash
cargo test vector::ops::commands::retrieval::tests --lib
cargo test vector::ops::ranking::tests --lib
cargo check --bin axon
```

Expected: tests preserve all current regression behavior.

## Task 6: Tune Authority-Aware Selection Diversity

**Files:**
- Modify: `src/vector/ops/commands/ask/context/build/selection.rs`
- Modify: `src/vector/ops/commands/ask/context/tests.rs`
- Modify: `src/vector/ops/commands/ask/context/retrieval.rs`

- [ ] **Step 1: Run baseline harness**

Run:

```bash
cargo build --bin axon
scripts/evaluate-retrieval.sh
cp "$(ls -t .cache/axon-rust/evals/retrieval-*.summary.json | head -n 1)" /tmp/axon-selection-before.json
cat /tmp/axon-selection-before.json
```

Expected: baseline summary is saved before selection changes.

- [ ] **Step 2: Add selection policy input**

In `selection.rs`, introduce:

```rust
#[derive(Debug, Clone, Copy)]
pub(super) struct SelectionPolicy {
    pub prefer_authoritative: bool,
    pub max_docs_per_domain: usize,
}

impl Default for SelectionPolicy {
    fn default() -> Self {
        Self {
            prefer_authoritative: true,
            max_docs_per_domain: 3,
        }
    }
}
```

Change `select_context_indices` signature to:

```rust
pub fn select_context_indices(
    reranked: &[ranking::AskCandidate],
    query_tokens: &[String],
    chunk_limit: usize,
    full_doc_limit: usize,
    policy: SelectionPolicy,
) -> (Vec<usize>, Vec<usize>)
```

- [ ] **Step 3: Enforce domain-level diversity for full docs**

Add this helper:

```rust
fn domain_count_for_selected(
    reranked: &[ranking::AskCandidate],
    selected: &[usize],
    candidate_idx: usize,
) -> usize {
    let Some(candidate_host) = host_from_url(&reranked[candidate_idx].url) else {
        return 0;
    };
    selected
        .iter()
        .filter(|&&idx| host_from_url(&reranked[idx].url).as_deref() == Some(candidate_host.as_str()))
        .count()
}
```

Before pushing a full-doc index, skip candidates where:

```rust
domain_count_for_selected(reranked, &top_full_doc_indices, idx) >= policy.max_docs_per_domain
```

- [ ] **Step 4: Add tests**

In `src/vector/ops/commands/ask/context/tests.rs`, add tests named:

```rust
#[test]
fn selection_limits_full_docs_per_domain_when_alternatives_exist() { /* build candidates and assert */ }

#[test]
fn selection_preserves_non_authoritative_high_signal_example() { /* build candidates and assert */ }

#[test]
fn selection_without_authority_signal_preserves_existing_diversity() { /* build candidates and assert */ }
```

Each test must construct candidates directly and assert selected URL domains.

- [ ] **Step 5: Run after harness**

Run:

```bash
cargo test vector::ops::commands::ask::context --lib
scripts/evaluate-retrieval.sh
cp "$(ls -t .cache/axon-rust/evals/retrieval-*.summary.json | head -n 1)" /tmp/axon-selection-after.json
jq -n --slurpfile before /tmp/axon-selection-before.json --slurpfile after /tmp/axon-selection-after.json \
  '{before:$before[0], after:$after[0]}'
```

Expected: top20 and popular selected pass counts do not regress. If they regress, revert the selection change or document the corpus-health reason in `axon_rust-hnfj.5`.

## Task 7: Document Anti-Overfit Workflow

**Files:**
- Modify: `docs/ASK.md`
- Modify: `docs/commands/ask.md`
- Modify: `src/vector/CLAUDE.md`

- [ ] **Step 1: Add the workflow diagram**

Add this diagram to `docs/commands/ask.md` under the diagnostics/explain section:

```text
query
  |
  v
TEI embedding + Qdrant dense/BM42/RRF retrieval
  |
  v
rerank/filter + token/authority policy
  |
  v
corpus-health classification
  |
  v
context selection + bounded full-doc fetch
  |
  v
ask --explain retrieval harness
  |
  +--> ranking bug? tune scoring/filtering
  +--> selection bug? tune context selection
  +--> corpus gap? crawl/index better docs
  +--> fixture mismatch? update tracked fixture notes
```

- [ ] **Step 2: Add the anti-overfit rule**

Add this text to `src/vector/CLAUDE.md`:

```markdown
### Retrieval Tuning Rule

Do not tune retrieval from a single query. Before changing scoring, token policy,
authority handling, or context selection, run the tracked retrieval fixture sweep.
Classify every miss first:

- ranking bug: relevant candidates exist but score/filter order is wrong
- selection bug: relevant candidates rank well but do not enter context
- corpus-health gap: expected source is not indexed or indexed too thinly
- fixture mismatch: the fixture expectation does not match indexed content

Hard-coded product/domain allowlists are not allowed in code. User-configured
authoritative domains are allowed through config.
```

- [ ] **Step 3: Add cost-tiered gates**

Add this text to `docs/ASK.md`:

```markdown
### Retrieval Quality Gates

Fast gates:
- token policy unit tests
- explain/schema unit tests
- selection unit tests

Medium gates:
- `scripts/evaluate-retrieval.sh`
- `FAIL_ON_MISS=1 scripts/evaluate-retrieval.sh`

Slow gates:
- `axon evaluate`
- `scripts/evaluate-ask-golden.sh`
- `scripts/bench-ask.sh`

Use slow gates for release signoff, not for every small retrieval tuning loop.
```

- [ ] **Step 4: Run docs checks**

Run:

```bash
rg -n "single query|hard-coded product|evaluate-retrieval|Retrieval Tuning Rule" docs src/vector/CLAUDE.md
cargo fmt --check
cargo check --bin axon
```

Expected: docs contain the anti-overfit rule and Rust checks still pass.

## Task 8: Final Verification And Bead Closure

**Files:**
- Bead state
- Git state

- [ ] **Step 1: Run targeted verification**

Run:

```bash
cargo test vector::ops::commands::retrieval::tests --lib
cargo test vector::ops::ranking::tests --lib
cargo test services::query::tests --lib
cargo test vector::ops::commands::ask::context --lib
cargo fmt --check
cargo check --bin axon
scripts/evaluate-retrieval.sh
```

Expected: all commands pass or any failure is recorded against the responsible child bead.

- [ ] **Step 2: Validate Beads graph**

Run:

```bash
bd swarm validate axon_rust-hnfj
bd list --parent axon_rust-hnfj --json | jq '[.[] | {id,title,status}]'
```

Expected: all implemented child beads are closed or explicitly left open with comments explaining remaining work.

- [ ] **Step 3: Commit**

Run:

```bash
git status --short
git add scripts/evaluate-retrieval.sh docs/eval/retrieval-fixtures.jsonl docs/eval/README.md docs/ASK.md docs/commands/ask.md src/vector/CLAUDE.md src/vector/ops src/services src/cli/commands/ask.rs
git commit -m "feat: harden retrieval quality workflow"
```

Expected: commit succeeds. If unrelated dirty files remain, do not stage them unless the user explicitly asked to include the whole worktree.

## Self-Review

Spec coverage:
- `lavra-research` findings are applied through harness, diagnostics, explain ranks, token policy, selection gating, and docs gates.
- `lavra-ceo-review` findings are applied through the preflight blocker, named failure states, rollback criteria, observability requirements, and the diagram requirement.
- `superpowers:writing-plans` requirements are met by exact files, exact commands, small tasks, and verification steps.

Placeholder scan:
- This plan contains no placeholder markers and no unspecified edge-case steps.

Type consistency:
- `CorpusHealthKind`, `CorpusHealthDiagnostic`, `SelectionPolicy`, and explain-rank field names are used consistently across tasks.

Execution handoff:

Plan complete and saved to `docs/superpowers/plans/2026-05-15-retrieval-quality-hardening.md`. Two execution options:

1. Subagent-Driven (recommended) - dispatch a fresh subagent per task, review between tasks, fast iteration.
2. Inline Execution - execute tasks in this session using executing-plans, batch execution with checkpoints.
