# ask Gate Simplification Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove Gates 5 & 6 (brittle URL-heuristic source-quality checks) from `normalize_ask_answer`, flip `ask_strict_procedural` and `ask_strict_config_schema` defaults to `false`, and clean up all dead code and config they leave behind.

**Architecture:** The LLM system prompt already enforces citation grounding. Gate 1 (no citations), Gate 2 (LLM self-flags insufficient), Gate 3 (unmapped citations), and Gate 4 (non-trivial min-citation count) are sufficient and correct. Gates 5 & 6 use `is_official_docs_source()` / `has_exact_page_citation()` heuristics that produce false positives on legitimate pluralized or non-standard path patterns (e.g. `/guides/` vs `/guide/`). Removing them eliminates the false-positive class with no quality regression.

**Tech Stack:** Rust, `crates/vector/ops/commands/ask.rs`, `crates/core/config/types.rs`, `crates/core/config/parse.rs`, `docs/commands/ask.md`

---

### Task 1: Remove Gates 5 & 6 dead code from ask.rs

**Files:**
- Modify: `crates/vector/ops/commands/ask.rs`

The following functions become dead code once Gates 5 & 6 are removed. Delete them entirely:
- `classify_query()` and `AskQueryClass` enum
- `is_official_docs_source()`
- `url_path_is_docs_like()`
- `host_from_source()`
- `source_matches_domain_list()`
- `query_file_like_tokens()`
- `has_exact_page_citation()`

**Step 1: Delete dead functions**

In `crates/vector/ops/commands/ask.rs`, remove lines 238–376 (everything from `#[derive(Clone, Copy, PartialEq, Eq)] enum AskQueryClass` through `fn has_exact_page_citation`). Also remove the `use spider::url::Url;` import at line 6 if it is only used by those functions (check first).

**Step 2: Run cargo check to confirm what's now unused**

```bash
cargo check 2>&1 | grep -E "unused|error"
```

Expected: errors referencing `classify_query`, `is_official_docs_source`, etc. — those are the call sites we fix next.

---

### Task 2: Simplify normalize_ask_answer — remove gate 5 & 6 call sites

**Files:**
- Modify: `crates/vector/ops/commands/ask.rs` — `normalize_ask_answer` function

**Step 1: Write the replacement normalize_ask_answer**

