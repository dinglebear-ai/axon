# Crawl Auto-Scope: Two-Segment Documentation Roots — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix `derive_auto_whitelist_pattern` so that crawling `https://ui.shadcn.com/docs/registry` auto-scopes to `/docs/registry` instead of silently falling back to no-scoping (and crawling the entire site).

**Architecture:** The current function strips the last path segment from non-trailing-slash URLs before counting segments; a two-segment path like `/docs/registry` becomes `/docs/` (one segment), which is explicitly excluded from scoping. The fix changes the "leaf file vs directory" classification so that paths with no file extension are treated as directory roots rather than leaf filenames — their full path is used for scoping, not their parent.

**Tech Stack:** Rust, `spider::url::Url`, `url_utils.rs` in `src/crawl/engine/`, sidecar test file `url_utils_tests.rs`.

---

## Background and Decision

### The bug

`derive_auto_whitelist_pattern("https://ui.shadcn.com/docs/registry")` returns `None` today.

Walk through the current logic:

1. `path = "/docs/registry"` — does not end with `/`, so we `rfind('/')` → position 5.
2. `dir_prefix = "/docs/"` — the function interprets `registry` as a leaf filename, takes its parent dir.
3. `segment_count = 1` (`["docs"]`) → falls into the `<= 1` guard → returns `None`.

The crawl runs with no whitelist, fetches `/blocks`, `/charts`, `/docs/installation`, etc., producing 183 docs / 2 193 chunks instead of the intended narrow set under `/docs/registry/`.

### Options considered

| # | Option | Pros | Cons |
|---|--------|------|------|
| A | **Treat extension-less path segments as directories (recommended)** | Matches how web doc sites work — `/docs/registry` is always a section, not a file. Simple, predictable rule. | Breaks if someone crawls a real extension-less file URL (rare; explicit `--url-whitelist` always overrides). |
| B | Require trailing slash to trigger scoping | 100% explicit | Users would need to type `https://ui.shadcn.com/docs/registry/` — breaks natural copy-paste of browser URLs. |
| C | Scope by full path regardless (drop the "leaf file" special case entirely) | Simpler code | `https://docs.python.org/3/library/os.path.html` would scope to `/3/library/os.path.html(/|$)`, which matches nothing. Every `.html`/`.md` crawl start would silently fail. |
| D | Leave as-is, document explicit `--url-whitelist` usage | Zero code change | Continues to surprise users; 183 vs ~12 doc pages is a 15× overcount that degrades search quality. |

### Recommendation: Option A

**Rule change:** When classifying the last path segment, check whether it contains a `.` **after** the last `/`. If it does, treat it as a file (current behavior). If it does not, treat it as a directory — use the full path (plus a trailing `/` for the regex) as the scope prefix.

**Concrete changes:**

| URL | Old behavior | New behavior |
|-----|-------------|--------------|
| `https://ui.shadcn.com/docs/registry` | `None` (no scope) | `^https?://ui\.shadcn\.com/docs/registry(/\|$)` |
| `https://ui.shadcn.com/docs` | `None` (1 segment) | `None` (1 segment — unchanged) |
| `https://docs.python.org/3/library/os.path.html` | `^.../3/library(/\|$)` | `^.../3/library(/\|$)` (unchanged — `.html` has extension) |
| `https://ai.google.dev/api/python/google/generativeai/GenerativeModel` | `^.../api/python/google/generativeai(/\|$)` | `^.../api/python/google/generativeai(/\|$)` (unchanged — no extension, last segment = leaf, parent dir has 4 segments ≥ 2) |
| `https://example.com/a` | `None` | `None` (1 segment — unchanged) |
| `https://example.com/a/b` | `None` (collapses to `/a/`, 1 segment) | `^.../a/b(/\|$)` (2 segments — NOW scoped) |
| `https://example.com/a/b/c.html` | `^.../a/b(/\|$)` | `^.../a/b(/\|$)` (unchanged) |
| `https://example.com/a/b/c` | `None` (collapses to `/a/b/`, 2 segments) | `^.../a/b/c(/\|$)` (3 segments — NOW scoped more precisely) |

The `/docs` single-segment case is intentionally left unscoped — that's the "already broad" case the original code documented. The fix only affects URLs where the last segment has no extension.

---

## Files

