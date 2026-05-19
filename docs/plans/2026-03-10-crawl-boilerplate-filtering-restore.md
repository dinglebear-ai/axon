# Crawl Boilerplate Filtering Restore Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Restore crawl-time HTML boilerplate stripping so saved markdown excludes common navigation, banner, footer, dialog, iframe, and hidden-page chrome before content is embedded or displayed.

**Architecture:** Add a single shared selector list in the core content transform layer and thread it through every direct `TransformInput` construction used by crawl recovery/render paths. Lock the behavior with unit tests in `crates/core/content/tests.rs`, then verify end-to-end by crawling `modelcontextprotocol.io` and inspecting produced markdown for previously observed nav/search cruft.

**Tech Stack:** Rust, `spider_transformations`, `lol_html`, Axon crawl engine, cargo test

---

### Task 1: Restore Shared Boilerplate Selectors In Core Transform

**Files:**
- Modify: `crates/core/content.rs`
- Test: `crates/core/content/tests.rs`

**Step 1: Write the failing tests**

Add focused tests in `crates/core/content/tests.rs` proving `to_markdown()` drops:
- ARIA landmark boilerplate: `[role="navigation"]`, `[role="banner"]`, `[role="complementary"]`, `[role="contentinfo"]`, `[role="search"]`
- HTML5/explicit boilerplate: `noscript`, `iframe`, `[hidden]`, `[data-nosnippet]`
- Preserve the real article/main body text in the same fixture

Example fixture shape:

```rust
let html = r#"
<html><body>
  <div role="banner">Site Header</div>
  <div role="navigation"><a href="/docs">Docs</a></div>
  <main>
    <h1>Specification</h1>
    <p>MCP defines how models communicate with tools.</p>
  </main>
  <iframe src="https://example.com/embed"></iframe>
</body></html>
"#;

let markdown = to_markdown(html, None);
assert!(markdown.contains("MCP defines how models communicate with tools."));
assert!(!markdown.contains("Site Header"));
assert!(!markdown.contains("Docs"));
assert!(!markdown.contains("example.com/embed"));
```

**Step 2: Run tests to verify they fail**

Run:

```bash
cargo test content::tests -- --nocapture
```

Expected: new boilerplate-removal assertions fail because `ignore_tags` is not wired.

**Step 3: Write the minimal implementation**

In `crates/core/content.rs`:
- Add `pub const BOILERPLATE_SELECTORS: &[&str]`
- Include only explicit low-risk selectors:

```rust
pub const BOILERPLATE_SELECTORS: &[&str] = &[
    "[role=\"navigation\"]",
    "[role=\"banner\"]",
    "[role=\"contentinfo\"]",
    "[role=\"complementary\"]",
    "[role=\"search\"]",
    "[role=\"dialog\"]",
    "[role=\"alertdialog\"]",
    "[role=\"form\"]",
    "[aria-hidden=\"true\"]",
    "noscript",
    "iframe",
    "[hidden]",
    "[data-nosnippet]",
];
```

- Update `to_markdown()` so `TransformInput.ignore_tags` uses `Some(BOILERPLATE_SELECTORS)`
- Keep existing `readability: false`, `clean_html: false`, and `main_content: true`

**Step 4: Run tests to verify they pass**

Run:

```bash
cargo test content::tests -- --nocapture
```

Expected: all content tests pass, including the new boilerplate-removal cases.

**Step 5: Commit**

```bash
git add crates/core/content.rs crates/core/content/tests.rs
git commit -m "fix(crawl): restore crawl-time boilerplate filtering"
```

### Task 2: Thread Selectors Through Direct Crawl Transform Paths

**Files:**
- Modify: `crates/crawl/engine/collector.rs`
- Modify: `crates/crawl/engine/thin_refetch.rs`
- Modify: `crates/crawl/engine/cdp_render.rs`

**Step 1: Write the failing test or characterization guard**

If an existing unit test covers markdown generation in these modules, extend it to assert the constructed `TransformInput` path uses the shared ignore list indirectly through output expectations. If there is no direct unit seam, add a narrow test in the lowest practical file that feeds HTML containing a nav/banner block through the helper and asserts the rendered markdown excludes it.

If no realistic seam exists without heavy refactoring, document that this task is covered by the end-to-end crawl verification in Task 4 and proceed with minimal code change only.

**Step 2: Run targeted tests to verify failure**

Run the narrowest applicable command. Examples:

```bash
cargo test crawl::engine -- --nocapture
```

or, if only end-to-end coverage is practical, record that no isolated test exists and move to implementation.

**Step 3: Write the minimal implementation**

In each file:
- import `BOILERPLATE_SELECTORS` from `crate::crates::core::content`
- replace `ignore_tags: None` with `ignore_tags: Some(BOILERPLATE_SELECTORS)`

Target locations:
- `crates/crawl/engine/collector.rs`
- `crates/crawl/engine/thin_refetch.rs`
- `crates/crawl/engine/cdp_render.rs`