Current `normalize_ask_answer` (lines 435–528) does:
1. Parse source map
2. Strip sources section from body
3. Extract cited IDs → Gate 1 (no citations)
4. `indicates_insufficient_evidence` → Gate 2
5. Map cited IDs to URLs → Gate 3 (unmapped)
6. `is_non_trivial` + min-citation count → Gate 4
7. Authoritative allowlist check → keep (it's opt-in, not on by default)
8. `classify_query` + Gate 5 (procedural) → **DELETE**
9. `classify_query` + Gate 6 (config/schema) → **DELETE**
10. Format passing answer

Replace the body of `normalize_ask_answer` with this (removing the `query_class` block entirely, keeping everything else):

```rust
fn normalize_ask_answer(cfg: &Config, query: &str, answer: &str, context: &str) -> String {
    let source_map = parse_context_source_map(context);
    let body = strip_sources_section(answer);
    let cited = extract_cited_source_ids(&body);
    let mut insufficiency_reasons: Vec<String> = Vec::new();

    // Gate 1: no citations at all
    if cited.is_empty() {
        insufficiency_reasons.push("Answer contained no source citations.".to_string());
        return format_insufficient_evidence(&source_map, None, &insufficiency_reasons);
    }

    // Gate 2: LLM self-flagged insufficient evidence
    if indicates_insufficient_evidence(&body) {
        insufficiency_reasons.push("Model flagged insufficient supporting evidence.".to_string());
        return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
    }

    // Gate 3: citations don't map to retrieved sources
    let mut seen_sources: HashSet<String> = HashSet::new();
    let source_lines = cited
        .iter()
        .filter_map(|id| {
            source_map.get(id).and_then(|source| {
                if seen_sources.insert(source.clone()) {
                    Some(format!("- [S{id}] {source}"))
                } else {
                    None
                }
            })
        })
        .collect::<Vec<_>>();
    if source_lines.is_empty() {
        insufficiency_reasons.push("Citations did not map to retrieved sources.".to_string());
        return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
    }

    // Gate 4: non-trivial answers need minimum unique citations
    let min_citations = if is_non_trivial(query, &body) {
        cfg.ask_min_citations_nontrivial
    } else {
        1
    };
    if source_lines.len() < min_citations {
        insufficiency_reasons.push(format!(
            "Non-trivial answer requires at least {min_citations} unique citations; found {}.",
            source_lines.len()
        ));
    }

    // Gate 4b: authoritative allowlist (opt-in, empty by default)
    let cited_sources = source_lines
        .iter()
        .filter_map(|line| line.split_once("] ").map(|(_, source)| source.to_string()))
        .collect::<Vec<_>>();
    if !cfg.ask_authoritative_allowlist.is_empty()
        && !cited_sources
            .iter()
            .any(|source| source_matches_domain_list(source, &cfg.ask_authoritative_allowlist))
    {
        insufficiency_reasons.push(
            "Authoritative allowlist is configured, but no cited source matched it.".to_string(),
        );
    }

    if !insufficiency_reasons.is_empty() {
        return format_insufficient_evidence(&source_map, Some(&cited), &insufficiency_reasons);
    }

    format!(
        "{}\n\n## Sources\n{}",
        body.trim_end(),
        source_lines.join("\n")
    )
}
```

Note: `source_matches_domain_list` is still used by Gate 4b (allowlist). Keep it. Only the functions listed in Task 1 are fully dead.

**Step 2: Run cargo check**

```bash
cargo check 2>&1 | grep error
```

Expected: 0 errors.

---

### Task 3: Remove ask_strict_procedural and ask_strict_config_schema from Config

**Files:**
- Modify: `crates/core/config/types.rs`
- Modify: `crates/core/config/parse.rs`

**Step 1: Remove fields from Config struct**

In `crates/core/config/types.rs`, delete:
```rust
    /// Gate 5: procedural queries ("how do I …") require an official-docs citation.
    pub ask_strict_procedural: bool,
    /// Gate 6: config/schema queries require a citation from the exact referenced page.
    pub ask_strict_config_schema: bool,
```

**Step 2: Remove from Config::default()**

In `crates/core/config/types.rs`, delete from the `default()` impl:
```rust
            ask_strict_procedural: true,
            ask_strict_config_schema: true,
```

**Step 3: Remove from Debug impl**

In `crates/core/config/types.rs`, delete from the `fmt::Debug` impl:
```rust
            .field("ask_strict_procedural", &self.ask_strict_procedural)
            .field("ask_strict_config_schema", &self.ask_strict_config_schema)
```

**Step 4: Remove from parse.rs**

In `crates/core/config/parse.rs`, delete:
```rust
        ask_strict_procedural: performance::env_bool("AXON_ASK_STRICT_PROCEDURAL", true),
        ask_strict_config_schema: performance::env_bool("AXON_ASK_STRICT_CONFIG_SCHEMA", true),
```

**Step 5: Run cargo check**

```bash
cargo check 2>&1 | grep error
```

Expected: 0 errors.

---

### Task 4: Update tests — remove gate 5/6 tests, fix remaining tests

**Files:**
- Modify: `crates/vector/ops/commands/ask.rs` — test module

**Step 1: Delete tests that specifically test the removed gates**

Remove these test functions entirely (they test behavior that no longer exists):
- `procedural_query_requires_official_docs_citation`
- `config_schema_query_requires_exact_page_citation`
- `strict_procedural_false_bypasses_gate5`
- `strict_config_schema_false_bypasses_gate6`
- `platejs_docs_url_passes_procedural_gate`
- `url_path_is_docs_like_recognizes_common_prefixes`
- `is_official_docs_source_accepts_docs_path_prefix`

**Step 2: Fix the regression fixture test**

The `ask_quality_regression_fixtures_five_queries` test has two fixtures that expected Gate 5/6 to block:
- `"codex command weak source"` — expected `insufficient` because source was a blog (Gate 5). Now it should PASS (Gate 5 gone, blog citation is fine).
- `"openai yaml missing exact page"` — expected `insufficient` because of Gate 6. Now it should PASS.

Update those two fixtures — change `expect_insufficient: true` to `expect_insufficient: false`.

Also remove `cfg.ask_min_citations_nontrivial = 2` and `cfg.ask_authoritative_domains` setup from that test if they were only there to exercise the now-removed gates (check whether they affect the remaining three fixtures first).

**Step 3: Fix normalize_ask_answer_replaces_sources_with_deduped_section test**

This test used `docs.*` hostnames specifically to pass Gate 5. Now that Gate 5 is gone, it can use any valid URLs. No change needed if it still passes — just verify.

**Step 4: Run the ask tests**

```bash
cargo test --lib -- ask 2>&1
```

Expected: all remaining tests pass.

**Step 5: Run all lib tests**

```bash
cargo test --lib 2>&1 | tail -5
```

Expected: 0 failures.

---

### Task 5: Update config types.rs default assertion test

**Files:**
- Modify: `crates/core/config/types.rs` — test at bottom

**Step 1: Remove the two assertions for deleted fields**

Find and delete:
```rust
        assert!(
            cfg.ask_strict_procedural,
            "ask_strict_procedural should default to true"
        );
        assert!(
            cfg.ask_strict_config_schema,
            "ask_strict_config_schema should default to true"
        );
```

**Step 2: Run config tests**

```bash
cargo test --lib -- config 2>&1
```

Expected: all pass.

---

### Task 6: Update docs and .env.example

**Files:**
- Modify: `docs/commands/ask.md`
- Modify: `.env.example`
- Modify: `README.md`

**Step 1: Update ask.md**

Remove the notes section entries for Gates 5 & 6:
```
  - Procedural queries must cite at least one official docs source.
  - Config/schema queries must cite at least one exact page (not just a root domain).
```

Replace with a cleaner description of the remaining gates:
```
- `ask` enforces citation-quality gates:
  - Answers must include inline `[S#]` citations from retrieved context.
  - Non-trivial responses must satisfy `AXON_ASK_MIN_CITATIONS_NONTRIVIAL`.
  - Failed gates return structured insufficient-evidence output with next-index suggestions.
```

**Step 2: Remove AXON_ASK_STRICT_PROCEDURAL and AXON_ASK_STRICT_CONFIG_SCHEMA from .env.example**

Search for and remove those two lines (if present).

**Step 3: Remove from README.md**

Search README for `AXON_ASK_STRICT_PROCEDURAL` and `AXON_ASK_STRICT_CONFIG_SCHEMA` — remove those rows from any env-var tables.

**Step 4: Verify no stale references**

```bash
grep -r "ask_strict_procedural\|ask_strict_config_schema\|AXON_ASK_STRICT" . \
  --include="*.rs" --include="*.md" --include="*.toml" --include=".env*"
```

Expected: 0 results.

---

### Task 7: Final verification and commit

**Step 1: Full lint + test**

```bash
cargo fmt --check && cargo clippy -- -D warnings && cargo test --lib 2>&1 | tail -10
```

Expected: fmt clean, 0 clippy warnings, 0 test failures.

**Step 2: Smoke test the ask command**

```bash
./scripts/axon ask "how does spider.rs handle JavaScript-heavy sites?" --diagnostics 2>&1
```

Expected: A real answer with `## Sources` section citing `spider.cloud/guides/spider/` — NOT an "Insufficient evidence" block.

**Step 3: Commit**

```bash
git add crates/vector/ops/commands/ask.rs \
        crates/core/config/types.rs \
        crates/core/config/parse.rs \
        docs/commands/ask.md \
        README.md \
        .env.example
git commit -m "fix(ask): remove brittle Gate 5/6 URL heuristics; trust LLM citation grounding

Gates 5 (procedural/official-docs) and 6 (config/schema/exact-page) used
url_path_is_docs_like() and has_exact_page_citation() heuristics that produced
false positives on legitimate sources with non-standard path patterns (e.g.
/guides/ vs /guide/). The LLM system prompt already enforces citation grounding;
Gates 1-4 (no citations, LLM self-flag, unmapped citations, min-citation count)
are sufficient.

Removes: classify_query, AskQueryClass, is_official_docs_source,
url_path_is_docs_like, host_from_source, source_matches_domain_list (in ask.rs),
query_file_like_tokens, has_exact_page_citation, ask_strict_procedural config,
ask_strict_config_schema config, and all associated tests."
```

---

## Summary of Changes

| File | Change |
|------|--------|
| `crates/vector/ops/commands/ask.rs` | Remove `AskQueryClass`, `classify_query`, `is_official_docs_source`, `url_path_is_docs_like`, `host_from_source` (ask.rs copy), `source_matches_domain_list` (ask.rs copy), `query_file_like_tokens`, `has_exact_page_citation`; simplify `normalize_ask_answer`; delete 7 tests |
| `crates/core/config/types.rs` | Remove `ask_strict_procedural`, `ask_strict_config_schema` fields + defaults + Debug impl + assertions |
| `crates/core/config/parse.rs` | Remove 2 env_bool parse lines |
| `docs/commands/ask.md` | Remove Gate 5/6 documentation |
| `README.md` | Remove `AXON_ASK_STRICT_*` env var rows |
| `.env.example` | Remove `AXON_ASK_STRICT_*` lines if present |