| File | Action | What changes |
|------|--------|-------------|
| `src/crawl/engine/url_utils.rs` | **Modify** | `derive_auto_whitelist_pattern`: new extension check to classify the last path segment as file vs directory |
| `src/crawl/engine/url_utils_tests.rs` | **Modify** | Add regression tests for the two-segment no-extension case and extension-present case |
| `CLAUDE.md` (project root) | **Modify** | Update the "Auto path-prefix scoping" gotcha description |
| `src/crawl/CLAUDE.md` | **Modify** | Update the "Auto path-prefix scoping" entry under Link Filter section |

No new files needed. The change is entirely self-contained in `url_utils.rs` and its sidecar test file.

---

## Task 1: Write the failing regression tests

**Files:**
- Modify: `src/crawl/engine/url_utils_tests.rs`

The tests file currently has no tests for `derive_auto_whitelist_pattern`. We add them before touching implementation so we can verify the before/after contrast.

- [ ] **Step 1.1: Add tests to `url_utils_tests.rs`**

Open `src/crawl/engine/url_utils_tests.rs` and add the following block after the last test (after line 163, before the final `}`-less EOF):

```rust
// ── derive_auto_whitelist_pattern ─────────────────────────────────────────────

// Root path → no scope.
#[test]
fn auto_whitelist_root_path_returns_none() {
    assert_eq!(derive_auto_whitelist_pattern("https://example.com/"), None);
    assert_eq!(derive_auto_whitelist_pattern("https://example.com"), None);
}

// Single segment (no trailing slash) — extension-less, treated as directory → 1 segment, no scope.
#[test]
fn auto_whitelist_single_segment_no_extension_returns_none() {
    // "/docs" is one segment — not scoped.
    assert_eq!(derive_auto_whitelist_pattern("https://ui.shadcn.com/docs"), None);
}

// Single segment with trailing slash — still 1 segment, no scope.
#[test]
fn auto_whitelist_single_segment_trailing_slash_returns_none() {
    assert_eq!(derive_auto_whitelist_pattern("https://ui.shadcn.com/docs/"), None);
}

// *** KEY REGRESSION: two-segment extension-less URL must scope to full path ***
#[test]
fn auto_whitelist_two_segment_no_extension_scopes_to_full_path() {
    let result = derive_auto_whitelist_pattern("https://ui.shadcn.com/docs/registry");
    assert!(
        result.is_some(),
        "two-segment extension-less URL must produce a whitelist pattern"
    );
    let pattern = result.unwrap();
    assert_eq!(
        pattern.as_str(),
        r"^https?://ui\.shadcn\.com/docs/registry(/|$)"
    );
}

// Two-segment trailing-slash URL — directory is explicit, scope to full path.
#[test]
fn auto_whitelist_two_segment_trailing_slash_scopes_to_full_path() {
    let result = derive_auto_whitelist_pattern("https://ui.shadcn.com/docs/registry/");
    assert!(result.is_some());
    let pattern = result.unwrap();
    assert_eq!(
        pattern.as_str(),
        r"^https?://ui\.shadcn\.com/docs/registry(/|$)"
    );
}

// Extension-present last segment — treat as file, scope to parent directory.
#[test]
fn auto_whitelist_last_segment_with_extension_scopes_to_parent_dir() {
    let result =
        derive_auto_whitelist_pattern("https://docs.python.org/3/library/os.path.html");
    assert!(result.is_some());
    let pattern = result.unwrap();
    assert_eq!(
        pattern.as_str(),
        r"^https?://docs\.python\.org/3/library(/|$)"
    );
}

// Deep extension-less URL — last three segments, no extension — scopes to full path.
#[test]
fn auto_whitelist_deep_no_extension_scopes_to_full_path() {
    let result = derive_auto_whitelist_pattern(
        "https://ai.google.dev/api/python/google/generativeai/GenerativeModel",
    );
    assert!(result.is_some());
    let pattern = result.unwrap();
    // GenerativeModel has no extension → full path used as scope prefix.
    assert_eq!(
        pattern.as_str(),
        r"^https?://ai\.google\.dev/api/python/google/generativeai/GenerativeModel(/|$)"
    );
}

// Two-segment path where last segment HAS an extension — parent dir has 1 segment → None.
#[test]
fn auto_whitelist_two_segment_with_extension_collapses_to_one_segment_returns_none() {
    // "/docs/index.html" → parent "/docs/" → 1 segment → None.
    assert_eq!(
        derive_auto_whitelist_pattern("https://example.com/docs/index.html"),
        None
    );
}
```

Also add `derive_auto_whitelist_pattern` to the `use super::*;` import (it is already brought in by the wildcard, but this is a note for the implementer — the existing `use super::*;` on line 1 covers it).

