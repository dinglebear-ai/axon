# ask Prompt Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the minimal 10-word `ask` system prompt with a precise 5-rule prompt that enforces inline citations, a `## Sources` footer, synthesis across sources, explicit gap-flagging, and technical precision — while stripping the now-redundant instruction lines from the context preamble.

**Architecture:** Two files, three targeted string replacements. System prompt consolidates all behavioral rules; the context preamble becomes pure data (`Sources:\n{sources}`). The baseline prompt (used by `axon evaluate`) also gets a parallel improvement. No structural changes, no new files.

**Tech Stack:** Rust — `cargo fmt`, `cargo clippy`, `cargo test`

---

## Context

### Current state (before)

**`streaming.rs` — system prompt used by `ask_llm_streaming` and `ask_llm_non_streaming`:**
```
"Answer only using provided context. Cite sources like [S1]."
```

**`streaming.rs` — system prompt used by `baseline_llm_streaming` and `baseline_llm_non_streaming`:**
```
"Answer the following question accurately and thoroughly. If you are unsure, say so explicitly."
```

**`ask.rs` — context preamble in `build_ask_context` (line 210–214):**
```rust
let context = format!(
    "Answer only from the provided sources.\nCite supporting sources inline using [S#] labels.\nIf the sources are incomplete, say so explicitly.\n\nSources:\n{}",
    context_entries.join(separator)
);
```

### Target state (after)

**`streaming.rs` — new `ask` system prompt (both streaming and non-streaming):**
```
You are a precise technical research assistant. You answer questions exclusively from the retrieved source documents provided in the context. Your rules:

1. CITATIONS — Cite inline immediately after each claim using [S#] labels. When multiple sources support the same point, cite all of them: [S1][S3]. Never make a claim without a citation.
2. FOOTER — After your answer, add a "## Sources" section listing each cited source number and its URL, e.g. "[S1] https://..."
3. SYNTHESIS — Integrate information from multiple sources into a unified answer. Do not quote or summarize sources one by one.
4. GAPS — If the sources do not fully answer the question, explicitly state what is covered and what is not. Do not fill gaps from general knowledge.
5. PRECISION — For technical questions, be specific: include exact values, function names, file paths, and configuration keys when the sources provide them.
```

**`streaming.rs` — new baseline system prompt (both streaming and non-streaming):**
```
You are a knowledgeable technical assistant. Answer the following question accurately and thoroughly, drawing on your full training knowledge. Where you are uncertain or your knowledge may be outdated, say so explicitly rather than presenting uncertain information as fact. For technical questions, be specific: include exact values, function names, and configuration details where you know them.
```

**`ask.rs` — new context preamble:**
```rust
let context = format!(
    "Sources:\n{}",
    context_entries.join(separator)
);
```

---

## Tasks

### Task 1: Update the `ask` system prompt in `streaming.rs`

**Files:**
- Modify: `crates/vector/ops/commands/streaming.rs:101` (ask_llm_streaming)
- Modify: `crates/vector/ops/commands/streaming.rs:129` (ask_llm_non_streaming)

The system prompt string appears identically at both locations. Both use `"Answer only using provided context. Cite sources like [S1]."`.

**Step 1: Replace the system prompt in `ask_llm_streaming`**

Find line 101 (inside the `.json(...)` block of `ask_llm_streaming`):
```rust
{"role": "system", "content": "Answer only using provided context. Cite sources like [S1]."},
```

Replace with:
```rust
{"role": "system", "content": "You are a precise technical research assistant. You answer questions exclusively from the retrieved source documents provided in the context. Your rules:\n\n1. CITATIONS — Cite inline immediately after each claim using [S#] labels. When multiple sources support the same point, cite all of them: [S1][S3]. Never make a claim without a citation.\n2. FOOTER — After your answer, add a \"## Sources\" section listing each cited source number and its URL, e.g. \"[S1] https://...\"\n3. SYNTHESIS — Integrate information from multiple sources into a unified answer. Do not quote or summarize sources one by one.\n4. GAPS — If the sources do not fully answer the question, explicitly state what is covered and what is not. Do not fill gaps from general knowledge.\n5. PRECISION — For technical questions, be specific: include exact values, function names, file paths, and configuration keys when the sources provide them."},
```

**Step 2: Replace the system prompt in `ask_llm_non_streaming`**

Find line 129 (identical string in the non-streaming variant):
```rust
{"role": "system", "content": "Answer only using provided context. Cite sources like [S1]."},
```

Replace with the same new system prompt string as Step 1.

**Step 3: Verify the replacements are consistent**

```bash
grep -n "Answer only using provided context" crates/vector/ops/commands/streaming.rs
```
Expected: no output (both instances replaced).

```bash
grep -c "precise technical research assistant" crates/vector/ops/commands/streaming.rs
```
Expected: `2` (both streaming and non-streaming updated).