Do not introduce duplicate selector lists. The core module remains the single source of truth.

**Step 4: Run tests to verify no regressions**

Run:

```bash
cargo test content::tests -- --nocapture
cargo test crawl::engine -- --nocapture
```

If the second command is too broad or no tests exist for those modules, run:

```bash
cargo check
```

Expected: compile succeeds; content tests remain green.

**Step 5: Commit**

```bash
git add crates/crawl/engine/collector.rs crates/crawl/engine/thin_refetch.rs crates/crawl/engine/cdp_render.rs
git commit -m "fix(crawl): apply boilerplate selectors in recovery render paths"
```

### Task 3: Reconcile Scrape Path And Prevent Drift

**Files:**
- Modify: `crates/core/content.rs`
- Modify: `crates/cli/commands/scrape.rs` only if direct `TransformInput` construction still bypasses `to_markdown()`
- Test: `crates/core/content/tests.rs`

**Step 1: Write the failing test**

Add one regression test proving the public scrape/content conversion path and the direct `to_markdown()` path behave the same for boilerplate stripping when selector config is `None`.

If scrape already routes through `to_markdown()`, keep this as a `content.rs` regression test and do not change `scrape.rs`.

**Step 2: Run it to verify failure**

Run:

```bash
cargo test content::tests -- --nocapture
```

Expected: fail if scrape/content path diverges.

**Step 3: Write minimal implementation**

Ensure all public HTML→markdown paths that should share behavior use the same selector list and whitespace normalization logic. Avoid copy-pasted transform code if a helper can be reused without widening scope.

**Step 4: Run tests**

Run:

```bash
cargo test content::tests -- --nocapture
cargo check
```

Expected: pass.

**Step 5: Commit**

```bash
git add crates/core/content.rs crates/core/content/tests.rs crates/cli/commands/scrape.rs
git commit -m "test(crawl): lock shared boilerplate filtering behavior"
```

### Task 4: End-To-End Verification Against modelcontextprotocol.io

**Files:**
- No code changes required unless verification reveals a missed path
- Inspect output under `.cache/axon-rust/output/` or an explicit temp output dir

**Step 1: Run the focused verification crawl**

Use an isolated output directory so old files do not contaminate inspection.

Run:

```bash
rm -rf /tmp/axon-mcp-docs-verify
mkdir -p /tmp/axon-mcp-docs-verify
./scripts/axon crawl https://modelcontextprotocol.io/specification/2025-03-26 --wait true --max-pages 5 --output-dir /tmp/axon-mcp-docs-verify
```

If crawl infra is not up, start the required services first per project README and re-run.

**Step 2: Inspect produced markdown for prior cruft**

Run:

```bash
rg -n "Skip to main content|Search\\.\\.\\.|Navigation|Model Context Protocol home page|Documentation Extensions Specification Registry Community|Was this page helpful|⌘K|⌘I" /tmp/axon-mcp-docs-verify/markdown
```

Expected: no matches in saved markdown files.

Also inspect the beginning of at least one file:

```bash
sed -n '1,120p' /tmp/axon-mcp-docs-verify/markdown/*.md | sed -n '1,120p'
```

Expected: real content starts near the top; nav/search chrome is absent or materially reduced.

**Step 3: Run targeted semantic verification**

Run:

```bash
rg -n "MCP|protocol|tools|resources|authorization" /tmp/axon-mcp-docs-verify/markdown
```

Expected: core article/spec text remains present, confirming we did not over-strip.

**Step 4: If verification fails, fix before claiming success**

Possible follow-up scope if still noisy:
- add one or two narrowly justified selectors such as `[role="status"]` or `[role="tooltip"]`
- do not add class-substring selectors
- re-run the exact crawl and grep commands above

**Step 5: Final verification summary**

Record the exact commands, grep results, and one representative markdown excerpt in the implementation report or session log.

### Task 5: Final Verification Gate

**Files:**
- Review all modified files from Tasks 1-4

**Step 1: Run the full proof commands**

```bash
cargo test content::tests -- --nocapture
cargo check
./scripts/axon crawl https://modelcontextprotocol.io/specification/2025-03-26 --wait true --max-pages 5 --output-dir /tmp/axon-mcp-docs-verify
rg -n "Skip to main content|Search\\.\\.\\.|Navigation|Model Context Protocol home page|Was this page helpful|⌘K|⌘I" /tmp/axon-mcp-docs-verify/markdown
```

**Step 2: Read the output**

Only claim success if:
- test command exits 0
- `cargo check` exits 0
- crawl completes successfully
- final `rg` returns no matches

**Step 3: Commit**

```bash
git add crates/core/content.rs crates/core/content/tests.rs crates/crawl/engine/collector.rs crates/crawl/engine/thin_refetch.rs crates/crawl/engine/cdp_render.rs docs/plans/2026-03-10-crawl-boilerplate-filtering-restore.md
git commit -m "fix(crawl): restore boilerplate filtering before markdown conversion"
```