- [ ] **Step 1.2: Run the tests — expect them to fail**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test derive_auto_whitelist -- --nocapture 2>&1 | head -60
```

Expected: Several `FAILED` entries. In particular `auto_whitelist_two_segment_no_extension_scopes_to_full_path` fails because `derive_auto_whitelist_pattern("https://ui.shadcn.com/docs/registry")` currently returns `None`. Also `auto_whitelist_deep_no_extension_scopes_to_full_path` and `auto_whitelist_two_segment_with_extension_collapses_to_one_segment_returns_none` may fail depending on current behavior.

- [ ] **Step 1.3: Commit the failing tests**

```bash
cd /home/jmagar/workspace/axon_rust
git add src/crawl/engine/url_utils_tests.rs
git commit -m "test(crawl): add failing regression tests for two-segment auto-scope behavior

bead: axon_rust-b4y"
```

---

## Task 2: Implement the fix in `derive_auto_whitelist_pattern`

**Files:**
- Modify: `src/crawl/engine/url_utils.rs` (lines 127–168)

The change is surgical — only the `dir_prefix` computation changes. The rest of the function (segment counting, regex building) stays identical.

**Current logic (lines 144–150):**
```rust
let dir_prefix = if path.ends_with('/') {
    path.to_string()
} else {
    // rfind('/') always finds at least the leading '/' for absolute paths.
    let slash_pos = path.rfind('/')?;
    path[..=slash_pos].to_string()
};
```

**New logic:** Before stripping the last segment, inspect the last segment. If it contains a `.` (i.e. has a file extension), treat it as a file and use the parent directory. If it has no `.`, treat it as a directory endpoint and use the full path as the scope prefix (appending `/` for the trailing-slash normalization that `trim_end_matches('/')` later undoes).

- [ ] **Step 2.1: Replace the `dir_prefix` computation in `url_utils.rs`**

Locate the block starting at line 127 (`/// Derive a whitelist regex pattern...`). Replace the function body with:

```rust
/// Derive a whitelist regex pattern that scopes a crawl to the directory
/// subtree of `start_url`.
///
/// Returns `None` when the URL path is `/` or a single non-empty segment
/// (e.g. `/docs`), since those are already broad enough — no scoping needed.
///
/// **Leaf-file detection:** The last path segment is treated as a *file* only
/// when it contains a dot (`.`), indicating a file extension such as `.html`,
/// `.md`, or `.pdf`. Extension-less segments (e.g. `/docs/registry`,
/// `/api/GenerativeModel`) are treated as directory endpoints — the full path
/// is used as the scope prefix, not the parent directory.
///
/// Examples:
/// - `https://ui.shadcn.com/docs/registry`
///   → `^https?://ui\.shadcn\.com/docs/registry(/|$)`
/// - `https://docs.python.org/3/library/os.path.html`
///   → `^https?://docs\.python\.org/3/library(/|$)`
/// - `https://ai.google.dev/api/python/google/generativeai/GenerativeModel`
///   → `^https?://ai\.google\.dev/api/python/google/generativeai/GenerativeModel(/|$)`
pub(crate) fn derive_auto_whitelist_pattern(
    start_url: &str,
) -> Option<spider::compact_str::CompactString> {
    let parsed = Url::parse(start_url).ok()?;
    let host = parsed.host_str()?;
    let path = parsed.path();

    // Find the directory prefix: use the full path when the last segment has no
    // file extension (it's a directory endpoint), or strip the last segment when
    // it does have an extension (it's a leaf file).
    let dir_prefix = if path.ends_with('/') {
        // Explicit trailing slash — already a directory path.
        path.to_string()
    } else {
        let slash_pos = path.rfind('/')?;
        let last_segment = &path[slash_pos + 1..];
        if last_segment.contains('.') {
            // Last segment has an extension (e.g. "os.path.html", "index.md") — it's a file.
            // Use the parent directory as the scope prefix.
            path[..=slash_pos].to_string()
        } else {
            // Last segment has no extension (e.g. "registry", "GenerativeModel") — it's a
            // directory endpoint. Use the full path. Append '/' so segment counting works
            // correctly and the trailing-slash strip below is a no-op on the segment count.
            format!("{path}/")
        }
    };

    // Count meaningful segments (non-empty parts after splitting on '/').
    let segment_count = dir_prefix.split('/').filter(|s| !s.is_empty()).count();

    // Root ("/") or single segment ("/docs/") — no auto-scoping.
    if segment_count <= 1 {
        return None;
    }

    // Strip trailing slash from the prefix for the regex (we add `(/|$)` ourselves).
    let prefix_for_regex = dir_prefix.trim_end_matches('/');
    let pattern = format!(
        "^https?://{}{}(/|$)",
        regex_escape(host),
        regex_escape(prefix_for_regex),
    );
    Some(spider::compact_str::CompactString::from(pattern))
}
```

- [ ] **Step 2.2: Run `cargo check` to confirm it compiles**

```bash
cd /home/jmagar/workspace/axon_rust
cargo check --bin axon 2>&1 | tail -10
```

Expected: `Finished` with no errors.

- [ ] **Step 2.3: Run the new tests — expect them to pass**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test derive_auto_whitelist -- --nocapture 2>&1
```