**Step 4: Run `cargo clippy` to verify no compile issues**

```bash
cargo clippy 2>&1 | tail -5
```
Expected:
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in Xs
```
Zero warnings.

**Step 5: Commit**

```bash
git add crates/vector/ops/commands/streaming.rs
git commit -m "feat(ask): replace minimal ask system prompt with 5-rule citation-enforcing prompt"
```

---

### Task 2: Update the baseline system prompt in `streaming.rs`

**Files:**
- Modify: `crates/vector/ops/commands/streaming.rs` (baseline_llm_streaming and baseline_llm_non_streaming)

The baseline system prompt appears twice — once in `baseline_llm_streaming` (around line 159) and once in `baseline_llm_non_streaming` (around line 186).

**Step 1: Replace the system prompt in `baseline_llm_streaming`**

Find:
```rust
{"role": "system", "content": "Answer the following question accurately and thoroughly. If you are unsure, say so explicitly."},
```

Replace with:
```rust
{"role": "system", "content": "You are a knowledgeable technical assistant. Answer the following question accurately and thoroughly, drawing on your full training knowledge. Where you are uncertain or your knowledge may be outdated, say so explicitly rather than presenting uncertain information as fact. For technical questions, be specific: include exact values, function names, and configuration details where you know them."},
```

**Step 2: Replace the system prompt in `baseline_llm_non_streaming`**

Find the same old string in `baseline_llm_non_streaming` and replace with the same new string from Step 1.

**Step 3: Verify**

```bash
grep -c "knowledgeable technical assistant" crates/vector/ops/commands/streaming.rs
```
Expected: `2`

```bash
grep -c "Answer the following question accurately" crates/vector/ops/commands/streaming.rs
```
Expected: `0` (old string fully replaced)

**Step 4: Run `cargo clippy`**

```bash
cargo clippy 2>&1 | tail -5
```
Expected: zero warnings.

**Step 5: Commit**

```bash
git add crates/vector/ops/commands/streaming.rs
git commit -m "feat(evaluate): improve baseline prompt for more precise knowledge-grounded answers"
```

---

### Task 3: Strip instruction lines from context preamble in `ask.rs`

**Files:**
- Modify: `crates/vector/ops/commands/ask.rs:210–214` (`build_ask_context`)

**Step 1: Find the current preamble format string**

In `build_ask_context`, near the bottom, find:
```rust
    let context = format!(
        "Answer only from the provided sources.\nCite supporting sources inline using [S#] labels.\nIf the sources are incomplete, say so explicitly.\n\nSources:\n{}",
        context_entries.join(separator)
    );
```

**Step 2: Replace with data-only preamble**

```rust
    let context = format!(
        "Sources:\n{}",
        context_entries.join(separator)
    );
```

The three instruction lines are removed. The system prompt now owns all behavioral rules. The preamble is pure source data.

**Step 3: Verify the old instruction text is gone**

```bash
grep -n "Answer only from the provided sources" crates/vector/ops/commands/ask.rs
```
Expected: no output.

```bash
grep -n "Sources:" crates/vector/ops/commands/ask.rs
```
Expected: one match at the `format!` call.

**Step 4: Run full test suite**

```bash
cargo test 2>&1 | grep -E "^test result"
```
Expected: all `ok`, 0 failed across all test suites. This is the regression guard — the context preamble change must not break any existing tests.

**Step 5: Run `cargo fmt --check`**

```bash
cargo fmt --check
```
Expected: clean (no formatting differences).

**Step 6: Commit**

```bash
git add crates/vector/ops/commands/ask.rs
git commit -m "feat(ask): strip redundant instruction lines from context preamble — system prompt owns all rules"
```

---

## Verification

After all three tasks are committed, do a final sanity check:

```bash
# 1. No old minimal prompt text remains
grep -rn "Answer only using provided context" crates/
# Expected: no output

# 2. No old preamble instruction text remains
grep -rn "Answer only from the provided sources" crates/
# Expected: no output

# 3. New system prompt present in both streaming variants
grep -c "precise technical research assistant" crates/vector/ops/commands/streaming.rs
# Expected: 2

# 4. New baseline prompt present in both baseline variants
grep -c "knowledgeable technical assistant" crates/vector/ops/commands/streaming.rs
# Expected: 2

# 5. Full build + lint + test
cargo clippy && cargo test
# Expected: 0 warnings, all tests pass
```

### Manual Smoke Test (requires live services)

```bash
./scripts/axon ask "what is the chunking strategy used in chunk_text?"
```

Expected output characteristics:
- Answer contains at least one `[S1]` (or other `[S#]`) inline citation
- Answer ends with a `## Sources` section listing `[S1] https://...` entries
- Answer does not make claims without backing citations
- If sources don't fully cover the question, response says so explicitly