Expected output (all pass):
```
test auto_whitelist_deep_no_extension_scopes_to_full_path ... ok
test auto_whitelist_root_path_returns_none ... ok
test auto_whitelist_single_segment_no_extension_returns_none ... ok
test auto_whitelist_single_segment_trailing_slash_returns_none ... ok
test auto_whitelist_two_segment_no_extension_scopes_to_full_path ... ok
test auto_whitelist_two_segment_trailing_slash_scopes_to_full_path ... ok
test auto_whitelist_last_segment_with_extension_scopes_to_parent_dir ... ok
test auto_whitelist_two_segment_with_extension_collapses_to_one_segment_returns_none ... ok
```

- [ ] **Step 2.4: Run the full url_utils test suite to confirm no regressions**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test url_utils -- --nocapture 2>&1
```

Expected: All existing tests still pass alongside the new ones.

- [ ] **Step 2.5: Run the full engine test suite**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test engine -- --nocapture 2>&1 | tail -20
```

Expected: All engine tests pass.

- [ ] **Step 2.6: Commit the fix**

```bash
cd /home/jmagar/workspace/axon_rust
git add src/crawl/engine/url_utils.rs
git commit -m "fix(crawl): treat extension-less path segments as directory roots in auto-scope

Previously, derive_auto_whitelist_pattern classified any non-trailing-slash
URL segment as a leaf file, stripping it to the parent directory before
counting segments. This caused two-segment paths like /docs/registry to
collapse to /docs/ (one segment), triggering the 'already broad enough'
guard and returning None — no scoping applied.

With this change, only segments that contain a '.' are treated as files.
Extension-less segments (docs/registry, GenerativeModel, etc.) are treated
as directory endpoints and the full path is used as the scope prefix.

Resolves: axon_rust-b4y — crawling https://ui.shadcn.com/docs/registry now
scopes to /docs/registry instead of crawling the entire site."
```

---

## Task 3: Update documentation

**Files:**
- Modify: `CLAUDE.md` (project root) — the "Auto path-prefix scoping" gotcha
- Modify: `src/crawl/CLAUDE.md` — the Link Filter section

- [ ] **Step 3.1: Update the "Auto path-prefix scoping" gotcha in `CLAUDE.md`**

Find the paragraph under `### Auto path-prefix scoping` in the project-root `CLAUDE.md`. It currently reads:

```
When crawling a URL with ≥2 path segments and no explicit `--url-whitelist`, the crawl is automatically scoped to the directory subtree of the start URL via a derived whitelist regex. For example, crawling `https://ai.google.dev/api/python/google/generativeai/GenerativeModel` auto-scopes to `^https?://ai\.google\.dev/api/python/google/generativeai(/|$)`. Root paths (`/`) and single-segment paths (`/docs`) are not scoped — they're already broad enough. Pass `--url-whitelist <pattern>` to override auto-scoping.
```

Replace it with:

```
When crawling a URL with ≥2 path segments and no explicit `--url-whitelist`, the crawl is automatically scoped to the directory subtree of the start URL via a derived whitelist regex. The last path segment is classified as a *file* only when it contains a `.` (file extension). Extension-less segments are treated as directory endpoints and the full path is used. Root paths (`/`) and single-segment paths (`/docs`) are not scoped — they're already broad enough. Pass `--url-whitelist <pattern>` to override auto-scoping.

Examples:
- `https://ui.shadcn.com/docs/registry` → `^https?://ui\.shadcn\.com/docs/registry(/|$)`
- `https://docs.python.org/3/library/os.path.html` → `^https?://docs\.python\.org/3/library(/|$)` (`.html` extension → parent dir)
```

- [ ] **Step 3.2: Update `src/crawl/CLAUDE.md`**

Find the "Auto path-prefix scoping" entry inside the `### Link Filter (`set_on_link_find`)` section of `src/crawl/CLAUDE.md`. Update the description there to match the new behavior (extension-less = directory, dot-containing = file). Add a concrete shadcn example:

```
**2. Auto path-prefix scoping** (`derive_auto_whitelist_pattern` in `url_utils.rs`):

Applied in `configure_website()` **before** the link-find callback. When no explicit `--url-whitelist` is provided and the start URL has ≥2 path segments, a whitelist regex scoping the crawl to that directory subtree is set automatically via `website.with_whitelist_url()`.

**Leaf-file detection:** A segment is treated as a file only when it contains `.` (extension). Extension-less segments are directory endpoints.
- `https://ui.shadcn.com/docs/registry` → scoped to `/docs/registry` (2 segments, no extension)
- `https://docs.python.org/3/library/os.path.html` → scoped to `/3/library` (`.html` = file; parent has 2 segments)
- `https://example.com/docs` → no scope (1 segment)

Single-segment paths (`/docs`) and root paths get no auto-scope. Override by passing `--url-whitelist`.
```

- [ ] **Step 3.3: Run `cargo test url_utils` one more time to confirm nothing regressed**

```bash
cd /home/jmagar/workspace/axon_rust
cargo test url_utils 2>&1 | tail -5
```

Expected: `test result: ok. N passed; 0 failed`.

- [ ] **Step 3.4: Commit the documentation updates**

```bash
cd /home/jmagar/workspace/axon_rust
git add CLAUDE.md src/crawl/CLAUDE.md
git commit -m "docs(crawl): document extension-less segment = directory rule for auto-scope"
```

---

## Task 4: Full quality gate and version bump

- [ ] **Step 4.1: Run `just verify`**

```bash
cd /home/jmagar/workspace/axon_rust
just verify 2>&1 | tail -30
```

Expected: `fmt-check`, `clippy`, `check`, `test` all pass. If clippy complains, fix before continuing.

- [ ] **Step 4.2: Bump patch version**

This is a `fix:` commit series → patch bump. Current version is in `Cargo.toml` (`version = "X.Y.Z"`). Find the current version:

```bash
grep '^version' /home/jmagar/workspace/axon_rust/Cargo.toml | head -1
```

Increment the patch number (e.g. `0.35.1` → `0.35.2`) in `Cargo.toml`. Also update `CHANGELOG.md` with a new entry:

```markdown
## [X.Y.Z+1] - 2026-05-21

### Fixed
- `derive_auto_whitelist_pattern` now treats extension-less path segments as
  directory endpoints. Crawling `https://ui.shadcn.com/docs/registry` now
  auto-scopes to `/docs/registry` instead of falling back to no-scoping and
  crawling the entire site. (#axon_rust-b4y)
```

- [ ] **Step 4.3: Commit the version bump**

```bash
cd /home/jmagar/workspace/axon_rust
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore: bump version to X.Y.Z+1"
```

- [ ] **Step 4.4: Close the bead**

```bash
bd close axon_rust-b4y
```

- [ ] **Step 4.5: Push**

```bash
cd /home/jmagar/workspace/axon_rust
git pull --rebase
git push
```

---

## Self-Review Checklist

**Spec coverage:**
- [x] Documents all four options (scope to full path, require trailing slash, explicit whitelist, leave as-is) — covered in "Options considered" table.
- [x] Recommends Option A with rationale — covered.
- [x] Specifies the exact code change — Task 2 shows complete replacement of the function with diff-level specificity.
- [x] Covers the concrete shadcn regression case — test in Task 1 `auto_whitelist_two_segment_no_extension_scopes_to_full_path`.
- [x] Documents impact on existing behavior (deep paths with extension, single-segment, trailing slash) — behavior table and tests cover all cases.

**Placeholder scan:** No TBD, no "implement later", no "handle edge cases" — all steps have exact code or exact commands.

**Type consistency:** `derive_auto_whitelist_pattern` is the only changed public symbol; its signature (`&str → Option<CompactString>`) is unchanged throughout all tasks.

**Edge cases verified by tests:**
- Root `/` → `None` ✓
- Single segment no extension → `None` ✓  
- Single segment trailing slash → `None` ✓
- Two-segment no extension → pattern ✓ (regression fix)
- Two-segment trailing slash → pattern ✓
- Two-segment with extension (parent = 1 seg) → `None` ✓
- Extension in last segment (deep path) → parent dir ✓
- Deep extension-less → full path ✓
