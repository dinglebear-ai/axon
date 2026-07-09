# Palette GitHub View Enhancements — Feed Tab + Two-Pane Split Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a real, authenticated cross-repo "Feed" activity view to the palette's GitHub browser, and convert the existing sequential repo→tree→file navigation into a two-pane split (tree sidebar + live file preview), so a Feed item that names a file can jump straight into the split view at that file.

**Architecture:** Both features are additive to the existing `github` palette action and its `github_browse` Tauri bridge (`apps/palette-tauri/src-tauri/src/github_bridge.rs`) — no new Tauri command surface, no new action registry entry. The Feed adds a new `GitHubRequestKind::Feed` variant that calls GitHub's REST **Events API** per-repo (chosen over the Notifications API — see "Data source decision" below), aggregated across the repos returned by the existing `ListRepos` call, normalized into a flat list of typed `FeedItem`s, grouped client-side by day. The two-pane split replaces `GitHubView`'s `history: GitHubBrowseResult[]` navigation stack with a `FilesView`-style `{ tree, selected }` state pair — this is not a novel pattern, it's copying an existing, working precedent already in this codebase (`apps/palette-tauri/src/components/palette/FilesView.tsx`'s `.files-body` / `.files-tree` / `.files-preview` layout). "Back" is redefined as "return to the repo list" (the one navigation hop that still exists), not "undo the last click."

**Tech Stack:** React 19 + TypeScript (palette frontend), Rust + `reqwest` + `tauri::command` (desktop bridge), Vitest + Testing Library (frontend tests), Rust `#[cfg(test)]` sidecar tests (bridge).

## Global Constraints

- **Module/test conventions differ by side of the app.** Rust changes in `apps/palette-tauri/src-tauri/` use the repo-root sidecar convention: a new/changed `foo.rs` gets tests in a sibling `foo_tests.rs` declared via `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` — never inline `#[cfg(test)] mod tests { ... }` and never `foo/mod.rs`. TypeScript changes in `apps/palette-tauri/src/` use co-located `*.test.ts(x)` files next to their source (see `apps/palette-tauri/CLAUDE.md`).
- **No second button form.** `apps/palette-tauri/CLAUDE.md` mandates exactly one canonical button primitive, `src/components/ui/aurora/button.tsx` (`Button` with `variant`/`size` props). Every new interactive element in this plan (Feed row click targets, the split-pane's file rows, a "Back to repos" affordance) must reuse `Button` or the existing bespoke `<button className="github-*">` pattern already established in `GitHubView.tsx` — do not invent a new button component.
- **Token discipline.** All new colors/spacing go through `var(--aurora-*)` CSS custom properties in `src/styles.css`. No raw hex.
- **All Aurora primitives** are installed via the `@aurora` shadcn registry — do not hand-roll a component that already exists there (e.g. don't build a new "day divider" primitive if a suitable list/section heading already exists — reuse `.stats-heading`-style patterns from `OperationResultViewShared.tsx`).
- **`GitHubBrowseResult` is a stable wire contract** shared between `src-tauri/src/github_bridge.rs` (Rust, `#[serde(rename_all = "camelCase")]`) and `src/lib/actionRequest.ts` (`GitHubBrowseResult` interface) and consumed by `GitHubView.tsx`. Extending it (e.g. adding a `feed` payload shape) must keep both sides in lockstep; there is no OpenAPI-drift generator for this bridge (it's a Tauri IPC command, not an Axon `/v1/*` HTTP route), so the discipline is manual — a Rust struct field rename with no matching TS interface update is a silent runtime bug, not a compile error.
- **The dev/browser fallback in `src/lib/invoke.ts` (`githubBrowseDevFallback`) must be kept in sync** with every new `kind` the Rust bridge accepts, per its own header comment ("Browser-dev-only mirror of `github_bridge.rs::github_browse` — same URL shapes"). Inside that function, `kind` branching is an `if`/`else if` chain (not the top-level `invoke()` command dispatcher, which is a `switch (command)` — the two are structurally different and this plan only ever touches the inner `if`/`else if` chain). It is always unauthenticated (60 req/hr, no token) — this is acceptable for local dev iteration but the Feed's real authenticated behavior can only be verified through the actual Tauri shell.
- **CSP boundary.** The renderer's `connect-src` CSP has no `api.github.com` origin (see `github_bridge.rs` header comment). All new GitHub Events API calls MUST go through the Rust bridge — never add a direct `fetch()` in `GitHubView.tsx` or any new Feed component.
- **Monolith policy.** Changed `.rs` files: ≤500 lines / function ≤120 lines (hard fail), enforced by CI + lefthook. `github_bridge.rs` is currently 388 lines: budget the Feed addition carefully or split a `github_bridge/feed.rs` submodule (see Task 2 for the exact split).
- **Rate limits are real and unauthenticated GitHub is 60 req/hr.** The Feed fans out one Events API call per repo — this plan bounds that fan-out (see Task 3) and requires `GITHUB_TOKEN` to be practically usable across more than a couple of repos (5,000 req/hr authenticated vs 60 unauthenticated). Do not skip the rate-limit-aware sequencing in Task 3 "to keep it simple."

---

## Background reading (do this before Task 1)

Read these in full before writing code — the plan below assumes you have:

1. `apps/palette-tauri/src/components/palette/GitHubView.tsx` — the current sequential view (296 lines). Note the `history: GitHubBrowseResult[]` stack (`go()`/`goBack()`), the `githubTitle()` switch, and that `FilePreview`/`TreeView`/`RepoListView` are already separate sub-components — you will reuse two of these three almost unchanged.
2. `apps/palette-tauri/src-tauri/src/github_bridge.rs` — the Rust bridge (388 lines). Note `GitHubRequestKind` (4 variants today), `build_request_url()` (one match arm per kind), `validate_segment`/`validate_file_path` (reuse, don't reinvent), and `describe_error()` (rate-limit-aware error strings you must extend for Events API rate limiting, which uses the *primary* rate limit, same headers).
3. `apps/palette-tauri/src/lib/invoke.ts` — the browser-dev fallback `githubBrowseDevFallback()`. It duplicates the Rust bridge's URL-building logic in TypeScript for use under `pnpm vite:dev` with no Tauri shell. Its own comment says it must mirror the Rust command's shapes. Inside this function specifically, `kind` dispatch is an `if`/`else if` chain — do not confuse this with the top-level `invoke()` command dispatcher elsewhere in the same file, which is a `switch (command)` statement; Task 5 only touches the inner `if`/`else if` chain.
4. `apps/palette-tauri/src/lib/actionRequest.ts` lines ~130–190 — `githubBrowseBody`, `GitHubBrowseRequestBody`, `GitHubBrowseResult` (the shared wire-contract interface), `parseGitHubTarget()` (parses the palette argument `owner[/repo[/path]]` into a request).
5. `apps/palette-tauri/src/lib/actionRegistryEntries.ts` — the `github:` entry (`route: getRoute("palette://github")`, `buildBody: githubBrowseBody`, `structuredView: "github"`). Note the comment: `github` is NOT an Axon REST call, the route is an inert `palette://` marker, and `executeAction` in `axonClient.ts` special-cases `subcommand === "github"` to call `github_browse` directly instead of `axon_http_request`.
6. `apps/palette-tauri/src/lib/axonClient.ts` lines ~95–140 — `executeAction()`'s `github` special case and `executeGitHubBrowse()`.
7. `apps/palette-tauri/src/components/palette/FilesView.tsx` (373 lines) — **this is the two-pane precedent you are copying the shape of.** Note `.files-body` (flex container) → `.files-tree` (left, `role="listbox"`, one `<button role="option">` per entry, `files-row-active` class on the selected row) + `.files-preview` (right, renders based on a `LoadState<FileContents>` union). Note there is **no navigation stack** here — just `cwd` (current directory string) + `selected: FileEntry | null` + a `file: LoadState<FileContents>` loaded on selection. This is the state shape Task 6 ports into `GitHubView`.
8. `apps/palette-tauri/src/lib/toolTabs.ts` and `useToolTabs.ts` — **read the header comments closely.** `github` is a `ToolKind` but is explicitly **excluded** from `TILEABLE_KINDS` (`= ["files", "terminal"]`) with the comment: only self-contained views with no `RunState` dependency can be a split partner in the *outer* multi-tool tiling system. This plan's "two-pane split" is a **different, inner concept**: a split *within* the `github` view's own file-tree/preview area, not adding `github` to `TILEABLE_KINDS`. Do not conflate the two — Task 6 does not touch `toolTabs.ts`/`useToolTabs.ts` at all.
9. `apps/palette-tauri/src/components/palette/OperationResultViewShared.tsx` — reuse `ResultHero`, `DetailLine`, `EmptyResult`, `StatusDot`, `toneForStatus` rather than inventing new chrome.
10. `apps/palette-tauri/src/lib/filesModel.ts` — reuse `fileKind()`, `formatBytes()`, `isMarkdownLike()` (or the near-identical `isMarkdownPath()` already duplicated in `GitHubView.tsx` — consolidate per Task 6's cleanup step) rather than re-deriving file-type icon logic.
11. `apps/palette-tauri/src/components/palette/GitHubView.test.tsx` — the existing Vitest suite (7 tests) you must keep green while refactoring, updating only the assertions that assert on the removed "Back means undo-last-click" behavior (the "shows a Back button after drilling and returns to the previous view" test, see Task 6).
12. `apps/palette-tauri/src-tauri/src/persistence.rs` — `read_default_env_entries()` / `value_for()`, the existing `GITHUB_TOKEN` read path already used by `github_token()` in `github_bridge.rs`. The Feed reuses this unchanged — it does NOT need a new env var.

**Reference mock note:** The task that produced this plan referenced a static prototype `palette-mock.html` (its `feedView()`/`feedRow()`/`FEED_KIND`/`FEED` object, and its `walk()`/`pvBody`/`pvHead`/`pvFoot` split-view) as the design target. **The mock is now available** at `./palette-mock.html` in the repo root (~3 MB, single-line minified JS embedded in HTML — greps and decodes cleanly; search for `feedView`, `feedRow`, `FEED_KIND`, `var FEED`, `walk(`, `pvBody`, `pvHead`, `pvFoot`). It was not available during the first drafting pass, so that pass reconstructed the Feed taxonomy and split-view chrome from (a) the task brief's prose description, (b) the real, current `GitHubView.tsx`/`github_bridge.rs` implementation, and (c) the closest working precedent already in the codebase (`FilesView.tsx`'s two-pane layout). **The reconstruction has since been diffed against the real mock and corrected in place** in Tasks 3, 5, 7, and 8 below — see each task for the specific corrections (the `FEED_KIND` taxonomy, `FeedItem` field shape, `feedRow()` markup, the Feed/Repos segmented control, and the `pvHead`/`pvFoot` preview-pane actions all needed changes). This note and "Open Questions" #1 below record what was corrected and why; there is no more outstanding mock-vs-plan gap to resolve before implementation.

---

## Data source decision: GitHub Events API, not Notifications API

Two real GitHub APIs could back the Feed. This plan commits to the **Events API**, per-repo, for these reasons — record this reasoning in the code comment in Task 2 so a future engineer doesn't re-litigate it:

| | **Events API** (`GET /repos/{owner}/{repo}/events`) | **Notifications API** (`GET /notifications`) |
|---|---|---|
| Scope | Public timeline of activity on repos you choose | Only things GitHub decided to notify *you* about (subscriptions, mentions, participation) |
| Matches "cross-repo activity feed: PRs, merges, reviews, issues, pushes, releases, dependency bumps" | Yes — `PushEvent`, `PullRequestEvent`, `IssuesEvent`, `ReleaseEvent`, `PullRequestReviewEvent` are all real event types this endpoint returns | Partial — notifications are filtered to "things GitHub thinks you care about," not deletable by unsubscribed repos, and needs `notifications` OAuth scope which `GITHUB_TOKEN` (a PAT) may or may not carry |
| Works with the existing `ListRepos` call (`GET /users/{owner}/repos`) that already drives the repo-list view | Yes — same owner, same repo set, one more call per repo | No — notifications are a completely separate global inbox, unrelated to the `owner` the palette is currently browsing |
| Auth requirement | Works unauthenticated (public repos, 60 req/hr) or authenticated (5,000 req/hr) | Requires authentication always (401 without a token) |
| Rate-limit shape | Same `x-ratelimit-*` headers already handled by `describe_error()`/`header_u32`/`header_i64` | Same headers, but a *different* rate-limit bucket in some GitHub Enterprise configurations |
| Dependency bumps (Dependabot) | Surfaces as a `PushEvent` (commit message `Bump X from Y to Z`) — has to be detected heuristically from commit messages, not a distinct event type | Same heuristic problem, no better |

**Decision: Events API, per-repo, fanned out across the repos returned by the existing `ListRepos` (or `RepoInfo` for a single repo) call.** This keeps the Feed scoped to "activity on repos owned by the `owner` the user is browsing" — consistent with how `github <owner>` already scopes everything else in this view. It does NOT attempt a true GitHub-wide "your activity across every repo you've ever touched" feed (that would need the Notifications API or the authenticated user's own event stream `GET /users/{user}/events` and a different mental model) — flagged as an explicit **out-of-scope / open question** below, not silently narrowed.

---

## File Structure

**Rust bridge (`apps/palette-tauri/src-tauri/src/`):**
- Modify: `github_bridge.rs` — add `GitHubRequestKind::Feed`, `FeedRequest`/`FeedItem`/`FeedResult` types, wire the new kind into `parse_kind`/`build_request_url`/`github_browse`. Actual per-repo fan-out + event normalization moves to a new submodule to keep `github_bridge.rs` under the monolith cap.
- Create: `github_feed.rs` — the Events-API fan-out, per-event normalization into `FeedItem`, day-grouping is NOT done here (grouping is a presentation concern, done in TS — see below); this file owns "call GitHub N times, merge, sort by timestamp desc, cap, return."
- Create: `github_feed_tests.rs` — sidecar tests for `github_feed.rs` (event-shape parsing, rate-limit short-circuit behavior, cap enforcement).
- Modify: `github_bridge_tests.rs` — add cases for the new `Feed` kind reaching `parse_kind`/`build_request_url` (URL-building only; the fan-out itself is tested in `github_feed_tests.rs`).

**TypeScript frontend (`apps/palette-tauri/src/`):**
- Modify: `lib/actionRequest.ts` — extend `GitHubBrowseRequestBody`'s `kind` union with `"feed"`, extend `GitHubBrowseResult` if the feed payload needs new echoed fields (it doesn't — `payload` is already `unknown`/generic).
- Modify: `lib/invoke.ts` — extend `githubBrowseDevFallback()` with a `"feed"` branch (fan-out done client-side in the dev fallback since there's no Rust process to delegate to; capped and clearly commented as dev-only, unauthenticated, small-N).
- Create: `lib/githubFeed.ts` — pure TS: `FeedItem` type (mirrors the Rust `FeedItem` struct field-for-field), `groupFeedByDay()` (Today/Yesterday/Earlier grouping — the one piece of "grouping" logic that lives in TS, matching the mock's client-side grouping since the mock had no backing API to do it server-side either), `feedKindLabel()`/`feedKindIcon()` mapping GitHub event types to the mock's real `FEED_KIND` taxonomy (`pr`/`merge`/`review`/`comment`/`conflict`/`deps`/`issue`/`push`/`release` — see Task 3 for the exact label strings and which of these this plan's Events-API source can and cannot populate).
- Create: `lib/githubFeed.test.ts` — co-located Vitest tests for `groupFeedByDay()` and the event-type→`FeedItem` normalization boundary (testing the TS-side shaping of what the Rust bridge already normalized, i.e. a round-trip fixture test, not re-testing Rust logic).
- Create: `components/palette/GitHubFeedView.tsx` — the Feed tab's renderer: day-grouped list, one row per `FeedItem`, click-through wired to open the split view (Task 8) when the item names a file, otherwise opens the repo/tree.
- Create: `components/palette/GitHubFeedView.test.tsx` — co-located render tests (day grouping renders correctly, empty state, error state, click navigates).
- Modify: `components/palette/GitHubView.tsx` — (a) add a Feed/Browse 2-option segmented switcher at the top of the view (matching the mock's `ghSeg()` pill control, not an underlined tab bar — see Task 8), (b) replace the `history` stack with `{ owner, repo, branch, selected, tree: LoadState<...>, file: LoadState<...> }` state modeled on `FilesView.tsx`, (c) render `.github-body` (`.github-tree` + `.github-preview`) instead of swapping `TreeView`/`FilePreview` sequentially.
- Modify: `components/palette/GitHubView.test.tsx` — update the "Back" test to assert "Back returns to repo list," add tests for simultaneous tree+preview rendering, add a test for the Feed tab switch.
- Modify: `src/styles.css` — add `.github-seg`/`.github-seg-btn` (a 2-option pill switcher matching the real mock's `ghSeg()` chrome — see Task 8's "Corrected against the real mock's feedView() header chrome" note; NOT an underlined tab bar), `.github-body`/`.github-tree` (mirrors `.files-body`/`.files-tree` almost verbatim), `.github-preview-head`/`.github-preview-foot` (the mock's `pvHead`/`pvFoot` file-preview action row and footer strip — see Task 6), `.github-feed-*` (day-group heading + mock-matched row layout: icon swatch, repo/kind/num header line, meta line with actor chip + badge — see Task 7).
- No change needed: `lib/actionRegistryEntries.ts`, `lib/axonClient.ts`, `components/palette/OperationResultView.tsx` — the `github` action's registry entry, route marker, and structured-view wiring are unchanged; both new capabilities live *inside* `GitHubView`'s existing `structuredView: "github"` slot.

---

## Task 1: Extend the wire-contract types (no behavior yet)

**Files:**
- Modify: `apps/palette-tauri/src/lib/actionRequest.ts:147-152` (the `GitHubBrowseRequestBody` type)
- Modify: `apps/palette-tauri/src/lib/actionRequest.ts:181-190` (`parseGitHubTarget`)
- Test: `apps/palette-tauri/src/lib/actionRequest.test.ts` (check if this file exists; if not, create it — the file currently has no co-located test)

**Interfaces:**
- Produces: `GitHubBrowseRequestBody.kind` union gains `"feed"`. `parseGitHubTarget()` gains a feed-triggering input shape.

This task only touches the **request-building** side (what the palette sends), not the response shape (Task 2 covers that in Rust, Task 4 covers the TS mirror). The palette needs a way for a user to type `github <owner> --feed` or similar to land on the Feed tab directly, OR (simpler, matches how the mock's tabs work) the Feed is *always* reachable as a tab inside the existing `github <owner>` / `github <owner>/<repo>` result — no new argument syntax needed. **This plan chooses the tab approach** (no new CLI-style argument), because:
1. It matches the mock's UI (`feedView()` is a tab within the same result panel, not a separately-dispatched action).
2. It requires zero changes to `actions.ts`/`actionRegistryEntries.ts`/`ActionBehavior` — the `github` subcommand's `buildBody`/`route` stay exactly as they are today, since the *initial* request is unchanged (`repos` or `tree` or `file`, exactly as now); the Feed is fetched lazily when the user clicks the Feed tab, via a **second** `github_browse` call with `kind: "feed"` issued directly from `GitHubView.tsx` (same pattern as `onOpenRepo`/`onOpenFile` already issuing follow-up `browse()` calls today).

Given that, `parseGitHubTarget()` needs **no changes** — skip that file. The only type change needed here is widening the discriminated `kind` your bridge call sites can request.

- [ ] **Step 1: Widen `GitHubBrowseRequestBody.kind`**

In `apps/palette-tauri/src/lib/actionRequest.ts`, change:

```ts
export type GitHubBrowseRequestBody = {
  kind: "repos" | "tree" | "file";
  owner: string;
  repo?: string;
  path?: string;
} & Record<string, unknown>;
```

to:

```ts
export type GitHubBrowseRequestBody = {
  kind: "repos" | "tree" | "file" | "feed";
  owner: string;
  repo?: string;
  path?: string;
} & Record<string, unknown>;
```

`parseGitHubTarget()` itself is unchanged (it never produces `kind: "feed"` — that request is built directly in `GitHubView.tsx`, see Task 7).

- [ ] **Step 2: Typecheck**

Run: `cd apps/palette-tauri && pnpm typecheck`
Expected: PASS (the widened union is backward compatible; nothing currently switches exhaustively on `kind` in a way that would break — verify by grepping)

Run: `grep -rn 'kind === "file"\|kind: "file"\|case "file"' apps/palette-tauri/src --include='*.ts' --include='*.tsx'`
Expected: confirm every switch/if-chain on `GitHubBrowseRequestBody`/`GitHubBrowseResult`'s `kind` has a `default`/fallthrough branch (they do — `githubTitle()`'s `switch` has a `default: "GitHub"`, and `GitHubView`'s render ternary chain falls through to `FilePreview` for anything that isn't `"repos"`/`"tree"` — this needs revisiting in Task 7 once `"feed"` becomes a real reachable state).

- [ ] **Step 3: Commit**

```bash
git add apps/palette-tauri/src/lib/actionRequest.ts
git commit -m "feat(palette): widen GitHub browse request kind to include feed"
```

---

## Task 2: Rust bridge — `GitHubRequestKind::Feed` request shape + routing (no fan-out yet)

**Files:**
- Modify: `apps/palette-tauri/src-tauri/src/github_bridge.rs:60-70` (`GitHubRequestKind` enum)
- Modify: `apps/palette-tauri/src-tauri/src/github_bridge.rs:122-130` (`parse_kind`)
- Modify: `apps/palette-tauri/src-tauri/src/github_bridge.rs:172-215` (`build_request_url`)
- Modify: `apps/palette-tauri/src-tauri/src/github_bridge_tests.rs`

**Interfaces:**
- Consumes: nothing new (uses existing `validate_segment`, `GITHUB_API_BASE`).
- Produces: `GitHubRequestKind::Feed` variant; `build_request_url` returns a per-repo Events URL for it. This task builds the URL for **one repo** — the per-owner fan-out across multiple repos is Task 3's job, in the new `github_feed.rs`.

This task is deliberately small and mechanical: prove the new enum variant threads through the existing dispatch before adding any real fan-out logic.

- [ ] **Step 1: Add the enum variant**

In `github_bridge.rs`, change:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitHubRequestKind {
    /// `GET /users/{owner}/repos` — repos owned by a user or org.
    ListRepos,
    /// `GET /repos/{owner}/{repo}` — repo metadata (default branch, description, …).
    RepoInfo,
    /// `GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1` — full file tree.
    Tree,
    /// `GET /repos/{owner}/{repo}/contents/{path}` — a single file (base64 content).
    FileContents,
}
```

to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitHubRequestKind {
    /// `GET /users/{owner}/repos` — repos owned by a user or org.
    ListRepos,
    /// `GET /repos/{owner}/{repo}` — repo metadata (default branch, description, …).
    RepoInfo,
    /// `GET /repos/{owner}/{repo}/git/trees/{branch}?recursive=1` — full file tree.
    Tree,
    /// `GET /repos/{owner}/{repo}/contents/{path}` — a single file (base64 content).
    FileContents,
    /// `GET /repos/{owner}/{repo}/events` — one repo's public event timeline, the
    /// building block for the cross-repo Feed. Unlike the other three variants,
    /// a single `Feed` browse request fans this out across every repo the owner
    /// has (see `github_feed.rs::fetch_feed`) rather than hitting one URL — so
    /// `build_request_url` below returns the single-repo URL shape used by that
    /// fan-out helper, not a URL `github_browse` calls directly for this kind.
    Feed,
}
```

- [ ] **Step 2: Wire `parse_kind`**

Change:

```rust
fn parse_kind(raw: &str) -> Result<GitHubRequestKind, String> {
    match raw {
        "repos" => Ok(GitHubRequestKind::ListRepos),
        "repo" => Ok(GitHubRequestKind::RepoInfo),
        "tree" => Ok(GitHubRequestKind::Tree),
        "file" => Ok(GitHubRequestKind::FileContents),
        other => Err(format!("unknown GitHub browse kind: {other}")),
    }
}
```

to:

```rust
fn parse_kind(raw: &str) -> Result<GitHubRequestKind, String> {
    match raw {
        "repos" => Ok(GitHubRequestKind::ListRepos),
        "repo" => Ok(GitHubRequestKind::RepoInfo),
        "tree" => Ok(GitHubRequestKind::Tree),
        "file" => Ok(GitHubRequestKind::FileContents),
        "feed" => Ok(GitHubRequestKind::Feed),
        other => Err(format!("unknown GitHub browse kind: {other}")),
    }
}
```

- [ ] **Step 3: Add the per-repo events URL builder to `build_request_url`**

Add a new match arm (this URL is used internally by `github_feed.rs`, not dispatched to directly by `github_browse`'s main match — see Step 5):

```rust
        GitHubRequestKind::Feed => {
            let repo = validate_segment(request.repo.as_deref().unwrap_or_default(), "repo")?;
            Ok(format!(
                "{GITHUB_API_BASE}/repos/{owner}/{repo}/events?per_page=30"
            ))
        }
```

Insert this arm into the existing `match kind { ... }` block in `build_request_url`, after the `FileContents` arm.

- [ ] **Step 4: Run existing tests to confirm nothing broke**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml github_bridge`
Expected: PASS (all existing tests still pass — this step only added a variant + two match arms, no behavior change to existing kinds)

- [ ] **Step 5: Add a test for the new URL shape**

In `github_bridge_tests.rs`, add (following the existing `req(...)` helper pattern already in that file):

```rust
#[test]
fn feed_kind_builds_per_repo_events_url() {
    let request = req("feed", "jmagar", Some("axon"), None, None);
    let url = build_request_url(&request, GitHubRequestKind::Feed).unwrap();
    assert_eq!(url, "https://api.github.com/repos/jmagar/axon/events?per_page=30");
}

#[test]
fn feed_kind_requires_repo() {
    let request = req("feed", "jmagar", None, None, None);
    let result = build_request_url(&request, GitHubRequestKind::Feed);
    assert!(result.is_err());
}

#[test]
fn parse_kind_accepts_feed() {
    assert_eq!(parse_kind("feed").unwrap(), GitHubRequestKind::Feed);
}
```

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml github_bridge`
Expected: PASS, 3 new tests green

- [ ] **Step 6: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/github_bridge.rs apps/palette-tauri/src-tauri/src/github_bridge_tests.rs
git commit -m "feat(palette-bridge): add GitHubRequestKind::Feed URL routing"
```

---

## Task 3: Rust bridge — `github_feed.rs` fan-out + event normalization

**Files:**
- Create: `apps/palette-tauri/src-tauri/src/github_feed.rs`
- Create: `apps/palette-tauri/src-tauri/src/github_feed_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/src/lib.rs` (register the new module)

**Interfaces:**
- Consumes: `GitHubClient` (existing shared `reqwest::Client` wrapper from `github_bridge.rs`), `github_token()` (existing token lookup), `validate_segment` (existing).
- Produces: `pub(crate) struct FeedItem { kind: String, repo: String, actor: String, title: String, url: String, path: Option<String>, num: Option<u64>, meta: String, badge: Option<FeedBadge>, timestamp_unix: i64 }`, `pub(crate) enum FeedBadge { Diff { add: u32, del: u32 }, Label { value: String } }` (both variants are struct-shaped — see this task's implementation note on why `Label(String)`, a newtype variant, cannot be used with `#[serde(tag = "type")]`), `pub(crate) struct FeedFetchResult { items: Vec<FeedItem>, rate_limit_remaining: Option<u32>, rate_limit_reset: Option<i64>, partial: bool, errors: Vec<String> }`, `pub(crate) async fn fetch_feed(client: &reqwest::Client, owner: &str, repos: &[String], token: Option<&str>) -> FeedFetchResult`. `FeedItem`/`FeedBadge`/`FeedFetchResult` are consumed by Task 4 (wiring into `github_browse`).

**Mock-verified taxonomy (corrected from the first drafting pass):** `palette-mock.html`'s real `FEED_KIND` object (see the plan's top-level "Reference mock note") defines exactly 9 kinds, each with a `label` string:

| kind | label | this plan's Events-API source |
|---|---|---|
| `pr` | `Pull Request` | `PullRequestEvent`, not closed+merged |
| `merge` | `Merged` | `PullRequestEvent`, `action == "closed" && pull_request.merged == true` |
| `review` | `Review` | `PullRequestReviewEvent` |
| `comment` | `Comment` | **not sourced by this plan — see below** |
| `conflict` | `Conflict` | **not sourced by this plan — see below** |
| `deps` | `Dependencies` | `PushEvent` where the actor is a bot or the commit message looks like a dependency bump (see below) |
| `issue` | `Issue` | `IssuesEvent` |
| `push` | `Push` | `PushEvent`, all other cases |
| `release` | `Release` | `ReleaseEvent` |

The first drafting pass (done before the mock was available) invented a `"dependency-bump"` kind with a different label set (`"Push"/"Pull request"/"Merged"/"Review"/"Issue"/"Release"/"Dependency bump"`, and no `comment`/`conflict`/`deps` at all). That is now corrected: the kind name is `deps` (not `dependency-bump`) with label `"Dependencies"`, and `pr`'s label is `"Pull Request"` (capital R, per the mock — not `"Pull request"`).

`comment` and `conflict` are **explicitly out of scope for this plan's Events API source**, not silently dropped:
- `comment` (mock label `"Comment"`) would map to `IssueCommentEvent`/`PullRequestReviewCommentEvent`/`CommitCommentEvent` in the real Events API. This plan does not fan those event types into the Feed in this pass — the mock's fixture data includes one `comment` row (a `tei-rs` PR discussion) but wiring three more event types through `normalize_event` is additional scope. `feedKindLabel()`/`feedKindIcon()` (Task 5) still register the `comment` kind (so the taxonomy stays forward-compatible and TS code that switches on `kind` doesn't need a second pass later), but `normalize_event` (this task) never emits it.
- `conflict` (mock label `"Conflict"`) has no clean source in the Events API at all — GitHub does not emit a "merge conflict detected" event; the mock's `conflict` row is fixture data representing something a real integration would need to compute out-of-band (e.g. by calling `GET /repos/{owner}/{repo}/pulls/{n}` and checking `mergeable_state`, one extra API call per open PR). Out of scope for this plan. `feedKindLabel()`/`feedKindIcon()` register the kind for forward-compatibility; `normalize_event` never emits it.

This is a real, load-bearing gap versus the mock's visual fixture — call it out to Jacob/reviewers before shipping the Feed tab as "matches the mock," since 2 of 9 kinds (and their glyphs) will simply never appear from live data in this pass.

- **`FeedItem` field shape — corrected to match the mock's real `FEED` array items**, which are `{ kind, repo, title, actor, num, time, day, meta, badge }` (see the mock's `var FEED = [...]` literal). The first drafting pass's `FeedItem` (`kind, repo, actor, title, url, path, timestamp_unix`) is missing three real fields, now added: `num: Option<u64>` (the PR/issue number, e.g. mock's `num:412` — `None` for pushes/releases, which the mock also sets to `null`), `meta: String` (a short freeform descriptive line, e.g. `"opened · main ← feat/research"`, `"1 update · security advisory"`, `"tagged v5.19.0 · 41 commits"` — populated per event type in `normalize_*` below), and `badge: Option<FeedBadge>` where `FeedBadge` is either a line-diff (`{add, del}`, e.g. merged PRs/pushes with commit stats) or a short status label (`"Approved"`, `"Closed"`, `"Bug"`, `"Latest"`, `"Patch"`, per the mock's `feedBadge()` renderer). The mock's `time`/`day` fields are presentation-only (relative-time string and precomputed day bucket) computed from a raw timestamp in the mock's static fixture — this plan keeps computing those client-side from `timestamp_unix` (Task 5's `groupFeedByDay`), not server-side, since "relative to now" is a viewer-clock concern the Rust bridge has no business owning; only `num`/`meta`/`badge` — genuinely per-event content, not derived from wall-clock — are added to the Rust-side `FeedItem`.
- **Open design question — can the real API populate `badge`/`meta` for every kind?** The mock's `badge`/`meta` values are static fixture data, not necessarily representative of what's cheaply available from a single Events API call. This plan populates them on a best-effort basis per event type (see `normalize_*` below: `merge` computes `{add, del}` from `payload.pull_request.additions/deletions` when present on the event payload — note this field is **not actually present on `PullRequestEvent` payloads from the Events API**, only from a direct `GET /repos/{o}/{r}/pulls/{n}` call, so in practice `badge` will be `None` for merges unless a follow-up task adds that extra per-PR fetch; `release` sets `badge: Some(Label("Latest"))` only when the release is flagged latest in the payload; `issue`/`review` currently have no reliable per-event `badge` source and get `badge: None`). Document this gap plainly rather than claiming full mock parity — it is flagged again in "Open Questions."
- **Repo list source:** the Feed needs to know which repos to fan out across. Rather than re-fetching `ListRepos` inside `fetch_feed` (which would duplicate the caller's already-fetched repo list and double an API call), `github_browse`'s `Feed` handler (Task 4) fetches `ListRepos` first if the caller didn't supply a repo list, then calls `fetch_feed` with the resolved list. This keeps `github_feed.rs` a pure "given repos, fetch+normalize+merge their events" function, easy to unit test with a fixed repo list.
- **Fan-out bound:** cap at the first 10 repos (by whatever order `ListRepos` returned — already `sort=updated`, so this is "10 most recently updated repos," a reasonable default matching what a user actually cares about). This bounds worst-case unauthenticated rate-limit burn to 10 requests (leaving 50 of the 60/hr budget for everything else) and keeps authenticated latency reasonable (10 sequential-or-small-batch requests, not 50+).
- **Sequencing, not full concurrency:** fetch repos' events with a small bounded concurrency (`futures::future::join_all` over chunks of 3, or a semaphore) rather than one giant `join_all` over all 10 at once — avoids bursting past secondary rate limits (GitHub's abuse-detection mechanism throttles bursts of concurrent requests even under the primary limit). If any individual repo's events call hits a 403/429, that repo's events are dropped (not the whole feed) and its error is recorded in `errors`, with `partial: true` set — matches "graceful degradation," not "one repo's rate limit kills everyone's feed."
- **Event → FeedItem mapping:** GitHub Events API returns entries shaped like `{ id, type, actor: { login }, repo: { name }, payload: {...}, created_at }`. Map `type` to the corrected taxonomy above:
  - `PushEvent` → `kind: "push"`, `meta: "{N} commits · {branch}"`; if any commit message matches `/^Bump \S+ from \S+ to \S+/i` or the actor login is `dependabot[bot]`/`dependabot-preview[bot]`, reclassify as `kind: "deps"` instead (mock label `"Dependencies"`), with `meta: "1 update · dependency"`.
  - `PullRequestEvent` with `payload.action == "closed"` and `payload.pull_request.merged == true` → `kind: "merge"`, `meta: "merged into {base branch}"`; otherwise (`opened`/`reopened`/etc.) → `kind: "pr"`, `meta: "{action} · {base} ← {head}"`. Both set `num` from `payload.pull_request.number`.
  - `PullRequestReviewEvent` → `kind: "review"`, `num` from `payload.pull_request.number`, `meta: "{review state} · {file count if available, else omitted}"`.
  - `IssuesEvent` → `kind: "issue"`, `num` from `payload.issue.number`, `meta: "{action} · {first label if any}"`.
  - `ReleaseEvent` → `kind: "release"`, `num: None`, `meta: "tagged {tag_name}"`.
  - Anything else (`WatchEvent`, `ForkEvent`, `CreateEvent`, `DeleteEvent`, `MemberEvent`, `IssueCommentEvent`, `PullRequestReviewCommentEvent`, `CommitCommentEvent`, etc.) → skip; not sourced by this plan (see the `comment`/`conflict` scoping note above — those two mock kinds have no `normalize_event` case here on purpose).
- **`path` extraction:** only `PushEvent` naturally names files (via `payload.commits[].message` or, better, nothing GitHub's events API actually exposes a file list for — Events API does NOT include per-commit file diffs). **Correction baked into this plan:** the Events API payload for `PushEvent` does not include changed file paths. To let a Feed item "link into" the split-pane view at a specific file (per the task's stated cross-feature link), this plan uses the **commit message's first `` `backtick-quoted` `` token if present**, else falls back to no `path` (the click just opens the repo's tree, unscoped, which is still correct behavior — it's a graceful degrade, not a broken link). Document this heuristic's limitation inline in the code and flag it in Open Questions — a fully correct "file this event touched" would require a second API call per push event (`GET /repos/{owner}/{repo}/commits/{sha}`), which multiplies the already-bounded fan-out and is out of scope for this plan's first cut.

- [ ] **Step 1: Write the failing test for event normalization**

Create `apps/palette-tauri/src-tauri/src/github_feed_tests.rs`:

```rust
use super::*;

fn sample_push_event() -> serde_json::Value {
    serde_json::json!({
        "id": "1",
        "type": "PushEvent",
        "actor": { "login": "jmagar" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T10:00:00Z",
        "payload": {
            "commits": [
                { "message": "fix: tighten SSRF validation in `src/core/http/ssrf.rs`" }
            ]
        }
    })
}

fn sample_dependabot_push_event() -> serde_json::Value {
    serde_json::json!({
        "id": "2",
        "type": "PushEvent",
        "actor": { "login": "dependabot[bot]" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T09:00:00Z",
        "payload": {
            "commits": [
                { "message": "Bump serde from 1.0.190 to 1.0.195" }
            ]
        }
    })
}

fn sample_merged_pr_event() -> serde_json::Value {
    serde_json::json!({
        "id": "3",
        "type": "PullRequestEvent",
        "actor": { "login": "jmagar" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T08:00:00Z",
        "payload": {
            "action": "closed",
            "pull_request": { "number": 42, "title": "Add feed view", "html_url": "https://github.com/jmagar/axon/pull/42", "merged": true, "base": { "ref": "main" } }
        }
    })
}

fn sample_opened_pr_event() -> serde_json::Value {
    serde_json::json!({
        "id": "4",
        "type": "PullRequestEvent",
        "actor": { "login": "jmagar" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T07:00:00Z",
        "payload": {
            "action": "opened",
            "pull_request": { "number": 43, "title": "WIP: feed", "html_url": "https://github.com/jmagar/axon/pull/43", "merged": false, "base": { "ref": "main" }, "head": { "ref": "feat/feed" } }
        }
    })
}

fn sample_unhandled_event() -> serde_json::Value {
    serde_json::json!({
        "id": "5",
        "type": "WatchEvent",
        "actor": { "login": "someone" },
        "repo": { "name": "jmagar/axon" },
        "created_at": "2024-01-15T06:00:00Z",
        "payload": {}
    })
}

#[test]
fn normalizes_plain_push_event_and_extracts_backtick_path() {
    let item = normalize_event(&sample_push_event()).expect("should normalize");
    assert_eq!(item.kind, "push");
    assert_eq!(item.repo, "jmagar/axon");
    assert_eq!(item.actor, "jmagar");
    assert_eq!(item.path.as_deref(), Some("src/core/http/ssrf.rs"));
    assert_eq!(item.num, None);
}

#[test]
fn reclassifies_dependabot_push_as_deps() {
    let item = normalize_event(&sample_dependabot_push_event()).expect("should normalize");
    // "deps" (mock label "Dependencies") — NOT "dependency-bump"; that kind
    // name does not exist in the real mock's FEED_KIND taxonomy.
    assert_eq!(item.kind, "deps");
}

#[test]
fn merged_pull_request_event_is_classified_as_merge() {
    let item = normalize_event(&sample_merged_pr_event()).expect("should normalize");
    assert_eq!(item.kind, "merge");
    assert_eq!(item.title, "Add feed view");
    assert_eq!(item.num, Some(42));
}

#[test]
fn opened_pull_request_event_is_classified_as_pr() {
    let item = normalize_event(&sample_opened_pr_event()).expect("should normalize");
    assert_eq!(item.kind, "pr");
    assert_eq!(item.num, Some(43));
}

#[test]
fn unhandled_event_types_are_skipped() {
    assert!(normalize_event(&sample_unhandled_event()).is_none());
}

#[test]
fn merge_feed_items_sorts_by_timestamp_descending() {
    let items = vec![
        FeedItem {
            kind: "push".into(), repo: "a".into(), actor: "x".into(),
            title: "older".into(), url: "".into(), path: None, num: None,
            meta: "".into(), badge: None, timestamp_unix: 100,
        },
        FeedItem {
            kind: "push".into(), repo: "a".into(), actor: "x".into(),
            title: "newer".into(), url: "".into(), path: None, num: None,
            meta: "".into(), badge: None, timestamp_unix: 200,
        },
    ];
    let sorted = sort_feed_items_desc(items);
    assert_eq!(sorted[0].title, "newer");
    assert_eq!(sorted[1].title, "older");
}

#[test]
fn caps_repo_fanout_at_ten() {
    let repos: Vec<String> = (0..25).map(|i| format!("repo-{i}")).collect();
    let capped = cap_repos_for_feed(&repos);
    assert_eq!(capped.len(), 10);
    assert_eq!(capped[0], "repo-0");
}
```

- [ ] **Step 2: Run to verify it fails (module doesn't exist yet)**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml github_feed`
Expected: FAIL — `error[E0433]: failed to resolve: use of undeclared crate or module 'github_feed'` (or similar; the module isn't registered in `lib.rs` yet and the file has no implementation)

- [ ] **Step 3: Implement `github_feed.rs`**

Create `apps/palette-tauri/src-tauri/src/github_feed.rs`:

```rust
//! Cross-repo activity Feed: fans `GET /repos/{owner}/{repo}/events` out across
//! a bounded set of repos, normalizes each event into a `FeedItem`, merges and
//! sorts by recency. Called from `github_bridge.rs`'s `Feed` branch, which
//! resolves the repo list (via `ListRepos`) before calling `fetch_feed` here.
//!
//! Data source: the GitHub Events API, not the Notifications API — see the
//! "Data source decision" section of `docs/plans/palette-github-enhancements.md`
//! for the full comparison. Short version: Events API scopes to "activity on
//! repos I'm already browsing" (matches how every other `github` browse kind
//! works), works unauthenticated, and needs no extra OAuth scope beyond the
//! `GITHUB_TOKEN` PAT the palette already reads.
//!
//! Known limitation: the Events API's `PushEvent` payload does not include
//! per-commit changed-file lists, so `FeedItem::path` is extracted heuristically
//! from the first backtick-quoted token in the lead commit message (a common
//! but not universal commit-message convention). When no backtick token is
//! found, `path` is `None` and clicking the feed item opens the repo's file
//! tree unscoped rather than jumping to a specific file — a graceful degrade,
//! not an error.

use std::collections::HashSet;

use serde::Serialize;

use crate::github_bridge::GITHUB_API_BASE;

/// Repos beyond this count (by the caller's ordering — `ListRepos` already
/// sorts by `updated`, so this means "10 most recently updated repos") are not
/// included in the fan-out. Bounds worst-case unauthenticated rate-limit burn
/// to 10 of the 60 req/hr budget, and keeps authenticated latency reasonable.
const MAX_FEED_REPOS: usize = 10;

/// Repos are fetched in chunks of this size (not all-at-once) to avoid
/// GitHub's secondary/abuse-detection rate limiting, which throttles bursts of
/// concurrent requests independent of the primary `x-ratelimit-*` budget.
const FEED_FANOUT_CHUNK_SIZE: usize = 3;

const MAX_ITEMS_PER_REPO: usize = 30;
const MAX_TOTAL_FEED_ITEMS: usize = 100;

/// One of `"pr"`, `"merge"`, `"review"`, `"comment"`, `"conflict"`, `"deps"`,
/// `"issue"`, `"push"`, `"release"` — the real mock's `FEED_KIND` taxonomy
/// (verified against `palette-mock.html`'s `var FEED_KIND = {...}` object;
/// mock labels are "Pull Request"/"Merged"/"Review"/"Comment"/"Conflict"/
/// "Dependencies"/"Issue"/"Push"/"Release" respectively). `normalize_event`
/// below never emits `"comment"` or `"conflict"` — this plan's Events API
/// source doesn't cover them (see this task's "Mock-verified taxonomy" note
/// in the plan doc); Task 5's TS-side label/icon maps still register both
/// kinds so the taxonomy stays forward-compatible.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedItem {
    pub(crate) kind: String,
    /// `owner/repo`.
    pub(crate) repo: String,
    /// GitHub login of the actor who triggered the event.
    pub(crate) actor: String,
    /// Human-readable title (PR/issue title, or the push's lead commit message).
    pub(crate) title: String,
    /// Best-effort link to view the event on github.com (PR/issue HTML URL, or
    /// the repo's commits page for pushes).
    pub(crate) url: String,
    /// Best-effort file path this event touched, when extractable — see the
    /// module doc's "Known limitation" for how/when this is populated.
    pub(crate) path: Option<String>,
    /// PR/issue number, when the event names one (`None` for pushes/releases,
    /// matching the mock's `num:null` on those rows).
    pub(crate) num: Option<u64>,
    /// Short freeform descriptive line (mock examples: `"opened · main ←
    /// feat/research"`, `"1 update · security advisory"`, `"tagged v5.19.0 ·
    /// 41 commits"`). Populated per event type in the `normalize_*` functions
    /// below — see this task's "open design question" note on how reliably
    /// the real Events API can back this field per kind.
    pub(crate) meta: String,
    /// Either a `{add, del}` line-diff or a short status label (mock
    /// examples: `"Approved"`, `"Closed"`, `"Bug"`, `"Latest"`, `"Patch"`).
    /// `None` when this event type has no reliable single-call source (see
    /// this task's "open design question" note).
    pub(crate) badge: Option<FeedBadge>,
    /// Unix seconds, parsed from the event's `created_at`.
    pub(crate) timestamp_unix: i64,
}

/// Mirrors the mock's `feedBadge()` renderer, which branches on whether `b`
/// is an object (`{add, del}`) or a string status label.
///
/// Both variants are struct-shaped (`Label { value: String }`, not a newtype
/// `Label(String)`) — serde's internally-tagged representation
/// (`#[serde(tag = "type")]`) cannot serialize a newtype variant wrapping a
/// primitive (it panics at runtime with "cannot serialize tagged newtype
/// variant ... containing a string"); every variant needs at least one named
/// field for the tag to be inlined alongside it.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub(crate) enum FeedBadge {
    Diff { add: u32, del: u32 },
    Label { value: String },
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FeedFetchResult {
    pub(crate) items: Vec<FeedItem>,
    pub(crate) rate_limit_remaining: Option<u32>,
    pub(crate) rate_limit_reset: Option<i64>,
    /// True when at least one repo's events call failed (e.g. individually
    /// rate-limited) and was dropped rather than failing the whole feed.
    pub(crate) partial: bool,
    /// Human-readable per-repo error messages, present only when `partial`.
    pub(crate) errors: Vec<String>,
}

/// Cap and return the leading `MAX_FEED_REPOS` entries of `repos`, preserving
/// order (callers pass already-sorted-by-recency repo names from `ListRepos`).
pub(crate) fn cap_repos_for_feed(repos: &[String]) -> Vec<String> {
    repos.iter().take(MAX_FEED_REPOS).cloned().collect()
}

/// Fetch and merge events across `repos` (already capped by the caller via
/// `cap_repos_for_feed` — this function does not re-cap, so tests can exercise
/// arbitrary repo counts directly).
pub(crate) async fn fetch_feed(
    client: &reqwest::Client,
    owner: &str,
    repos: &[String],
    token: Option<&str>,
) -> FeedFetchResult {
    let mut all_items = Vec::new();
    let mut errors = Vec::new();
    let mut last_remaining = None;
    let mut last_reset = None;

    for chunk in repos.chunks(FEED_FANOUT_CHUNK_SIZE) {
        let futures = chunk.iter().map(|repo| fetch_repo_events(client, owner, repo, token));
        let results = futures::future::join_all(futures).await;
        for (repo, result) in chunk.iter().zip(results) {
            match result {
                Ok((items, remaining, reset)) => {
                    all_items.extend(items);
                    last_remaining = remaining.or(last_remaining);
                    last_reset = reset.or(last_reset);
                }
                Err(message) => {
                    errors.push(format!("{owner}/{repo}: {message}"));
                }
            }
        }
    }

    let mut items = sort_feed_items_desc(all_items);
    items.truncate(MAX_TOTAL_FEED_ITEMS);

    FeedFetchResult {
        items,
        rate_limit_remaining: last_remaining,
        rate_limit_reset: last_reset,
        partial: !errors.is_empty(),
        errors,
    }
}

async fn fetch_repo_events(
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    token: Option<&str>,
) -> Result<(Vec<FeedItem>, Option<u32>, Option<i64>), String> {
    let url = format!("{GITHUB_API_BASE}/repos/{owner}/{repo}/events?per_page={MAX_ITEMS_PER_REPO}");
    let mut builder = client
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = token {
        builder = builder.bearer_auth(token);
    }

    let response = builder.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    let remaining = response
        .headers()
        .get("x-ratelimit-remaining")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u32>().ok());
    let reset = response
        .headers()
        .get("x-ratelimit-reset")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok());

    if !status.is_success() {
        return Err(format!("HTTP {status}"));
    }

    let body: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
    let events = body.as_array().cloned().unwrap_or_default();
    let items = events.iter().filter_map(normalize_event).collect();
    Ok((items, remaining, reset))
}

/// Convert one raw GitHub event JSON object into a `FeedItem`, or `None` if
/// its `type` isn't sourced by this plan (push/PR/merge/review/issue/release/
/// deps — NOT comment/conflict, see this module's doc comment and the plan's
/// "Mock-verified taxonomy" note in Task 3).
pub(crate) fn normalize_event(event: &serde_json::Value) -> Option<FeedItem> {
    let event_type = event.get("type")?.as_str()?;
    let actor = event
        .get("actor")
        .and_then(|a| a.get("login"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();
    let repo = event
        .get("repo")
        .and_then(|r| r.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let created_at = event.get("created_at").and_then(|v| v.as_str())?;
    let timestamp_unix = parse_iso8601_to_unix(created_at)?;
    let payload = event.get("payload").cloned().unwrap_or(serde_json::Value::Null);

    match event_type {
        "PushEvent" => normalize_push_event(&payload, &repo, &actor, timestamp_unix),
        "PullRequestEvent" => normalize_pull_request_event(&payload, &repo, &actor, timestamp_unix),
        "PullRequestReviewEvent" => normalize_review_event(&payload, &repo, &actor, timestamp_unix),
        "IssuesEvent" => normalize_issue_event(&payload, &repo, &actor, timestamp_unix),
        "ReleaseEvent" => normalize_release_event(&payload, &repo, &actor, timestamp_unix),
        _ => None,
    }
}

fn normalize_push_event(payload: &serde_json::Value, repo: &str, actor: &str, ts: i64) -> Option<FeedItem> {
    let commits = payload.get("commits").and_then(|c| c.as_array());
    let commit_count = commits.map(|list| list.len()).unwrap_or(0);
    let lead_message = commits
        .and_then(|list| list.last())
        .and_then(|c| c.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("(no commit message)")
        .to_string();
    let branch_ref = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .and_then(|r| r.strip_prefix("refs/heads/"))
        .unwrap_or("main");

    // "deps" (mock label "Dependencies") — NOT "dependency-bump", which does
    // not exist in the real mock's FEED_KIND taxonomy.
    let is_deps = actor.starts_with("dependabot")
        || lead_message
            .to_lowercase()
            .starts_with("bump ");

    let (kind, meta) = if is_deps {
        ("deps", "1 update · dependency".to_string())
    } else {
        ("push", format!("{commit_count} commits · {branch_ref}"))
    };

    Some(FeedItem {
        kind: kind.to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title: lead_message.clone(),
        url: format!("https://github.com/{repo}/commits"),
        path: extract_backtick_path(&lead_message),
        num: None,
        meta,
        // The Events API's PushEvent payload has no line-diff stats; a
        // {add, del} badge would need a separate per-commit API call. See
        // this task's "open design question" note — left None rather than
        // guessed.
        badge: None,
        timestamp_unix: ts,
    })
}

fn normalize_pull_request_event(payload: &serde_json::Value, repo: &str, actor: &str, ts: i64) -> Option<FeedItem> {
    let pr = payload.get("pull_request")?;
    let title = pr.get("title").and_then(|v| v.as_str()).unwrap_or("(untitled PR)").to_string();
    let url = pr.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let num = pr.get("number").and_then(|v| v.as_u64());
    let merged = pr.get("merged").and_then(|v| v.as_bool()).unwrap_or(false);
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let base_ref = pr.get("base").and_then(|b| b.get("ref")).and_then(|v| v.as_str()).unwrap_or("main");
    let head_ref = pr.get("head").and_then(|h| h.get("ref")).and_then(|v| v.as_str());

    let (kind, meta, badge) = if action == "closed" && merged {
        // The Events API's PullRequestEvent payload does not include
        // additions/deletions counts (those require a direct
        // GET /repos/{o}/{r}/pulls/{n} call) — badge is left None rather
        // than guessed. See this task's "open design question" note.
        ("merge", format!("merged into {base_ref}"), None)
    } else {
        let meta = match head_ref {
            Some(head) => format!("{action} · {base_ref} ← {head}"),
            None => format!("{action} · {base_ref}"),
        };
        ("pr", meta, None)
    };

    Some(FeedItem {
        kind: kind.to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num,
        meta,
        badge,
        timestamp_unix: ts,
    })
}

fn normalize_review_event(payload: &serde_json::Value, repo: &str, actor: &str, ts: i64) -> Option<FeedItem> {
    let pr = payload.get("pull_request")?;
    let title = pr.get("title").and_then(|v| v.as_str()).unwrap_or("(untitled PR)").to_string();
    let url = pr.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let num = pr.get("number").and_then(|v| v.as_u64());
    let review_state = payload
        .get("review")
        .and_then(|r| r.get("state"))
        .and_then(|v| v.as_str())
        .unwrap_or("submitted");
    Some(FeedItem {
        kind: "review".to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num,
        meta: review_state.to_string(),
        // No reliable single-call source for a review's file count/badge
        // from the Events API — see this task's "open design question" note.
        badge: None,
        timestamp_unix: ts,
    })
}

fn normalize_issue_event(payload: &serde_json::Value, repo: &str, actor: &str, ts: i64) -> Option<FeedItem> {
    let issue = payload.get("issue")?;
    let title = issue.get("title").and_then(|v| v.as_str()).unwrap_or("(untitled issue)").to_string();
    let url = issue.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let num = issue.get("number").and_then(|v| v.as_u64());
    let action = payload.get("action").and_then(|v| v.as_str()).unwrap_or("opened");
    let first_label = issue
        .get("labels")
        .and_then(|l| l.as_array())
        .and_then(|list| list.first())
        .and_then(|l| l.get("name"))
        .and_then(|v| v.as_str());
    let meta = match first_label {
        Some(label) => format!("{action} · {label}"),
        None => action.to_string(),
    };
    let badge = (action == "closed").then(|| FeedBadge::Label { value: "Closed".to_string() });

    Some(FeedItem {
        kind: "issue".to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num,
        meta,
        badge,
        timestamp_unix: ts,
    })
}

fn normalize_release_event(payload: &serde_json::Value, repo: &str, actor: &str, ts: i64) -> Option<FeedItem> {
    let release = payload.get("release")?;
    let title = release
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| release.get("tag_name").and_then(|v| v.as_str()))
        .unwrap_or("(untitled release)")
        .to_string();
    let url = release.get("html_url").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let tag_name = release.get("tag_name").and_then(|v| v.as_str()).unwrap_or("");
    // The Events API's ReleaseEvent payload has no "is this the latest
    // release" flag inline — that requires a separate
    // GET /repos/{o}/{r}/releases/latest call. Left None rather than guessed;
    // see this task's "open design question" note.
    Some(FeedItem {
        kind: "release".to_string(),
        repo: repo.to_string(),
        actor: actor.to_string(),
        title,
        url,
        path: None,
        num: None,
        meta: format!("tagged {tag_name}"),
        badge: None,
        timestamp_unix: ts,
    })
}

/// Extract the first backtick-quoted token from a commit message, e.g.
/// `` fix: tighten SSRF validation in `src/core/http/ssrf.rs` `` → `Some("src/core/http/ssrf.rs")`.
/// Returns `None` when no backtick-quoted span exists — see module doc's
/// "Known limitation."
fn extract_backtick_path(message: &str) -> Option<String> {
    let start = message.find('`')? + 1;
    let rest = &message[start..];
    let end = rest.find('`')?;
    let candidate = &rest[..end];
    // Cheap sanity filter: only treat it as a path if it looks path-shaped
    // (contains a '/' or a recognizable extension) — avoids false positives
    // like `` `cargo test` `` being mistaken for a file path.
    let looks_path_shaped = candidate.contains('/') || candidate.contains('.');
    looks_path_shaped.then(|| candidate.to_string())
}

/// Parse an RFC 3339 / ISO 8601 UTC timestamp (`2024-01-15T10:00:00Z`, the
/// exact shape GitHub's API sends) into Unix seconds, without pulling in a
/// chrono dependency — mirrors `github_bridge.rs`'s existing
/// dependency-free-time-math precedent (`civil_from_days`/`format_unix_time`).
fn parse_iso8601_to_unix(input: &str) -> Option<i64> {
    let bytes = input.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let year: i64 = input.get(0..4)?.parse().ok()?;
    let month: i64 = input.get(5..7)?.parse().ok()?;
    let day: i64 = input.get(8..10)?.parse().ok()?;
    let hour: i64 = input.get(11..13)?.parse().ok()?;
    let minute: i64 = input.get(14..16)?.parse().ok()?;
    let second: i64 = input.get(17..19)?.parse().ok()?;

    let days = days_from_civil(year, month, day);
    Some(days * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Inverse of `github_bridge.rs::civil_from_days` — proleptic Gregorian
/// (year, month, day) to days-since-epoch. Same Howard Hinnant algorithm
/// family, kept local to avoid a cross-module `pub(crate)` for a two-line
/// helper only this file needs.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn sort_feed_items_desc(mut items: Vec<FeedItem>) -> Vec<FeedItem> {
    items.sort_by(|a, b| b.timestamp_unix.cmp(&a.timestamp_unix));
    items
}

/// Distinct repo names referenced by a set of feed items (used by the bridge
/// layer for logging/diagnostics; not required by the happy path).
#[allow(dead_code)]
pub(crate) fn distinct_repos(items: &[FeedItem]) -> HashSet<String> {
    items.iter().map(|item| item.repo.clone()).collect()
}

#[cfg(test)]
#[path = "github_feed_tests.rs"]
mod tests;
```

- [ ] **Step 4: Expose `GITHUB_API_BASE` from `github_bridge.rs` for reuse**

In `github_bridge.rs`, change:

```rust
const GITHUB_API_BASE: &str = "https://api.github.com";
```

to:

```rust
pub(crate) const GITHUB_API_BASE: &str = "https://api.github.com";
```

- [ ] **Step 5: Register the module in `lib.rs`**

In `apps/palette-tauri/src-tauri/src/lib.rs`, find:

```rust
mod github_bridge;
```

and add immediately after it:

```rust
mod github_feed;
```

- [ ] **Step 6: Add the `futures` dependency if not already present**

Run: `grep -n '^futures' apps/palette-tauri/src-tauri/Cargo.toml`

If no output, add it:

```bash
cd apps/palette-tauri/src-tauri && cargo add futures
```

- [ ] **Step 7: Run the tests to confirm they pass**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml github_feed`
Expected: PASS, all 7 tests from Step 1 green

- [ ] **Step 8: Run the monolith check**

Run: `wc -l apps/palette-tauri/src-tauri/src/github_feed.rs apps/palette-tauri/src-tauri/src/github_bridge.rs`
Expected: both under 500 lines (if `github_feed.rs` is close to 500, this is fine — it's a new file so there's no "changed file" ambiguity; if it exceeds 500, split the per-event `normalize_*` functions into a `github_feed/normalize.rs` submodule per the sidecar convention — but do NOT create `github_feed/mod.rs`, use `#[path]` the same way test sidecars do, or simply keep `normalize_*` inline since each function is well under the 120-line function cap)

- [ ] **Step 9: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/github_feed.rs apps/palette-tauri/src-tauri/src/github_feed_tests.rs apps/palette-tauri/src-tauri/src/github_bridge.rs apps/palette-tauri/src-tauri/src/lib.rs apps/palette-tauri/src-tauri/Cargo.toml apps/palette-tauri/src-tauri/Cargo.lock
git commit -m "feat(palette-bridge): fan out GitHub Events API across repos for the Feed"
```

---

## Task 4: Wire `Feed` into `github_browse` end-to-end

**Files:**
- Modify: `apps/palette-tauri/src-tauri/src/github_bridge.rs` (the `GitHubBrowseResult` struct + `github_browse` command)
- Modify: `apps/palette-tauri/src-tauri/src/github_bridge_tests.rs`

**Interfaces:**
- Consumes: `github_feed::{fetch_feed, cap_repos_for_feed, FeedFetchResult}` (Task 3), existing `ListRepos` URL building.
- Produces: `GitHubBrowseResult.payload` for `kind: "feed"` responses is a JSON object `{ items: FeedItem[], partial: bool, errors: string[] }` (the `FeedFetchResult` struct, minus its rate-limit fields which are already carried by `GitHubBrowseResult`'s existing top-level `rate_limit_remaining`/`rate_limit_reset`).

- [ ] **Step 1: Write the failing integration-shaped test**

This one needs a live-ish HTTP mock. Check whether the existing test file already has an HTTP mocking pattern:

Run: `grep -n "mockito\|wiremock\|httpmock" apps/palette-tauri/src-tauri/Cargo.toml apps/palette-tauri/src-tauri/src/github_bridge_tests.rs`

If none of these are present, the existing bridge tests are pure unit tests of URL-building/validation only (no live HTTP call is exercised in tests today — confirm this by re-reading `github_bridge_tests.rs` in full). **In that case, do not add a new test dependency for this task** — instead, add a focused unit test of the *dispatch logic only* by extracting a small pure function that Task 4 needs anyway: `fn build_feed_payload(fetch_result: FeedFetchResult) -> serde_json::Value`, tested without any network call. The actual `fetch_feed` → HTTP behavior stays covered by Task 3's tests (which test `normalize_event`/`fetch_repo_events`'s pure helpers, not live network — `fetch_repo_events` itself is exercised manually in Task 9's manual verification pass, consistent with how the rest of `github_bridge.rs`'s live-network path has no automated test today either).

Add to `github_bridge_tests.rs`:

```rust
#[test]
fn feed_payload_serializes_items_and_partial_flag() {
    use crate::github_feed::FeedItem;

    let items = vec![FeedItem {
        kind: "push".to_string(),
        repo: "jmagar/axon".to_string(),
        actor: "jmagar".to_string(),
        title: "fix: bug".to_string(),
        url: "https://github.com/jmagar/axon/commits".to_string(),
        path: Some("src/main.rs".to_string()),
        timestamp_unix: 1_700_000_000,
    }];
    let fetch_result = crate::github_feed::FeedFetchResult {
        items,
        rate_limit_remaining: Some(55),
        rate_limit_reset: Some(1_700_003_600),
        partial: false,
        errors: vec![],
    };
    let payload = build_feed_payload(fetch_result);
    assert_eq!(payload["items"][0]["repo"], "jmagar/axon");
    assert_eq!(payload["partial"], false);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml feed_payload_serializes`
Expected: FAIL — `build_feed_payload` doesn't exist yet

- [ ] **Step 3: Implement `build_feed_payload` and wire the `Feed` branch into `github_browse`**

In `github_bridge.rs`, add this function near `truncate_file_payload`:

```rust
fn build_feed_payload(fetch_result: crate::github_feed::FeedFetchResult) -> serde_json::Value {
    serde_json::json!({
        "items": fetch_result.items,
        "partial": fetch_result.partial,
        "errors": fetch_result.errors,
    })
}
```

Now modify the `github_browse` command function. The existing function has one shared code path for all four (now five) kinds — `Feed` needs a *different* control flow (fetch repo list first, then fan out) rather than the single-URL `build_request_url` → single GET path the other four kinds use. Restructure the top of `github_browse`:

```rust
#[tauri::command]
pub(crate) async fn github_browse(
    client: tauri::State<'_, GitHubClient>,
    request: GitHubBrowseRequest,
) -> Result<GitHubBrowseResult, String> {
    let kind = parse_kind(&request.kind)?;
    let token = github_token();
    let authenticated = token.is_some();
    let owner = request.owner.clone();
    let repo = request.repo.clone();
    let branch = request.branch.clone();
    let path = request.path.clone();

    if kind == GitHubRequestKind::Feed {
        return github_browse_feed(&client, &request, token.as_deref(), authenticated).await;
    }

    let url = build_request_url(&request, kind)?;
    let mut builder = (*client)
        .client()
        .get(&url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28");
    if let Some(token) = token.as_deref() {
        builder = builder.bearer_auth(token);
    }

    let response = builder.send().await.map_err(|err| err.to_string())?;
    let status = response.status();
    let rate_limit_remaining = header_u32(&response, "x-ratelimit-remaining");
    let rate_limit_reset = header_i64(&response, "x-ratelimit-reset");

    let text = response.text().await.map_err(|err| err.to_string())?;
    let payload: serde_json::Value = if text.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text))
    };

    if status.is_success() {
        let payload = if kind == GitHubRequestKind::FileContents {
            truncate_file_payload(payload)
        } else {
            payload
        };
        return Ok(GitHubBrowseResult {
            ok: true,
            status: status.as_u16(),
            kind: request.kind,
            owner,
            repo,
            branch,
            path,
            payload,
            error: None,
            rate_limit_remaining,
            rate_limit_reset,
            authenticated,
        });
    }

    let error = describe_error(status, rate_limit_remaining, rate_limit_reset, &payload);
    Ok(GitHubBrowseResult {
        ok: false,
        status: status.as_u16(),
        kind: request.kind,
        owner,
        repo,
        branch,
        path,
        payload: serde_json::Value::Null,
        error: Some(error),
        rate_limit_remaining,
        rate_limit_reset,
        authenticated,
    })
}

/// `Feed` branch of `github_browse`: resolves the repo list for `request.owner`
/// (reusing the `ListRepos` URL/shape) unless the caller already supplied one
/// via `request.repo` as a comma-separated list (not currently exercised by the
/// frontend — see Task 7 — but supported here so a future caller can skip the
/// extra `ListRepos` round trip when it already knows the repos), then fans
/// events out across up to `MAX_FEED_REPOS` of them via `github_feed::fetch_feed`.
async fn github_browse_feed(
    client: &GitHubClient,
    request: &GitHubBrowseRequest,
    token: Option<&str>,
    authenticated: bool,
) -> Result<GitHubBrowseResult, String> {
    let owner = validate_segment(&request.owner, "owner")?.to_string();

    let repos: Vec<String> = if let Some(explicit) = request.repo.as_deref().filter(|r| !r.trim().is_empty()) {
        explicit.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
    } else {
        let list_url = format!("{GITHUB_API_BASE}/users/{owner}/repos?sort=updated&per_page=50");
        let mut builder = client
            .client()
            .get(&list_url)
            .header(reqwest::header::ACCEPT, "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28");
        if let Some(token) = token {
            builder = builder.bearer_auth(token);
        }
        let response = builder.send().await.map_err(|err| err.to_string())?;
        let status = response.status();
        if !status.is_success() {
            let rate_limit_remaining = header_u32(&response, "x-ratelimit-remaining");
            let rate_limit_reset = header_i64(&response, "x-ratelimit-reset");
            let text = response.text().await.unwrap_or_default();
            let payload: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
            let error = describe_error(status, rate_limit_remaining, rate_limit_reset, &payload);
            return Ok(GitHubBrowseResult {
                ok: false,
                status: status.as_u16(),
                kind: "feed".to_string(),
                owner,
                repo: None,
                branch: None,
                path: None,
                payload: serde_json::Value::Null,
                error: Some(error),
                rate_limit_remaining,
                rate_limit_reset,
                authenticated,
            });
        }
        let repos_json: serde_json::Value = response.json().await.map_err(|err| err.to_string())?;
        repos_json
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| r.get("name").and_then(|n| n.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };

    let capped = crate::github_feed::cap_repos_for_feed(&repos);
    let fetch_result = crate::github_feed::fetch_feed(client.client(), &owner, &capped, token).await;
    let rate_limit_remaining = fetch_result.rate_limit_remaining;
    let rate_limit_reset = fetch_result.rate_limit_reset;
    let payload = build_feed_payload(fetch_result);

    Ok(GitHubBrowseResult {
        ok: true,
        status: 200,
        kind: "feed".to_string(),
        owner,
        repo: None,
        branch: None,
        path: None,
        payload,
        error: None,
        rate_limit_remaining,
        rate_limit_reset,
        authenticated,
    })
}
```

- [ ] **Step 4: Run all bridge tests**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml github_bridge`
Expected: PASS, including the new `feed_payload_serializes_items_and_partial_flag` test

- [ ] **Step 5: Full workspace Rust check**

Run: `cargo check --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`
Expected: PASS, no warnings about unused imports (the `Feed` match arm removed the need for `build_request_url` to be called for that kind inside `github_browse`'s main path, but it's still called from `github_feed_tests.rs`'s Task-2 tests and possibly nowhere else at runtime — verify with `cargo clippy` in Task 9, not here)

- [ ] **Step 6: Commit**

```bash
git add apps/palette-tauri/src-tauri/src/github_bridge.rs apps/palette-tauri/src-tauri/src/github_bridge_tests.rs
git commit -m "feat(palette-bridge): wire Feed kind into github_browse end-to-end"
```

---

## Task 5: TypeScript `lib/githubFeed.ts` — types, day-grouping, dev fallback

**Files:**
- Create: `apps/palette-tauri/src/lib/githubFeed.ts`
- Create: `apps/palette-tauri/src/lib/githubFeed.test.ts`
- Modify: `apps/palette-tauri/src/lib/invoke.ts` (extend `githubBrowseDevFallback`)

**Interfaces:**
- Consumes: nothing new (pure functions + fetch, mirroring `github_bridge.rs`'s Rust shapes field-for-field).
- Produces: `export interface FeedItem { kind: string; repo: string; actor: string; title: string; url: string; path: string | null; num: number | null; meta: string; badge: FeedBadge | null; timestampUnix: number }` (camelCase — matches the Rust struct's `#[serde(rename_all = "camelCase")]`; `num`/`meta`/`badge` are new fields added to match the real mock's `FEED` item shape, see Task 3), `export type FeedBadge = { type: "diff"; add: number; del: number } | { type: "label"; value: string }` (mirrors Task 3's Rust `FeedBadge` enum, tagged the same way serde emits it with `#[serde(tag = "type")]`), `export interface FeedPayload { items: FeedItem[]; partial: boolean; errors: string[] }`, `export interface FeedDayGroup { label: "Today" | "Yesterday" | "Earlier"; items: FeedItem[] }`, `export function groupFeedByDay(items: FeedItem[], nowMs?: number): FeedDayGroup[]`, `export function feedKindLabel(kind: string): string`, `export function feedKindIcon(kind: string): LucideIcon` (consumed by Task 6's `GitHubFeedView.tsx`).

- [ ] **Step 1: Write the failing test**

Create `apps/palette-tauri/src/lib/githubFeed.test.ts`:

```ts
import { describe, expect, it } from "vitest";

import { feedKindLabel, groupFeedByDay, type FeedItem } from "./githubFeed";

function item(overrides: Partial<FeedItem>): FeedItem {
  return {
    kind: "push",
    repo: "jmagar/axon",
    actor: "jmagar",
    title: "fix: bug",
    url: "https://github.com/jmagar/axon/commits",
    path: null,
    num: null,
    meta: "3 commits · main",
    badge: null,
    timestampUnix: 0,
    ...overrides,
  };
}

describe("groupFeedByDay", () => {
  it("groups items into Today/Yesterday/Earlier relative to now", () => {
    const now = new Date("2024-06-15T12:00:00Z").getTime();
    const todayItem = item({ title: "today", timestampUnix: Math.floor(new Date("2024-06-15T08:00:00Z").getTime() / 1000) });
    const yesterdayItem = item({ title: "yesterday", timestampUnix: Math.floor(new Date("2024-06-14T08:00:00Z").getTime() / 1000) });
    const earlierItem = item({ title: "earlier", timestampUnix: Math.floor(new Date("2024-06-01T08:00:00Z").getTime() / 1000) });

    const groups = groupFeedByDay([earlierItem, todayItem, yesterdayItem], now);

    expect(groups.map((g) => g.label)).toEqual(["Today", "Yesterday", "Earlier"]);
    expect(groups[0].items).toEqual([todayItem]);
    expect(groups[1].items).toEqual([yesterdayItem]);
    expect(groups[2].items).toEqual([earlierItem]);
  });

  it("omits empty groups", () => {
    const now = new Date("2024-06-15T12:00:00Z").getTime();
    const onlyToday = item({ timestampUnix: Math.floor(new Date("2024-06-15T08:00:00Z").getTime() / 1000) });
    const groups = groupFeedByDay([onlyToday], now);
    expect(groups.map((g) => g.label)).toEqual(["Today"]);
  });

  it("returns an empty array for no items", () => {
    expect(groupFeedByDay([], Date.now())).toEqual([]);
  });
});

describe("feedKindLabel", () => {
  // Labels verified against palette-mock.html's real `var FEED_KIND = {...}`
  // object — NOT the first drafting pass's reconstruction (which used
  // "Pull request" lowercase-r and invented a "dependency-bump" kind that
  // does not exist in the mock).
  it.each([
    ["push", "Push"],
    ["pr", "Pull Request"],
    ["merge", "Merged"],
    ["review", "Review"],
    ["comment", "Comment"],
    ["conflict", "Conflict"],
    ["deps", "Dependencies"],
    ["issue", "Issue"],
    ["release", "Release"],
  ])("labels %s as %s", (kind, expected) => {
    expect(feedKindLabel(kind)).toBe(expected);
  });

  it("falls back to the raw kind for unknown values", () => {
    expect(feedKindLabel("mystery")).toBe("mystery");
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/githubFeed.test.ts`
Expected: FAIL — module `./githubFeed` doesn't exist

- [ ] **Step 3: Implement `githubFeed.ts`**

Create `apps/palette-tauri/src/lib/githubFeed.ts`:

```ts
// Cross-repo activity Feed — types and pure presentation helpers.
//
// `FeedItem` mirrors `src-tauri/src/github_feed.rs::FeedItem` field-for-field
// (camelCase because that Rust struct is `#[serde(rename_all = "camelCase")]`,
// same convention as `GitHubBrowseResult` in actionRequest.ts). Grouping by
// day happens here, not in Rust — it's a presentation concern (depends on the
// viewer's "now," which the bridge has no reason to know about), matching how
// the reference mock also grouped its (fixture) feed data client-side.
//
// Data source: GitHub's Events API, fanned out per-repo by the Rust bridge —
// see docs/plans/palette-github-enhancements.md's "Data source decision"
// section for why this was chosen over the Notifications API.
//
// Kind taxonomy verified against palette-mock.html's real `var FEED_KIND =
// {...}` object: pr/merge/review/comment/conflict/deps/issue/push/release.
// `comment` and `conflict` are registered here for forward-compatibility but
// `github_feed.rs::normalize_event` never emits them in this pass (no clean
// Events API source — see Task 3's "Mock-verified taxonomy" note).

import {
  AlertTriangle,
  GitBranch,
  GitCommit,
  GitMerge,
  GitPullRequest,
  MessageSquare,
  Package,
  Tag,
  type LucideIcon,
} from "lucide-react";

export type FeedBadge = { type: "diff"; add: number; del: number } | { type: "label"; value: string };

export interface FeedItem {
  /** One of: "pr" | "merge" | "review" | "comment" | "conflict" | "deps" | "issue" | "push" | "release". */
  kind: string;
  /** `owner/repo`. */
  repo: string;
  actor: string;
  title: string;
  url: string;
  path: string | null;
  /** PR/issue number, or `null` for kinds that don't carry one (push/release). */
  num: number | null;
  /** Short freeform descriptive line — see `github_feed.rs::FeedItem::meta` doc comment for examples per kind. */
  meta: string;
  /** Line-diff or status-label badge, or `null` when this event type has no reliable single-call source. */
  badge: FeedBadge | null;
  timestampUnix: number;
}

export interface FeedPayload {
  items: FeedItem[];
  partial: boolean;
  errors: string[];
}

export interface FeedDayGroup {
  label: "Today" | "Yesterday" | "Earlier";
  items: FeedItem[];
}

/**
 * Group feed items into Today/Yesterday/Earlier buckets relative to `nowMs`,
 * matching the mock's day-grouping. Empty buckets are omitted. Within each
 * bucket, item order is preserved from the input (callers should pass
 * already-sorted-descending items — the Rust bridge already sorts by
 * `timestamp_unix` descending before returning).
 */
export function groupFeedByDay(items: FeedItem[], nowMs: number = Date.now()): FeedDayGroup[] {
  const now = new Date(nowMs);
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const startOfYesterday = startOfToday - 24 * 60 * 60 * 1000;

  const today: FeedItem[] = [];
  const yesterday: FeedItem[] = [];
  const earlier: FeedItem[] = [];

  for (const item of items) {
    const ms = item.timestampUnix * 1000;
    if (ms >= startOfToday) {
      today.push(item);
    } else if (ms >= startOfYesterday) {
      yesterday.push(item);
    } else {
      earlier.push(item);
    }
  }

  const groups: FeedDayGroup[] = [];
  if (today.length > 0) groups.push({ label: "Today", items: today });
  if (yesterday.length > 0) groups.push({ label: "Yesterday", items: yesterday });
  if (earlier.length > 0) groups.push({ label: "Earlier", items: earlier });
  return groups;
}

// Labels verified against palette-mock.html's real FEED_KIND object — note
// "Pull Request" is capital-R (the mock, not "Pull request" from the first
// drafting pass) and there is no "dependency-bump" kind; the mock's actual
// dependency kind is "deps" labeled "Dependencies".
const FEED_KIND_LABELS: Record<string, string> = {
  pr: "Pull Request",
  merge: "Merged",
  review: "Review",
  comment: "Comment",
  conflict: "Conflict",
  deps: "Dependencies",
  issue: "Issue",
  push: "Push",
  release: "Release",
};

export function feedKindLabel(kind: string): string {
  return FEED_KIND_LABELS[kind] ?? kind;
}

// Icon substitutes for the mock's inline SVG glyphs (the mock ships a
// hand-drawn path per kind via `Svg(k.g, 15)`; this plan uses the closest
// lucide-react icon per kind rather than porting raw SVG paths).
const FEED_KIND_ICONS: Record<string, LucideIcon> = {
  pr: GitPullRequest,
  merge: GitMerge,
  review: GitBranch,
  comment: MessageSquare,
  conflict: AlertTriangle,
  deps: Package,
  issue: GitBranch,
  push: GitCommit,
  release: Tag,
};

export function feedKindIcon(kind: string): LucideIcon {
  return FEED_KIND_ICONS[kind] ?? GitCommit;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/githubFeed.test.ts`
Expected: PASS, all cases green

- [ ] **Step 5: Extend the browser-dev fallback in `invoke.ts`**

In `apps/palette-tauri/src/lib/invoke.ts`, the `githubBrowseDevFallback` function needs a `"feed"` branch. This is dev-only (no Tauri, no Rust bridge, no `GITHUB_TOKEN`), so it fans out client-side against the browser's own `fetch`, capped small since it's always unauthenticated (60 req/hr shared across the *whole* app in dev mode, not just this call):

Add a new constant near the top of the file, after the existing constants:

```ts
/** Dev-fallback-only cap — much smaller than the Rust bridge's MAX_FEED_REPOS
 * (10), because this path is always unauthenticated (60 req/hr total) and
 * shared with every other GitHub call the browser-dev session makes. */
const DEV_FEED_MAX_REPOS = 3;
```

**Note on the edit below:** `githubBrowseDevFallback` dispatches on `kind` with a local `if`/`else if` chain — this is distinct from `invoke.ts`'s top-level `invoke()` command dispatcher elsewhere in the file, which is a `switch (command)` statement. The edit below inserts a new branch into the `if`/`else if` chain *inside* `githubBrowseDevFallback` only; it does not touch the outer `switch`.

Modify the `githubBrowseDevFallback` function's `kind` branching. Change:

```ts
  } else if (kind === "file") {
```

Add a new `else if` branch immediately before it:

```ts
  } else if (kind === "feed") {
    return githubFeedDevFallback(owner);
```

And add the new helper function at the bottom of the file, after `numericHeader`:

```ts
/** Dev-only Feed fallback: fetches the owner's repo list, then fans out
 * `GET /repos/{owner}/{repo}/events` across the first `DEV_FEED_MAX_REPOS`
 * repos directly from the browser. Always unauthenticated — mirrors
 * `github_feed.rs::fetch_feed`'s normalization but is deliberately not a
 * byte-for-byte port (dev-only, small-N, no rate-limit retry/backoff). */
async function githubFeedDevFallback(owner: string): Promise<GitHubBrowseDevResult> {
  const reposResp = await fetch(
    `https://api.github.com/users/${encodeURIComponent(owner)}/repos?sort=updated&per_page=50`,
    { headers: { accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28" } },
  );
  if (!reposResp.ok) {
    return {
      ok: false,
      status: reposResp.status,
      kind: "feed",
      owner,
      repo: null,
      branch: null,
      path: null,
      payload: null,
      error: `GitHub API error: ${reposResp.status}`,
      rateLimitRemaining: numericHeader(reposResp, "x-ratelimit-remaining"),
      rateLimitReset: numericHeader(reposResp, "x-ratelimit-reset"),
      authenticated: false,
    };
  }
  const repos: Array<{ name: string }> = await reposResp.json();
  const capped = repos.slice(0, DEV_FEED_MAX_REPOS);

  const events = await Promise.all(
    capped.map(async (repo) => {
      const resp = await fetch(
        `https://api.github.com/repos/${encodeURIComponent(owner)}/${encodeURIComponent(repo.name)}/events?per_page=30`,
        { headers: { accept: "application/vnd.github+json", "X-GitHub-Api-Version": "2022-11-28" } },
      );
      if (!resp.ok) return [];
      const raw: unknown[] = await resp.json();
      return raw;
    }),
  );

  const items = events
    .flat()
    .map((raw) => normalizeDevFeedEvent(raw))
    .filter((item): item is Record<string, unknown> => item !== null)
    .sort((a, b) => (b.timestampUnix as number) - (a.timestampUnix as number));

  return {
    ok: true,
    status: 200,
    kind: "feed",
    owner,
    repo: null,
    branch: null,
    path: null,
    payload: { items, partial: false, errors: [] },
    error: null,
    rateLimitRemaining: null,
    rateLimitReset: null,
    authenticated: false,
  };
}

/** Minimal dev-fallback event normalizer — intentionally simpler than the Rust
 * bridge's `github_feed.rs::normalize_event` (no dependabot/"Bump " → "deps"
 * reclassification, no backtick path extraction, no per-kind `meta`/`badge`
 * derivation beyond a placeholder `meta`). Dev iteration only cares about
 * "the Feed tab renders something plausible without the desktop shell." */
function normalizeDevFeedEvent(raw: unknown): Record<string, unknown> | null {
  if (typeof raw !== "object" || raw === null) return null;
  const event = raw as Record<string, unknown>;
  const type = event.type;
  const repoName = (event.repo as Record<string, unknown> | undefined)?.name;
  const actorLogin = (event.actor as Record<string, unknown> | undefined)?.login;
  const createdAt = event.created_at;
  if (typeof type !== "string" || typeof repoName !== "string" || typeof createdAt !== "string") return null;

  // Kind names match the real mock's FEED_KIND taxonomy (pr/merge/review/
  // comment/conflict/deps/issue/push/release) — "deps" not "dependency-bump".
  // This dev fallback does not attempt the dependabot/"Bump " reclassification
  // Task 3's Rust normalizer does; it always reports plain "push".
  const kindMap: Record<string, string> = {
    PushEvent: "push",
    PullRequestEvent: "pr",
    PullRequestReviewEvent: "review",
    IssuesEvent: "issue",
    ReleaseEvent: "release",
  };
  const kind = kindMap[type];
  if (!kind) return null;

  return {
    kind,
    repo: repoName,
    actor: typeof actorLogin === "string" ? actorLogin : "unknown",
    title: `${type} on ${repoName}`,
    url: `https://github.com/${repoName}`,
    path: null,
    num: null,
    meta: type,
    badge: null,
    timestampUnix: Math.floor(new Date(createdAt).getTime() / 1000),
  };
}
```

- [ ] **Step 6: Typecheck the whole frontend**

Run: `cd apps/palette-tauri && pnpm typecheck`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src/lib/githubFeed.ts apps/palette-tauri/src/lib/githubFeed.test.ts apps/palette-tauri/src/lib/invoke.ts
git commit -m "feat(palette): add Feed types, day-grouping, and dev fallback"
```

---

## Task 6: Refactor `GitHubView.tsx` — replace the history stack with tree+preview split state

**Files:**
- Modify: `apps/palette-tauri/src/components/palette/GitHubView.tsx`
- Modify: `apps/palette-tauri/src/components/palette/GitHubView.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css`

**Interfaces:**
- Consumes: `FilesView.tsx`'s pattern (not its code — no cross-import; `FilesView` is local-fs-specific, `GitHubView` is GitHub-specific, they stay separate components, only the *shape* of state/layout is copied).
- Produces: `GitHubView`'s new internal state shape (for reference by Task 7, which adds the Feed tab into the same component): `{ owner: string; repo: string | null; branch: string | null; selectedPath: string | null; tree: LoadState<GitHubBrowseResult>; file: LoadState<GitHubBrowseResult> }` where `LoadState<T>` is the same 4-variant union already used in `FilesView.tsx` (`{kind:"idle"} | {kind:"loading"} | {kind:"loaded", value:T} | {kind:"error", message:string}`).

This task does the split-pane conversion **without** the Feed tab yet — keep the diff reviewable. The "Back" button changes meaning from "undo last click" to "return to the repo list" (there is no other place left to go back to, once tree+preview are simultaneous).

- [ ] **Step 1: Update the test file's expectations for the new Back semantics FIRST (red)**

In `apps/palette-tauri/src/components/palette/GitHubView.test.tsx`, replace the last test:

```ts
  it("shows a Back button after drilling and returns to the previous view", async () => {
    invokeMock.mockResolvedValueOnce(treeResult);
    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByText("jmagar/axon"));
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const backButton = screen.getByRole("button", { name: /back/i });
    fireEvent.click(backButton);
    await waitFor(() => expect(screen.getByText("jmagar/axon")).toBeInTheDocument());
  });
```

with:

```ts
  it("shows a Back-to-repos button once inside a repo, and returns to the repo list", async () => {
    invokeMock.mockResolvedValueOnce(treeResult);
    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByText("jmagar/axon"));
    await waitFor(() => expect(screen.getByText("README.md")).toBeInTheDocument());
    const backButton = screen.getByRole("button", { name: /back/i });
    fireEvent.click(backButton);
    await waitFor(() => expect(screen.getByText("jmagar/axon")).toBeInTheDocument());
    // Back from the repo-list view (the top level) does not show a Back button.
    expect(screen.queryByRole("button", { name: /back/i })).not.toBeInTheDocument();
  });

  it("renders the tree and a file's preview simultaneously in a split view", async () => {
    invokeMock.mockResolvedValueOnce(fileResult);
    render(<GitHubView payload={treeResult} />);
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(screen.getByText(/hello/)).toBeInTheDocument());
    // The tree is still visible alongside the preview — this is the split-pane
    // behavior replacing the old sequential "tree screen -> separate preview
    // screen" navigation.
    expect(screen.getByText("README.md")).toBeInTheDocument();
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
    expect(screen.getByText(/hello/)).toBeInTheDocument();
  });

  it("selecting a different file only swaps the preview pane, tree stays put", async () => {
    invokeMock.mockResolvedValueOnce(fileResult);
    render(<GitHubView payload={treeResult} />);
    fireEvent.click(screen.getByText("README.md"));
    await waitFor(() => expect(screen.getByText(/hello/)).toBeInTheDocument());

    const secondFileResult = {
      ...fileResult,
      path: "src/main.rs",
      payload: { path: "src/main.rs", content: btoa("fn main() {}"), encoding: "base64", size: 12 },
    };
    invokeMock.mockResolvedValueOnce(secondFileResult);
    fireEvent.click(screen.getByText("src/main.rs"));
    await waitFor(() => expect(screen.getByText(/fn main/)).toBeInTheDocument());
    // Tree entries for BOTH files are still visible — no navigation occurred.
    expect(screen.getByText("README.md")).toBeInTheDocument();
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run to verify the new tests fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/GitHubView.test.tsx`
Expected: FAIL — the current sequential-screen implementation doesn't render tree+preview simultaneously

- [ ] **Step 3: Rewrite `GitHubView.tsx`'s state and render logic**

Replace the full contents of `apps/palette-tauri/src/components/palette/GitHubView.tsx` with:

```tsx
import { memo, useCallback, useEffect, useMemo, useState } from "react";
import {
  AlertTriangle,
  ChevronLeft,
  Copy,
  ExternalLink,
  File as FileIcon,
  FileArchive,
  FileCode,
  FileCog,
  FileText,
  FolderGit2,
  Loader2,
  MessageSquare,
} from "lucide-react";

import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { EmptyResult, ResultHero } from "@/components/palette/OperationResultViewShared";
import type { GitHubBrowseResult } from "@/lib/actionRequest";
import { fileKind, isMarkdownLike } from "@/lib/filesModel";
import { invoke } from "@/lib/invoke";
import { isRecord } from "@/lib/payload";

// Note: this rewrite drops the plain Path/Size `DetailLine` detail card the
// first drafting pass used for the file preview header, replacing it with
// the mock-matched `pvHead`/`pvFoot` chrome in `FilePreview` below (path is
// now shown inline in the header, size moves to the footer strip) — so
// `DetailLine` is no longer imported here. It is still used elsewhere in the
// codebase (`OperationResultViewShared.tsx` consumers); this file's import
// list simply no longer needs it.

export type { GitHubBrowseResult } from "@/lib/actionRequest";

interface GitHubBrowseRequest {
  kind: "repos" | "repo" | "tree" | "file" | "feed";
  owner: string;
  repo?: string;
  branch?: string;
  path?: string;
}

interface RepoSummary {
  name: string;
  full_name: string;
  description: string | null;
  language: string | null;
  stargazers_count: number;
  forks_count: number;
  private: boolean;
  default_branch: string;
}

interface TreeEntry {
  path: string;
  type: "blob" | "tree" | string;
  size?: number;
}

interface FileContents {
  path: string;
  content?: string;
  encoding?: string;
  size?: number;
  truncated?: boolean;
}

type LoadState<T> =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "loaded"; value: T }
  | { kind: "error"; message: string };

async function browse(request: GitHubBrowseRequest): Promise<GitHubBrowseResult> {
  return invoke<GitHubBrowseResult>("github_browse", { request });
}

/**
 * Structured view for the `github` palette action — real GitHub browsing via
 * the `github_browse` Tauri command (never a direct renderer fetch; the
 * desktop CSP `connect-src` has no `api.github.com` origin — see the header
 * comment in `src-tauri/src/github_bridge.rs`).
 *
 * Two levels of navigation, not a single undo-able history stack:
 *   1. Repo list <-> a specific repo. "Back" always means "return to the repo
 *      list" — there is nothing else to go back to once you're inside a repo,
 *      because...
 *   2. ...inside a repo, the file tree and the selected file's preview are
 *      rendered SIMULTANEOUSLY in a two-pane split (`.github-body`), modeled
 *      on `FilesView.tsx`'s `.files-body`/`.files-tree`/`.files-preview`
 *      layout. Clicking a different tree entry only swaps `selectedPath` +
 *      re-fetches the preview pane; the tree itself never re-renders from
 *      scratch and there is no "back" from a file to the tree, because both
 *      are always on screen together.
 *
 * The initial payload comes from the dispatched action (`owner`,
 * `owner/repo`, or `owner/repo/path`) and seeds the tree/selection state
 * below rather than being replayed through a history array.
 */
export const GitHubView = memo(function GitHubView({ payload }: { payload: Record<string, unknown> }) {
  const initial = payload as unknown as GitHubBrowseResult;

  const [repoRoot, setRepoRoot] = useState<GitHubBrowseResult>(initial);
  const [reposLoading, setReposLoading] = useState(false);
  const [reposError, setReposError] = useState<string | null>(null);

  const [selectedPath, setSelectedPath] = useState<string | null>(
    initial.ok && initial.kind === "file" ? (initial.path ?? null) : null,
  );
  const [file, setFile] = useState<LoadState<GitHubBrowseResult>>(
    initial.ok && initial.kind === "file" ? { kind: "loaded", value: initial } : { kind: "idle" },
  );

  const inRepo = repoRoot.ok && (repoRoot.kind === "tree" || repoRoot.kind === "file");

  const loadFile = useCallback(
    (path: string) => {
      if (!repoRoot.ok || !repoRoot.repo) return;
      setSelectedPath(path);
      setFile({ kind: "loading" });
      browse({
        kind: "file",
        owner: repoRoot.owner,
        repo: repoRoot.repo,
        path,
        branch: repoRoot.branch ?? undefined,
      })
        .then((result) => setFile(result.ok ? { kind: "loaded", value: result } : { kind: "error", message: result.error ?? "Unable to load file." }))
        .catch((err) => setFile({ kind: "error", message: errorMessage(err) }));
    },
    [repoRoot],
  );

  async function openRepo(repo: RepoSummary) {
    setReposLoading(true);
    setReposError(null);
    try {
      const result = await browse({ kind: "tree", owner: repoRoot.owner, repo: repo.name, branch: repo.default_branch });
      if (result.ok) {
        setRepoRoot(result);
        setSelectedPath(null);
        setFile({ kind: "idle" });
      } else {
        setReposError(result.error ?? "Unable to load repository.");
      }
    } finally {
      setReposLoading(false);
    }
  }

  function backToRepos() {
    setRepoRoot({ ...repoRoot, ok: true, kind: "repos", repo: null, branch: null, path: null, payload: [] });
    // Re-fetch the repo list fresh rather than trying to reconstruct it — the
    // initial `repos` payload is not retained once the user has drilled in.
    setReposLoading(true);
    browse({ kind: "repos", owner: repoRoot.owner })
      .then((result) => {
        if (result.ok) setRepoRoot(result);
        else setReposError(result.error ?? "Unable to load repositories.");
      })
      .finally(() => setReposLoading(false));
    setSelectedPath(null);
    setFile({ kind: "idle" });
  }

  if (!repoRoot.ok) {
    return (
      <div className="output-body operation-view aurora-scrollbar">
        <ResultHero
          icon={<AlertTriangle size={16} />}
          title="GitHub request failed"
          tone="warn"
          metrics={[
            ["Status", repoRoot.status || "-"],
            ["Authenticated", repoRoot.authenticated ? "yes" : "no"],
          ]}
        />
        <section className="operation-section">
          <p className="operation-muted">{repoRoot.error ?? "Unknown GitHub error."}</p>
        </section>
      </div>
    );
  }

  return (
    <div className="output-body operation-view aurora-scrollbar github-view">
      <div className="github-header">
        <ResultHero
          icon={reposLoading ? <Loader2 size={16} className="github-spin" /> : <FolderGit2 size={16} />}
          title={githubTitle(repoRoot)}
          tone="neutral"
          metrics={[
            ["Rate limit", repoRoot.rateLimitRemaining ?? "-"],
            ["Auth", repoRoot.authenticated ? "token" : "anonymous"],
          ]}
        />
        {inRepo ? (
          <button type="button" className="github-back" onClick={backToRepos} disabled={reposLoading}>
            <ChevronLeft size={13} /> Back
          </button>
        ) : null}
      </div>
      {reposError ? (
        <section className="operation-section">
          <p className="operation-muted">{reposError}</p>
        </section>
      ) : repoRoot.kind === "repos" ? (
        <RepoListView payload={repoRoot.payload} onOpenRepo={openRepo} />
      ) : (
        <GitHubSplitView
          treePayload={repoRoot.payload}
          selectedPath={selectedPath}
          file={file}
          onSelectFile={loadFile}
          repo={repoRoot.repo ? `${repoRoot.owner}/${repoRoot.repo}` : repoRoot.owner}
          branch={repoRoot.branch}
        />
      )}
    </div>
  );
});

function GitHubSplitView({
  treePayload,
  selectedPath,
  file,
  onSelectFile,
  repo,
  branch,
}: {
  treePayload: unknown;
  selectedPath: string | null;
  file: LoadState<GitHubBrowseResult>;
  onSelectFile: (path: string) => void;
  /** `owner/repo` (or just `owner` if no repo is selected yet), passed through
   * to `FilePreview`'s pvHead/pvFoot actions (Copy/Open-on-GitHub/Ask). */
  repo: string;
  branch: string | null;
}) {
  const entries = useMemo(() => {
    const tree = isRecord(treePayload) ? treePayload.tree : undefined;
    return Array.isArray(tree) ? (tree as TreeEntry[]) : [];
  }, [treePayload]);
  const files = useMemo(
    () => entries.filter((entry) => entry.type === "blob").sort((a, b) => a.path.localeCompare(b.path)),
    [entries],
  );

  return (
    <div className="github-body">
      <div className="github-tree aurora-scrollbar" role="listbox" aria-label="Repository files">
        {files.length === 0 ? (
          <EmptyResult kind="generic" />
        ) : (
          files.slice(0, 300).map((entry) => (
            <button
              key={entry.path}
              type="button"
              role="option"
              aria-selected={selectedPath === entry.path}
              className={`github-tree-row${selectedPath === entry.path ? " github-tree-row-active" : ""}`}
              onClick={() => onSelectFile(entry.path)}
            >
              <TreeEntryIcon name={entry.path} />
              <span className="github-tree-path">{entry.path}</span>
              {typeof entry.size === "number" ? <span className="github-tree-size">{formatBytes(entry.size)}</span> : null}
            </button>
          ))
        )}
      </div>
      <div className="github-preview aurora-scrollbar">
        {!selectedPath ? (
          <div className="files-empty operation-muted">Select a file</div>
        ) : file.kind === "loading" ? (
          <div className="files-empty">
            <Loader2 size={16} className="github-spin" />
            <span>Loading...</span>
          </div>
        ) : file.kind === "error" ? (
          <div className="files-empty operation-muted">{file.message}</div>
        ) : file.kind === "loaded" ? (
          <FilePreview payload={file.value.payload} repo={repo} branch={branch} path={selectedPath} />
        ) : null}
      </div>
    </div>
  );
}

function githubTitle(result: GitHubBrowseResult): string {
  switch (result.kind) {
    case "repos":
      return `${result.owner}'s repositories`;
    case "tree":
    case "file":
      return `${result.owner}/${result.repo}${result.branch ? ` @ ${result.branch}` : ""}`;
    default:
      return "GitHub";
  }
}

function RepoListView({
  payload,
  onOpenRepo,
}: {
  payload: unknown;
  onOpenRepo: (repo: RepoSummary) => void;
}) {
  const repos = useMemo(() => (Array.isArray(payload) ? (payload as RepoSummary[]) : []), [payload]);
  if (repos.length === 0) return <EmptyResult kind="generic" />;
  return (
    <section className="operation-section">
      <div className="github-repo-list">
        {repos.map((repo) => (
          <button key={repo.full_name} type="button" className="github-repo-row" onClick={() => onOpenRepo(repo)}>
            <div className="github-repo-main">
              <span className="github-repo-name">{repo.full_name}</span>
              {repo.private ? <span className="github-repo-badge">Private</span> : null}
            </div>
            {repo.description ? <p className="operation-muted">{repo.description}</p> : null}
            <div className="github-repo-meta">
              {repo.language ? <span>{repo.language}</span> : null}
              <span>★ {repo.stargazers_count}</span>
              <span>⑂ {repo.forks_count}</span>
            </div>
          </button>
        ))}
      </div>
    </section>
  );
}

function TreeEntryIcon({ name }: { name: string }) {
  const base = name.split("/").pop() ?? name;
  switch (fileKind(base)) {
    case "doc":
      return <FileText size={13} className="files-icon-doc" aria-hidden="true" />;
    case "code":
      return <FileCode size={13} className="files-icon-code" aria-hidden="true" />;
    case "config":
      return <FileCog size={13} className="files-icon-config" aria-hidden="true" />;
    case "archive":
      return <FileArchive size={13} className="files-icon-muted" aria-hidden="true" />;
    default:
      return <FileIcon size={13} className="files-icon-muted" aria-hidden="true" />;
  }
}

/**
 * Corrected against the real mock's `pvHead`/`pvFoot` chrome (verified in
 * `palette-mock.html`, search `pvHead`/`pvFoot`/`pvBody`): the mock's file
 * preview has a header action row (Copy contents, Open on GitHub, Ask about
 * this file) and a footer strip (byte size, file extension). The first
 * drafting pass's `FilePreview` (a bare Path/Size detail card + body) was
 * missing all of this — now added below as `pvHead`/`pvFoot`, ported as
 * closely as this codebase's existing primitives allow:
 *
 * - "Copy contents" and "Open on GitHub" are both straightforward: clipboard
 *   write + a toast, no new cross-component plumbing required.
 * - "Ask about this file" in the mock closes the GitHub view and pipes
 *   `"About {repo}/{path}"` into the ask action (mock: `runOp(find('ask'), ...)`).
 *   This codebase's `GitHubView` currently has no callback prop for
 *   dispatching a *different* palette action from inside a structured result
 *   view — `OperationResultView.tsx`'s `github` entry renders `<GitHubView
 *   payload={...} />` with no action-dispatch callback passed down (see
 *   `apps/palette-tauri/src/components/palette/OperationResultView.tsx`).
 *   Wiring a real "run the ask action and switch views" hookup is out of
 *   scope for this task — it would require adding an `onAskAbout` prop
 *   threaded through `OperationResultView` down to every structured view,
 *   which is a bigger, cross-cutting change than "add the mock's preview
 *   actions." **This plan ships the "Ask" button as a copy-to-clipboard
 *   affordance** (copies `"About {repo}/{path}"` to the clipboard with a
 *   toast, same mechanism as the other two buttons) rather than silently
 *   dropping the button or silently pretending it's wired end-to-end. Flagged
 *   again in "Open Questions" below — a follow-up task should add the real
 *   `onAskAbout` plumbing if full mock parity is wanted.
 */
function FilePreview({ payload, repo, branch, path }: { payload: unknown; repo: string; branch: string | null; path: string | null }) {
  const file = isRecord(payload) ? (payload as unknown as FileContents) : null;
  const [toast, setToast] = useState<string | null>(null);
  if (!file) return <EmptyResult kind="generic" />;
  const decoded = decodeFileContent(file);
  const filePath = file.path ?? path ?? "";
  const fileName = filePath.split("/").pop() ?? filePath;
  const ext = fileName.includes(".") ? fileName.split(".").pop()!.toUpperCase() : "FILE";

  function showToast(message: string) {
    setToast(message);
    setTimeout(() => setToast(null), 2000);
  }

  async function copyContents() {
    if (decoded === null) return;
    try {
      await navigator.clipboard.writeText(decoded);
      showToast(`Copied ${fileName}`);
    } catch {
      showToast("Copy failed");
    }
  }

  async function copyGitHubLink() {
    const url = `https://github.com/${repo}/blob/${branch ?? "main"}/${filePath}`;
    try {
      await navigator.clipboard.writeText(url);
      showToast(`GitHub link copied · ${fileName}`);
    } catch {
      showToast("Copy failed");
    }
  }

  async function copyAskPrompt() {
    // See this function's doc comment above: real ask-action dispatch is out
    // of scope for this task; this copies the mock's prompt text instead.
    try {
      await navigator.clipboard.writeText(`About ${repo}/${filePath}`);
      showToast("Ask prompt copied");
    } catch {
      showToast("Copy failed");
    }
  }

  return (
    <>
      <div className="github-preview-head">
        <TreeEntryIcon name={fileName} />
        <div className="github-preview-head-main">
          <div className="github-preview-head-name">{fileName}</div>
          <div className="github-preview-head-path">{repo} / {filePath}</div>
        </div>
        <button type="button" className="github-preview-action" title="Copy contents" aria-label="Copy contents" onClick={copyContents}>
          <Copy size={15} aria-hidden="true" />
        </button>
        <button type="button" className="github-preview-action" title="Open on GitHub" aria-label="Open on GitHub" onClick={copyGitHubLink}>
          <ExternalLink size={15} aria-hidden="true" />
        </button>
        <button type="button" className="github-preview-ask" title="Ask about this file" onClick={copyAskPrompt}>
          <MessageSquare size={12} aria-hidden="true" /> Ask
        </button>
      </div>
      <section className="operation-section operation-reader-section">
        {file.truncated ? (
          <p className="operation-muted">File too large to preview inline.</p>
        ) : decoded !== null ? (
          <div className="operation-reader">
            {isMarkdownLike(filePath) ? (
              <MarkdownBody>{decoded}</MarkdownBody>
            ) : (
              <pre className="output-body output-code github-file-code">{decoded}</pre>
            )}
          </div>
        ) : (
          <p className="operation-muted">Unable to decode file contents.</p>
        )}
      </section>
      <div className="github-preview-foot">
        {typeof file.size === "number" ? <span>{formatBytes(file.size)}</span> : null}
        <span className="github-preview-foot-spacer" />
        <span className="github-preview-foot-ext">{ext}</span>
      </div>
      {toast ? <div className="github-toast">{toast}</div> : null}
    </>
  );
}

function decodeFileContent(file: FileContents): string | null {
  if (typeof file.content !== "string" || !file.content) return null;
  try {
    if (file.encoding && file.encoding !== "base64") return file.content;
    const normalized = file.content.replace(/\n/g, "");
    const binary = atob(normalized);
    const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
    return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
  } catch {
    return null;
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(bytes < 10240 ? 1 : 0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
```

Note: this rewrite consolidates the duplicated `isMarkdownPath`/`isMarkdownLike` logic by importing `isMarkdownLike` from `@/lib/filesModel` (already used by `FilesView.tsx`) instead of keeping `GitHubView.tsx`'s own near-identical local copy — a small cleanup that removes drift risk between the two, per the CLAUDE.md guidance to prefer existing helpers.

Also note: when the initial payload is `kind: "file"` (deep-linked directly to a file, e.g. from a future Feed click — see Task 8), `repoRoot` is still `initial` (which has `kind: "file"`, not `"tree"`), so `GitHubSplitView`'s `treePayload` prop receives the **file's own payload**, not a tree — this is a bug in the naive reading of `inRepo`/`repoRoot.kind === "file"` branch. Fix this before moving on:

- [ ] **Step 3b: Handle the file-payload-as-initial-entry-point case**

When `initial.kind === "file"`, the view has a file to preview but has NOT been given the tree. Add a tree-fetch effect. Modify the `GitHubView` component: add this state variable near the top (with the other `useState` calls):

```ts
  const [treeState, setTreeState] = useState<LoadState<GitHubBrowseResult>>(
    initial.ok && initial.kind === "tree" ? { kind: "loaded", value: initial } : { kind: "idle" },
  );
```

Add this effect after the `loadFile` callback definition:

```ts
  useEffect(() => {
    if (!repoRoot.ok || repoRoot.kind !== "file" || !repoRoot.repo) return;
    if (treeState.kind !== "idle") return;
    setTreeState({ kind: "loading" });
    browse({ kind: "tree", owner: repoRoot.owner, repo: repoRoot.repo, branch: repoRoot.branch ?? undefined })
      .then((result) => setTreeState(result.ok ? { kind: "loaded", value: result } : { kind: "error", message: result.error ?? "Unable to load file tree." }))
      .catch((err) => setTreeState({ kind: "error", message: errorMessage(err) }));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoRoot]);
```

Change the render branch that picks between repo list and split view:

```tsx
      ) : (
        <GitHubSplitView
          treePayload={repoRoot.payload}
          selectedPath={selectedPath}
          file={file}
          onSelectFile={loadFile}
        />
      )}
```

to:

```tsx
      ) : (
        <GitHubSplitView
          treePayload={
            repoRoot.kind === "tree"
              ? repoRoot.payload
              : treeState.kind === "loaded"
                ? treeState.value.payload
                : undefined
          }
          selectedPath={selectedPath}
          file={file}
          onSelectFile={loadFile}
        />
      )}
```

Also update `openRepo` and `backToRepos` to reset `treeState` to `{ kind: "idle" }` alongside `selectedPath`/`file` (they already reset both of those — add `setTreeState({ kind: "idle" })` to each), and set `treeState` directly to `{ kind: "loaded", value: result }` in `openRepo`'s success branch (since a fresh `tree` browse already returns the tree payload — no need to re-fetch it via the effect).

- [ ] **Step 4: Add split-pane CSS**

In `apps/palette-tauri/src/styles.css`, find the `.github-tree-list` block (~line 6260) and add immediately after the existing `.github-*` rules (before or after, doesn't matter — keep all `.github-*` rules contiguous per existing file organization):

```css
.github-body {
  display: flex;
  flex: 1;
  min-height: 0;
  gap: 1px;
  background: var(--aurora-border);
}

.github-tree {
  flex: 0 0 34%;
  min-width: 220px;
  max-width: 360px;
  overflow-y: auto;
  background: var(--aurora-surface);
  display: flex;
  flex-direction: column;
}

.github-tree-row-active {
  background: var(--aurora-accent-surface);
}

.github-preview {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  background: var(--aurora-surface);
  padding: 0.5rem 0;
  display: flex;
  flex-direction: column;
}

/* pvHead/pvFoot — mock-matched preview-pane chrome (see FilePreview's doc
 * comment in Task 6): a header action row (name/path + Copy/Open-on-GitHub/
 * Ask) and a footer strip (byte size + extension chip). */
.github-preview-head {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.625rem;
  border-bottom: 1px solid var(--aurora-border);
}

.github-preview-head-main {
  flex: 1;
  min-width: 0;
}

.github-preview-head-name {
  font-size: 0.8125rem;
  font-weight: 700;
  color: var(--aurora-text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.github-preview-head-path {
  font-size: 0.65625rem;
  color: var(--aurora-text-muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.github-preview-action {
  appearance: none;
  cursor: pointer;
  flex: 0 0 auto;
  display: grid;
  place-items: center;
  width: 28px;
  height: 28px;
  border-radius: 7px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--aurora-text-muted);
}

.github-preview-ask {
  appearance: none;
  cursor: pointer;
  flex: 0 0 auto;
  white-space: nowrap;
  display: inline-flex;
  align-items: center;
  gap: 5px;
  margin-left: 2px;
  font-size: 0.6875rem;
  font-weight: 600;
  color: var(--aurora-accent-pink);
  background: color-mix(in srgb, var(--aurora-accent-pink) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--aurora-accent-pink) 30%, var(--aurora-border));
  border-radius: 7px;
  padding: 4px 9px;
}

.github-preview-foot {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.5rem 0.8125rem;
  border-top: 1px solid var(--aurora-border);
  font-size: 0.65625rem;
  color: var(--aurora-text-muted);
}

.github-preview-foot-spacer {
  flex: 1;
}

.github-preview-foot-ext {
  text-transform: uppercase;
  letter-spacing: 0.06em;
}

.github-toast {
  position: absolute;
  bottom: 14px;
  left: 50%;
  transform: translateX(-50%);
  z-index: 80;
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--aurora-text-primary);
  background: var(--aurora-panel-strong);
  border: 1px solid var(--aurora-border);
  border-radius: 999px;
  padding: 7px 14px;
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.4);
}
```

Verify `--aurora-border`, `--aurora-surface`, `--aurora-accent-surface`, `--aurora-accent-pink`, `--aurora-panel-strong`, `--aurora-text-primary`, `--aurora-text-muted` already exist as tokens (the mock's `pvHead`'s Ask button and toast use a rose accent and a panel background — confirmed against the real token names already used elsewhere in `apps/palette-tauri/src/styles.css`, e.g. `--aurora-accent-pink` for the secondary/rose accent and `--aurora-panel-strong` for the mock's window background, per this plan's "Token discipline" Global Constraint):

Run: `grep -n '\-\-aurora-border:\|\-\-aurora-surface:\|\-\-aurora-accent-surface:\|\-\-aurora-accent-pink:\|\-\-aurora-panel-strong:\|\-\-aurora-text-primary:\|\-\-aurora-text-muted:' apps/palette-tauri/src/components/aurora.css apps/palette-tauri/src/styles.css`
Expected: at least one definition of each; if any is missing, use the closest existing token instead of introducing a new one (check `.files-tree`/`.files-preview`'s existing CSS block for the exact tokens they already use and match those instead of guessing).

- [ ] **Step 5: Run the full GitHubView test suite**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/GitHubView.test.tsx`
Expected: PASS, all 9 tests (7 original + 2 new from Step 1, with the modified Back test)

- [ ] **Step 6: Typecheck**

Run: `cd apps/palette-tauri && pnpm typecheck`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src/components/palette/GitHubView.tsx apps/palette-tauri/src/components/palette/GitHubView.test.tsx apps/palette-tauri/src/styles.css
git commit -m "refactor(palette): convert GitHubView to a two-pane tree+preview split"
```

---

## Task 7: `GitHubFeedView.tsx` — the Feed tab component

**Files:**
- Create: `apps/palette-tauri/src/components/palette/GitHubFeedView.tsx`
- Create: `apps/palette-tauri/src/components/palette/GitHubFeedView.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css`

**Interfaces:**
- Consumes: `groupFeedByDay`, `feedKindLabel`, `feedKindIcon`, `FeedItem`, `FeedPayload` (Task 5).
- Produces: `export function GitHubFeedView({ owner, onOpenItem }: { owner: string; onOpenItem: (item: FeedItem) => void }): JSX.Element` — self-contained, fetches its own data on mount via `github_browse({ kind: "feed", owner })`, consumed by Task 8's tab wiring in `GitHubView.tsx`.

- [ ] **Step 1: Write the failing test**

Create `apps/palette-tauri/src/components/palette/GitHubFeedView.test.tsx`:

```tsx
// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, fireEvent } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();

vi.mock("@/lib/invoke", () => ({
  isTauriRuntime: false,
  invoke: (...args: unknown[]) => invokeMock(...args),
  appWindow: { listen: () => Promise.resolve(() => {}) },
}));

import { GitHubFeedView } from "./GitHubFeedView";

afterEach(() => {
  cleanup();
  invokeMock.mockReset();
});

const feedResult = {
  ok: true,
  status: 200,
  kind: "feed",
  owner: "jmagar",
  repo: null,
  branch: null,
  path: null,
  payload: {
    items: [
      {
        kind: "push",
        repo: "jmagar/axon",
        actor: "jmagar",
        title: "fix: bug in `src/main.rs`",
        url: "https://github.com/jmagar/axon/commits",
        path: "src/main.rs",
        num: null,
        meta: "3 commits · main",
        badge: null,
        timestampUnix: Math.floor(Date.now() / 1000),
      },
    ],
    partial: false,
    errors: [],
  },
  error: null,
  rateLimitRemaining: 55,
  rateLimitReset: null,
  authenticated: true,
};

const emptyFeedResult = { ...feedResult, payload: { items: [], partial: false, errors: [] } };

const errorFeedResult = {
  ok: false,
  status: 403,
  kind: "feed",
  owner: "jmagar",
  repo: null,
  branch: null,
  path: null,
  payload: null,
  error: "GitHub API rate limited — retry later",
  rateLimitRemaining: 0,
  rateLimitReset: null,
  authenticated: false,
};

describe("GitHubFeedView", () => {
  it("fetches and renders feed items grouped by day", async () => {
    invokeMock.mockResolvedValueOnce(feedResult);
    render(<GitHubFeedView owner="jmagar" onOpenItem={() => {}} />);
    expect(invokeMock).toHaveBeenCalledWith("github_browse", { request: { kind: "feed", owner: "jmagar" } });
    await waitFor(() => expect(screen.getByText(/fix: bug/)).toBeInTheDocument());
    expect(screen.getByText("Today")).toBeInTheDocument();
  });

  it("renders an empty state when there are no items", async () => {
    invokeMock.mockResolvedValueOnce(emptyFeedResult);
    render(<GitHubFeedView owner="jmagar" onOpenItem={() => {}} />);
    await waitFor(() => expect(screen.getByText(/no activity/i)).toBeInTheDocument());
  });

  it("renders an error state on failure", async () => {
    invokeMock.mockResolvedValueOnce(errorFeedResult);
    render(<GitHubFeedView owner="jmagar" onOpenItem={() => {}} />);
    await waitFor(() => expect(screen.getByText(/rate limited/)).toBeInTheDocument());
  });

  it("calls onOpenItem when a feed row is clicked", async () => {
    invokeMock.mockResolvedValueOnce(feedResult);
    const onOpenItem = vi.fn();
    render(<GitHubFeedView owner="jmagar" onOpenItem={onOpenItem} />);
    await waitFor(() => expect(screen.getByText(/fix: bug/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/fix: bug/));
    expect(onOpenItem).toHaveBeenCalledWith(feedResult.payload.items[0]);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/GitHubFeedView.test.tsx`
Expected: FAIL — component doesn't exist

- [ ] **Step 3: Implement `GitHubFeedView.tsx`**

Create `apps/palette-tauri/src/components/palette/GitHubFeedView.tsx`:

```tsx
import { AlertTriangle, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";

import { EmptyResult } from "@/components/palette/OperationResultViewShared";
import type { GitHubBrowseResult } from "@/lib/actionRequest";
import { feedKindLabel, feedKindIcon, groupFeedByDay, type FeedItem, type FeedPayload } from "@/lib/githubFeed";
import { invoke } from "@/lib/invoke";
import { isRecord } from "@/lib/payload";

type LoadState =
  | { kind: "loading" }
  | { kind: "loaded"; payload: FeedPayload }
  | { kind: "error"; message: string };

/**
 * The Feed tab's content: fetches `github_browse({ kind: "feed", owner })` on
 * mount (a fresh authenticated-if-possible fan-out across the owner's most
 * recently updated repos — see `src-tauri/src/github_feed.rs` for the fan-out
 * and normalization), groups results by day, and renders one row per item.
 * Clicking a row calls `onOpenItem`, which the parent `GitHubView` uses to
 * jump into the split-pane tree/preview view (opening the item's repo, and
 * its `path` when the event's `path` heuristic found one) — see
 * `GitHubView.tsx`'s `onFeedItemOpen`.
 */
export function GitHubFeedView({
  owner,
  onOpenItem,
}: {
  owner: string;
  onOpenItem: (item: FeedItem) => void;
}) {
  const [state, setState] = useState<LoadState>({ kind: "loading" });

  useEffect(() => {
    let cancelled = false;
    setState({ kind: "loading" });
    invoke<GitHubBrowseResult>("github_browse", { request: { kind: "feed", owner } })
      .then((result) => {
        if (cancelled) return;
        if (!result.ok) {
          setState({ kind: "error", message: result.error ?? "Unable to load activity feed." });
          return;
        }
        const payload = isRecord(result.payload) ? (result.payload as unknown as FeedPayload) : { items: [], partial: false, errors: [] };
        setState({ kind: "loaded", payload });
      })
      .catch((err) => {
        if (!cancelled) setState({ kind: "error", message: err instanceof Error ? err.message : String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [owner]);

  if (state.kind === "loading") {
    return (
      <section className="operation-section github-feed-loading">
        <Loader2 size={16} className="github-spin" />
        <span>Loading activity...</span>
      </section>
    );
  }

  if (state.kind === "error") {
    return (
      <section className="operation-section">
        <div className="github-feed-error">
          <AlertTriangle size={14} />
          <span>{state.message}</span>
        </div>
      </section>
    );
  }

  const groups = groupFeedByDay(state.payload.items);
  if (groups.length === 0) {
    return <EmptyResult kind="generic" />;
  }

  return (
    <section className="operation-section">
      {state.payload.partial ? (
        <p className="operation-muted github-feed-partial">
          Some repos could not be loaded ({state.payload.errors.length}) — showing partial results.
        </p>
      ) : null}
      <div className="github-feed">
        {groups.map((group) => (
          <div key={group.label} className="github-feed-day">
            <h3 className="stats-heading">{group.label}</h3>
            <div className="operation-list">
              {group.items.map((item, index) => (
                <FeedRow key={`${item.repo}-${item.timestampUnix}-${index}`} item={item} onOpen={() => onOpenItem(item)} />
              ))}
            </div>
          </div>
        ))}
      </div>
    </section>
  );
}

/**
 * Corrected against the real mock's `feedRow()` markup (verified in
 * `palette-mock.html`, search `feedRow`): a colored icon swatch in the kind's
 * tone, a header line of [owner/repo] + [kind label, in the kind's tone] +
 * [#num if present], the title, and a meta line of [actor initial chip +
 * actor name] + middot + [meta string] + [badge], with relative time shown
 * to the right of the row (this plan renders it via `formatRelativeTime`
 * below rather than a mock-fixture `time` string, since `FeedItem` carries a
 * raw `timestampUnix`, not a precomputed relative string). The first
 * drafting pass's `FeedRow` (icon + title + "repo · actor" line + a single
 * kind badge on the right) did not match this structure — now corrected.
 */
function FeedRow({ item, onOpen }: { item: FeedItem; onOpen: () => void }) {
  const Icon = feedKindIcon(item.kind);
  return (
    <button type="button" className="operation-row github-feed-row" onClick={onOpen}>
      <span className="github-feed-icon">
        <Icon size={15} aria-hidden="true" />
      </span>
      <div className="operation-row-main">
        <div className="github-feed-row-head">
          <span className="github-feed-repo">{item.repo}</span>
          <span className="github-feed-kind">{feedKindLabel(item.kind)}</span>
          {item.num !== null ? <span className="github-feed-num">#{item.num}</span> : null}
        </div>
        <div className="operation-row-title">{item.title}</div>
        <div className="github-feed-row-meta">
          <span className="github-feed-actor">
            <span className="github-feed-actor-chip">{(item.actor[0] ?? "?").toUpperCase()}</span>
            {item.actor}
          </span>
          <span className="github-feed-dot">·</span>
          <span>{item.meta}</span>
          <FeedBadgeView badge={item.badge} />
        </div>
      </div>
      <span className="github-feed-time">{formatRelativeTime(item.timestampUnix)}</span>
    </button>
  );
}

/** Mirrors the mock's `feedBadge()`: a `{add, del}` line-diff pair, or a
 * short status-label pill. Renders nothing when `badge` is `null` — the
 * common case for kinds this plan can't reliably badge yet, see Task 3. */
function FeedBadgeView({ badge }: { badge: FeedItem["badge"] }) {
  if (!badge) return null;
  if (badge.type === "diff") {
    return (
      <span className="github-feed-diff">
        <span className="github-feed-diff-add">+{badge.add}</span>
        <span className="github-feed-diff-del">-{badge.del}</span>
      </span>
    );
  }
  return <span className="github-feed-badge">{badge.value}</span>;
}

/** Coarse relative-time formatter for the feed row's right-aligned time
 * column (mock examples: "11m", "34m", "3h", "Yesterday", "2d"). */
function formatRelativeTime(timestampUnix: number): string {
  const deltaSec = Math.max(0, Math.floor(Date.now() / 1000) - timestampUnix);
  if (deltaSec < 3600) return `${Math.max(1, Math.floor(deltaSec / 60))}m`;
  if (deltaSec < 86400) return `${Math.floor(deltaSec / 3600)}h`;
  return `${Math.floor(deltaSec / 86400)}d`;
}
```

- [ ] **Step 4: Add feed-specific CSS**

In `apps/palette-tauri/src/styles.css`, add after the `.github-preview` block from Task 6:

```css
.github-feed-loading,
.github-feed-error {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  color: var(--aurora-text-muted);
}

.github-feed-day + .github-feed-day {
  margin-top: 1rem;
}

.github-feed-partial {
  margin-bottom: 0.5rem;
}

.github-feed-row {
  width: 100%;
  text-align: left;
  cursor: pointer;
  align-items: flex-start;
  gap: 12px;
}

/* feedRow chrome corrected against the real mock (see FeedRow's doc comment
 * in Task 7): icon swatch, header line (repo/kind/num), title, meta line
 * (actor chip + meta + badge), right-aligned relative time. */
.github-feed-icon {
  width: 30px;
  height: 30px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  flex: 0 0 auto;
  margin-top: 1px;
  color: var(--aurora-accent-primary);
  background: color-mix(in srgb, var(--aurora-accent-primary) 13%, transparent);
  border: 1px solid color-mix(in srgb, var(--aurora-accent-primary) 28%, var(--aurora-border));
}

.github-feed-row-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.github-feed-kind {
  font-size: 0.65625rem;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--aurora-accent-primary);
}

.github-feed-num {
  font-size: 0.71875rem;
  color: var(--aurora-text-muted);
}

.github-feed-row-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 5px;
  flex-wrap: wrap;
  font-size: 0.71875rem;
  color: var(--aurora-text-muted);
}

.github-feed-actor {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.github-feed-actor-chip {
  width: 15px;
  height: 15px;
  border-radius: 999px;
  display: grid;
  place-items: center;
  background: color-mix(in srgb, var(--aurora-accent-primary) 22%, var(--aurora-surface));
  color: var(--aurora-accent-primary);
  font-size: 0.53125rem;
  font-weight: 700;
  flex: 0 0 auto;
}

.github-feed-dot {
  opacity: 0.45;
}

.github-feed-diff {
  display: inline-flex;
  gap: 7px;
  font-size: 0.6875rem;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}

.github-feed-diff-add {
  color: var(--aurora-success);
}

.github-feed-diff-del {
  color: var(--aurora-error);
}

.github-feed-badge {
  font-size: 0.65625rem;
  font-weight: 700;
  letter-spacing: 0.02em;
  color: var(--aurora-text-muted);
  background: color-mix(in srgb, var(--aurora-text-muted) 14%, transparent);
  border: 1px solid color-mix(in srgb, var(--aurora-text-muted) 30%, transparent);
  border-radius: 999px;
  padding: 1px 8px;
}

.github-feed-time {
  font-size: 0.6875rem;
  color: var(--aurora-text-muted);
  flex: 0 0 auto;
  white-space: nowrap;
  margin-top: 2px;
}
```

Verify `--aurora-text-muted`, `--aurora-accent-primary`, `--aurora-success`, `--aurora-error` exist (confirmed present in `apps/palette-tauri/src/styles.css`'s existing token mappings; `--aurora-accent` alone is not a real token name — `--aurora-accent-primary` is) (or substitute the token `OperationResultViewShared.tsx`'s `.operation-muted` class already uses):

Run: `grep -n '\.operation-muted' apps/palette-tauri/src/styles.css | head -3`

Match whatever color token that rule already uses instead of guessing a new name.

- [ ] **Step 5: Run the tests**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/GitHubFeedView.test.tsx`
Expected: PASS, all 4 tests green

- [ ] **Step 6: Typecheck**

Run: `cd apps/palette-tauri && pnpm typecheck`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src/components/palette/GitHubFeedView.tsx apps/palette-tauri/src/components/palette/GitHubFeedView.test.tsx apps/palette-tauri/src/styles.css
git commit -m "feat(palette): add GitHubFeedView, the Feed tab's renderer"
```

---

## Task 8: Wire the Feed tab into `GitHubView` and link Feed items into the split view

**Files:**
- Modify: `apps/palette-tauri/src/components/palette/GitHubView.tsx`
- Modify: `apps/palette-tauri/src/components/palette/GitHubView.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css`

**Interfaces:**
- Consumes: `GitHubFeedView` (Task 7), `FeedItem` (Task 5).
- Produces: none further — this is the integration point where Tasks 3–7 come together inside one component.

- [ ] **Step 1: Write the failing test**

Add to `apps/palette-tauri/src/components/palette/GitHubView.test.tsx` (after the existing tests, before the closing `});`):

```ts
  it("shows a Feed tab and switches to it, fetching the owner's activity feed", async () => {
    const feedResult = {
      ok: true,
      status: 200,
      kind: "feed",
      owner: "jmagar",
      repo: null,
      branch: null,
      path: null,
      payload: { items: [], partial: false, errors: [] },
      error: null,
      rateLimitRemaining: 55,
      rateLimitReset: null,
      authenticated: false,
    };
    invokeMock.mockResolvedValueOnce(feedResult);
    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByRole("tab", { name: /feed/i }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("github_browse", { request: { kind: "feed", owner: "jmagar" } }),
    );
  });

  it("clicking a Feed item with a path jumps into the split view at that file", async () => {
    const feedItem = {
      kind: "push",
      repo: "jmagar/axon",
      actor: "jmagar",
      title: "fix: bug in `README.md`",
      url: "https://github.com/jmagar/axon/commits",
      path: "README.md",
      num: null,
      meta: "1 commits · main",
      badge: null,
      timestampUnix: Math.floor(Date.now() / 1000),
    };
    const feedResult = {
      ok: true,
      status: 200,
      kind: "feed",
      owner: "jmagar",
      repo: null,
      branch: null,
      path: null,
      payload: { items: [feedItem], partial: false, errors: [] },
      error: null,
      rateLimitRemaining: 55,
      rateLimitReset: null,
      authenticated: false,
    };
    invokeMock.mockResolvedValueOnce(feedResult); // feed fetch
    invokeMock.mockResolvedValueOnce(treeResult); // tree fetch for jmagar/axon
    invokeMock.mockResolvedValueOnce(fileResult); // file fetch for README.md

    render(<GitHubView payload={reposResult} />);
    fireEvent.click(screen.getByRole("tab", { name: /feed/i }));
    await waitFor(() => expect(screen.getByText(/fix: bug/)).toBeInTheDocument());
    fireEvent.click(screen.getByText(/fix: bug/));

    await waitFor(() => expect(screen.getByText(/hello/)).toBeInTheDocument());
    // Landed in the split view with the tree visible alongside the preview.
    expect(screen.getByText("src/main.rs")).toBeInTheDocument();
  });
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/GitHubView.test.tsx`
Expected: FAIL — no `tab` role elements exist yet

- [ ] **Step 3: Add the tab switcher and Feed integration to `GitHubView.tsx`**

Add the import at the top of `GitHubView.tsx`:

```ts
import { GitHubFeedView } from "@/components/palette/GitHubFeedView";
import type { FeedItem } from "@/lib/githubFeed";
```

Add a new state variable near the top of the `GitHubView` component (with the other `useState` calls):

```ts
  const [activeTab, setActiveTab] = useState<"browse" | "feed">("browse");
```

Add the Feed-item-click handler after `loadFile`:

```ts
  const openFeedItem = useCallback(
    (item: FeedItem) => {
      const [feedOwner, feedRepo] = item.repo.split("/");
      if (!feedOwner || !feedRepo) return;
      setActiveTab("browse");
      setReposLoading(true);
      browse({ kind: "tree", owner: feedOwner, repo: feedRepo })
        .then((result) => {
          if (!result.ok) {
            setReposError(result.error ?? "Unable to load repository.");
            return;
          }
          setRepoRoot(result);
          setTreeState({ kind: "loaded", value: result });
          if (item.path) {
            loadFile(item.path);
          } else {
            setSelectedPath(null);
            setFile({ kind: "idle" });
          }
        })
        .finally(() => setReposLoading(false));
    },
    [loadFile],
  );
```

Note: `openFeedItem` calls `loadFile(item.path)` which reads `repoRoot` via closure — but `repoRoot` hasn't been updated to the new repo yet at the point `loadFile` is invoked inside the `.then()` callback (React state updates from `setRepoRoot` are async). This is a real bug to avoid: `loadFile` as defined in Task 6 reads `repoRoot.owner`/`repoRoot.repo` from its own closure over the *old* `repoRoot`, not the one just set. Fix this by making the file-fetch inline here rather than reusing `loadFile`:

Replace the `openFeedItem` body's `if (item.path) { loadFile(item.path); }` branch with a direct fetch that doesn't depend on `repoRoot` having updated yet:

```ts
  const openFeedItem = useCallback((item: FeedItem) => {
    const [feedOwner, feedRepo] = item.repo.split("/");
    if (!feedOwner || !feedRepo) return;
    setActiveTab("browse");
    setReposLoading(true);
    browse({ kind: "tree", owner: feedOwner, repo: feedRepo })
      .then((result) => {
        if (!result.ok) {
          setReposError(result.error ?? "Unable to load repository.");
          return;
        }
        setRepoRoot(result);
        setTreeState({ kind: "loaded", value: result });
        if (item.path) {
          setSelectedPath(item.path);
          setFile({ kind: "loading" });
          browse({ kind: "file", owner: feedOwner, repo: feedRepo, path: item.path, branch: result.branch ?? undefined })
            .then((fileResult) =>
              setFile(
                fileResult.ok
                  ? { kind: "loaded", value: fileResult }
                  : { kind: "error", message: fileResult.error ?? "Unable to load file." },
              ),
            )
            .catch((err) => setFile({ kind: "error", message: errorMessage(err) }));
        } else {
          setSelectedPath(null);
          setFile({ kind: "idle" });
        }
      })
      .finally(() => setReposLoading(false));
  }, []);
```

**Corrected against the real mock's `feedView()` header chrome** (verified in `palette-mock.html`, search `ghSeg`/`feedView`): the mock does not have a plain multi-tab bar. Its Feed view header is: back button, GitHub glyph, title `"GitHub"` + subtitle `"{owner} · activity across {N} repos"`, and a **2-option segmented pill control** (`ghSeg()` — Feed/Repos, rendered as a small pill-shaped switcher with the active option highlighted, not underlined tabs). The first drafting pass's plain `role="tablist"` underlined-tab-bar visual (`.github-tabs`/`.github-tab`) does not match this — the fix below keeps the same `activeTab` state, `role="tab"`/`aria-selected` accessibility semantics, and test IDs (so Step 1's test assertions are unaffected), but restyles the two buttons as a pill switcher (`.github-seg`/`.github-seg-btn`) instead of underlined tabs, and adds the subtitle/repo-count line the mock always shows. There are still exactly 2 options ("Browse" for repos/tree/file, "Feed" for activity) — this plan does not add a third option, matching the mock's exact 2-way `ghSeg()`.

Now add the segmented control and route rendering by `activeTab`. Replace the render section (everything from `{reposError ? (` through the closing of that ternary chain) with:

```tsx
      <div className="github-seg" role="tablist" aria-label="GitHub view">
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "browse"}
          className={`github-seg-btn${activeTab === "browse" ? " github-seg-btn-active" : ""}`}
          onClick={() => setActiveTab("browse")}
        >
          Browse
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTab === "feed"}
          className={`github-seg-btn${activeTab === "feed" ? " github-seg-btn-active" : ""}`}
          onClick={() => setActiveTab("feed")}
        >
          Feed
        </button>
      </div>
      {activeTab === "feed" ? (
        <GitHubFeedView owner={repoRoot.owner} onOpenItem={openFeedItem} />
      ) : reposError ? (
        <section className="operation-section">
          <p className="operation-muted">{reposError}</p>
        </section>
      ) : repoRoot.kind === "repos" ? (
        <RepoListView payload={repoRoot.payload} onOpenRepo={openRepo} />
      ) : (
        <GitHubSplitView
          treePayload={
            repoRoot.kind === "tree"
              ? repoRoot.payload
              : treeState.kind === "loaded"
                ? treeState.value.payload
                : undefined
          }
          selectedPath={selectedPath}
          file={file}
          onSelectFile={loadFile}
        />
      )}
```

- [ ] **Step 4: Add segmented-control CSS**

In `apps/palette-tauri/src/styles.css`, add near the `.github-header` block (Task 6's additions). This replaces the first drafting pass's underlined `.github-tabs`/`.github-tab` with a pill switcher matching the real mock's `ghSeg()` (see the note above Step 3's tab-bar edit):

```css
.github-seg {
  display: inline-flex;
  gap: 3px;
  padding: 3px;
  border-radius: 9px;
  background: var(--aurora-surface);
  border: 1px solid var(--aurora-border);
  flex: 0 0 auto;
  margin: 0 0.75rem 0.5rem;
}

.github-seg-btn {
  appearance: none;
  cursor: pointer;
  font-size: 0.75rem;
  font-weight: 600;
  padding: 4px 12px;
  border-radius: 6px;
  border: none;
  color: var(--aurora-text-muted);
  background: transparent;
}

.github-seg-btn-active {
  color: var(--aurora-text-primary);
  background: color-mix(in srgb, var(--aurora-accent-primary) 18%, transparent);
}
```

Verify `--aurora-text-primary`/`--aurora-accent-primary`/`--aurora-surface`/`--aurora-border` token names against `apps/palette-tauri/src/components/aurora.css` before committing (confirmed present in `apps/palette-tauri/src/styles.css`'s existing token mappings as of this revision — note the bare name `--aurora-accent` is NOT a real token, `--aurora-accent-primary` is).

Run: `grep -n '\-\-aurora-text-primary:\|\-\-aurora-accent-primary:' apps/palette-tauri/src/components/aurora.css`

- [ ] **Step 5: Run the full GitHubView test suite**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/GitHubView.test.tsx`
Expected: PASS, all 11 tests (9 from Task 6 + 2 new)

- [ ] **Step 6: Typecheck**

Run: `cd apps/palette-tauri && pnpm typecheck`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add apps/palette-tauri/src/components/palette/GitHubView.tsx apps/palette-tauri/src/components/palette/GitHubView.test.tsx apps/palette-tauri/src/styles.css
git commit -m "feat(palette): wire Feed tab into GitHubView and link feed items to the split view"
```

---

## Task 9: Full verification pass

**Files:** none (verification only)

- [ ] **Step 1: Full frontend test suite**

Run: `cd apps/palette-tauri && pnpm test`
Expected: PASS, no regressions in any other `*.test.ts(x)` file (in particular, confirm nothing else imports `GitHubView`'s removed `history`/`goBack` exports — it exports only the component and the re-exported `GitHubBrowseResult` type, so nothing external should reference the removed internals)

- [ ] **Step 2: Full frontend typecheck**

Run: `cd apps/palette-tauri && pnpm typecheck`
Expected: PASS

- [ ] **Step 3: Frontend lint**

Run: `cd apps/palette-tauri && pnpm lint` (check `package.json` for the exact script name if this differs)
Expected: PASS

- [ ] **Step 4: Full Rust test suite for the Tauri shell**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`
Expected: PASS

- [ ] **Step 5: Rust clippy**

Run: `cargo clippy --manifest-path apps/palette-tauri/src-tauri/Cargo.toml -- -D warnings`
Expected: PASS — pay particular attention to any "unused function" warning on `build_request_url`'s `Feed` arm (added in Task 2) if `github_browse_feed` (Task 4) ends up not calling it — if so, either wire it in (e.g. use it to validate `repo` shape before delegating to `github_feed::fetch_feed`, since `github_feed.rs`'s per-repo URL building currently duplicates the format string rather than calling `build_request_url`) or remove the dead arm and its test. Prefer wiring it in: change `github_feed.rs::fetch_repo_events`'s URL construction to call `crate::github_bridge::build_request_url` — but note `build_request_url` is currently a private (non-`pub(crate)`) function; make it `pub(crate)` alongside `GITHUB_API_BASE` if you take this route, to avoid the literal URL format string existing in two places.

- [ ] **Step 6: Rust format check**

Run: `cargo fmt --manifest-path apps/palette-tauri/src-tauri/Cargo.toml --check`
Expected: PASS (run `cargo fmt --manifest-path apps/palette-tauri/src-tauri/Cargo.toml` first if not)

- [ ] **Step 7: Monolith check on all changed Rust files**

Run: `wc -l apps/palette-tauri/src-tauri/src/github_bridge.rs apps/palette-tauri/src-tauri/src/github_feed.rs`
Expected: both ≤500 lines. If `github_bridge.rs` grew past 500 with the `github_browse_feed` addition from Task 4, extract `github_browse_feed` + `build_feed_payload` into `github_feed.rs` instead (they already depend on `github_feed`'s types) — moving them there is a clean cut since they're Feed-specific, not shared with the other four kinds.

- [ ] **Step 8: Manual verification in the real Tauri shell (not the browser-dev fallback)**

This step cannot be automated — the dev fallback (`invoke.ts`) is deliberately simplified and unauthenticated, so it will not catch bridge-specific bugs (token attachment, real rate-limit headers, real Events API response shapes GitHub actually sends, which can include fields absent from this plan's hand-written fixtures).

Run: `cd apps/palette-tauri && pnpm tauri dev`

In the running app:
1. Set a real `GITHUB_TOKEN` in `~/.axon/.env` (a PAT with `public_repo` scope is sufficient — no special scope needed for Events API on public repos).
2. Run the `github <your-username>` action, confirm the repo list still renders.
3. Click into a repo, confirm the split view shows the tree AND select a file — confirm the preview pane updates without the tree disappearing, and selecting a second file updates only the preview (Task 6's core behavior).
4. Click "Back", confirm it returns to the repo list (not a partial "undo one click" state).
5. Click the "Feed" tab, confirm real GitHub activity loads (requires the account to actually have recent push/PR/issue/release activity on at least one of its 10 most-recently-updated repos — if the test account is quiet, temporarily point `owner` at a busy public account like `torvalds` or an active org to exercise this with real traffic).
6. Click a Feed item whose title contains a backtick-quoted path (a real commit message that happens to reference a file) and confirm it opens the split view at that repo/file. Click a Feed item with no extractable path and confirm it opens the split view at the repo's tree root without a preview selected (graceful degrade, not an error).
7. Watch the `rateLimitRemaining` metric in the hero after the Feed load — confirm it dropped by roughly (1 for the repo list, if fetched + up to 10 for the fan-out), consistent with `MAX_FEED_REPOS`.

- [ ] **Step 9: Update `apps/palette-tauri/CHANGELOG.md`**

Check the existing changelog format:

Run: `head -30 apps/palette-tauri/CHANGELOG.md`

Add an `### Added` entry under the current unreleased/next version section (create one if release-please hasn't yet, following whatever pattern the file already uses for prior entries):

```markdown
### Added

- GitHub view: a new Feed tab shows a cross-repo activity feed (pushes, PRs,
  merges, reviews, issues, releases, dependency bumps) for the browsed
  owner's most recently updated repos, grouped by day.
- GitHub view: the repo browser is now a two-pane split (file tree + live
  preview) instead of a sequential tree screen followed by a separate file
  preview screen. Selecting a different file updates the preview in place.
```

- [ ] **Step 10: Final commit**

```bash
git add apps/palette-tauri/CHANGELOG.md
git commit -m "docs(palette): changelog entry for GitHub Feed tab and split view"
```

---

## Testing Strategy Summary

| Layer | What's tested | How |
|---|---|---|
| `github_bridge.rs` URL routing (Task 2) | New `Feed` kind reaches `parse_kind`/`build_request_url` correctly | Sidecar unit tests, no network |
| `github_feed.rs` (Task 3) | Event-type → `FeedItem` classification (7 event shapes), sort order, repo cap | Sidecar unit tests against hand-built `serde_json::Value` fixtures, no network |
| `github_bridge.rs` Feed dispatch (Task 4) | Payload serialization shape (`items`/`partial`/`errors`) | Unit test on `build_feed_payload`, no network (live HTTP dispatch is manual-only, consistent with the rest of this bridge's existing test coverage) |
| `lib/githubFeed.ts` (Task 5) | Day-grouping boundary conditions (today/yesterday/earlier, empty groups), kind label mapping | Co-located Vitest unit tests, no rendering |
| `GitHubView.tsx` split-pane behavior (Task 6) | Simultaneous tree+preview rendering, per-file selection swap, "Back to repos" semantics | Co-located Vitest + Testing Library render tests, mocked `invoke` |
| `GitHubFeedView.tsx` (Task 7) | Fetch-on-mount, day grouping renders, empty/error states, click → `onOpenItem` | Co-located Vitest + Testing Library render tests, mocked `invoke` |
| Feed → split-view integration (Task 8) | Segmented-control switch triggers feed fetch; clicking a feed item with a path lands in the split view with that file loaded | Co-located Vitest + Testing Library render tests, mocked `invoke`, multi-call sequencing |
| End-to-end real API behavior | Real `GITHUB_TOKEN` auth, real rate-limit headers, real Events API payload shapes | Manual verification only (Task 9, Step 8) — no automated live-network test exists for this bridge today, and this plan does not introduce one, consistent with existing project convention |

No new test-double/mocking library is introduced — the plan reuses the existing `vi.mock("@/lib/invoke", ...)` pattern already established in `GitHubView.test.tsx`, and the existing zero-network-mocking convention already established in `github_bridge_tests.rs` (pure function tests only; live HTTP is never exercised in Rust tests for this bridge).

---

## Open Questions / Risks

1. **The reference mock (`palette-mock.html`) is now available and has been diffed against this plan's reconstruction; several corrections were applied.** It lives at `./palette-mock.html` in the repo root. It was not available during the first drafting pass, so that pass reconstructed the Feed taxonomy, `FeedItem` shape, `feedRow()` markup, and `pvHead`/`pvFoot` split-view chrome from the task's prose description plus the closest working precedent in the codebase (`FilesView.tsx`). Diffing against the real mock (search `feedView`, `feedRow`, `FEED_KIND`, `var FEED`, `walk(`, `pvBody`, `pvHead`, `pvFoot`) found and corrected the following in Tasks 3, 5, 7, and 8: the `FEED_KIND` taxonomy (real kinds are `pr`/`merge`/`review`/`comment`/`conflict`/`deps`/`issue`/`push`/`release` — the first pass invented a `"dependency-bump"` kind that doesn't exist and used `"Pull request"` instead of the mock's `"Pull Request"`); the `FeedItem` shape (added `num`, `meta`, `badge` fields the mock's `FEED` array items carry that the first pass omitted); `feedRow()`'s markup (icon swatch + repo/kind/num header line + meta line with actor chip and badge, not the first pass's simpler icon+title+badge row); the Feed/Repos header control (a 2-option segmented pill switcher, `ghSeg()`, not an underlined tab bar); and the file preview's `pvHead`/`pvFoot` chrome (Copy contents / Open on GitHub / Ask action row + byte-size/extension footer, entirely absent from the first pass's `FilePreview`). See items 8–11 below for what remains open even after these corrections.
2. **The Feed is scoped to "activity on the browsed owner's repos," not "your activity across GitHub."** This is a deliberate scope decision (see "Data source decision" above) — a true cross-repo, cross-org "everything I've touched" feed would need the Notifications API or `GET /users/{user}/events` (the authenticated user's own combined event stream, which is a different endpoint from the per-repo one this plan uses) and raises new auth/privacy questions (whose feed is it — the browsed owner's, or the token holder's?). Flag this to Jacob before implementation: does "Feed" mean "what's happening in the repos I'm looking at" (this plan) or "what have I personally been doing everywhere" (different endpoint, different plan)?
3. **The `path` extraction heuristic (backtick-quoted token in the lead commit message) is lossy.** Many real commits don't reference a file in backticks, so many Feed items will legitimately have no `path` and clicking them lands on the tree root rather than a specific file. A more accurate implementation would call `GET /repos/{owner}/{repo}/commits/{sha}` per push event to get the real changed-files list, but that multiplies the already-bounded API fan-out (up to 10 repos × up to 30 events × 1 extra call each = up to 300 extra calls) and was deliberately left out of this first cut. If accurate file-linking turns out to matter more than this plan assumes, that's a follow-up, not a Task-9 blocker.
4. **Rate limits make the Feed marginal without a token.** Unauthenticated GitHub is 60 req/hr total, shared across every browse action the user makes in the whole session, not just the Feed. A single Feed load can consume up to 11 of those 60 (1 repo-list + up to 10 events calls) in one click. This plan does not add a "you should set a token" nudge UI, but Task 9's manual verification step should confirm the existing error messaging (`describe_error`'s rate-limit branch) surfaces clearly when this happens — if it reads as confusing in practice, a follow-up UI nudge (e.g. reusing the existing settings-panel GITHUB_TOKEN field with a contextual link) is worth considering separately.
5. **`toolTabs.ts`'s exclusion of `github` from `TILEABLE_KINDS` is unaffected by this plan and should stay that way.** The two-pane split built here is *internal* to the `GitHubView` component (tree vs. preview), not the *outer* multi-tool tiling system (github vs. terminal vs. files as whole panels). Do not be tempted to "fix" `TILEABLE_KINDS` as a side effect of this work — that's a materially different, larger change (making `github` self-contained enough to tile against `files`/`terminal`) that this plan explicitly does not attempt.
6. **Dependency-bump ("deps") detection is a heuristic** (`dependabot` actor prefix or a commit message starting with "Bump "), not a distinct GitHub event type — Renovate-authored bumps (actor `renovate[bot]`, commit messages like "chore(deps): update dependency X to vY") will currently misclassify as plain `push`. If Renovate is in active use on any repo this Feed browses (the project's own `renovate.json` suggests it might be), extend the heuristic in `github_feed.rs::normalize_push_event` to also match `renovate` actor prefixes and the `chore(deps):` commit convention before shipping, or explicitly scope this plan's `deps`-kind detection to Dependabot-only in user-facing copy. (Corrected from the first pass's `"dependency-bump"` kind name, which does not exist in the mock — the mock's kind is `deps`, labeled "Dependencies".)
7. **No pagination for the Feed.** Each repo's events call caps at `per_page=30` (Events API's own max is effectively bounded — GitHub only retains ~300 events or 90 days per repo, whichever is smaller, regardless of pagination), and the merged, capped total is 100 items. This matches "recent activity feed," not "complete history" — acceptable for the stated use case but worth confirming with Jacob if "complete history" was actually the intent.
8. **Two of the mock's nine `FEED_KIND` values (`comment`, `conflict`) have no data source in this plan.** `comment` would need `IssueCommentEvent`/`PullRequestReviewCommentEvent`/`CommitCommentEvent` wired into `normalize_event` (Task 3) — straightforward but not done in this pass. `conflict` has no GitHub Events API equivalent at all; it would require polling `GET /repos/{owner}/{repo}/pulls/{n}`'s `mergeable_state` per open PR, a materially different (poll-based, not event-based) mechanism. Both kinds are registered in `feedKindLabel()`/`feedKindIcon()` (Task 5) for forward-compatibility, but will never appear in a live Feed from this plan alone — flag this to Jacob/reviewers before claiming "Feed tab matches the mock," since two of nine row types are visually absent from real data.
9. **`badge`/`meta` per-kind fidelity is best-effort, not guaranteed to match the mock's fixture data.** The mock's `badge` (`{add, del}` line-diff or status label like "Approved"/"Latest") and `meta` (freeform descriptive line) fields were static fixture values in a mockup, not necessarily things a single Events API call can always populate. This plan's Task 3 normalizers set `badge: None` for merges/reviews/releases (the additions/deletions and "latest release" flags are not present on the Events API payloads themselves — they'd need an extra per-item API call, e.g. `GET /repos/{o}/{r}/pulls/{n}` or `GET /repos/{o}/{r}/releases/latest`) and only set a `badge` on issues (`"Closed"` when `action == "closed"`). If full mock-parity badges matter more than the added API-call cost, that's a follow-up scoping decision for Jacob, not something this plan's first cut resolves.
10. **The mock's "Ask about this file" button is not wired to a real ask-action dispatch in this plan.** The real mock calls `runOp(find('ask'), 'About ' + repo + '/' + sel)`, closing the GitHub view and switching to the ask action with a pre-filled prompt. This codebase's `GitHubView` has no callback prop today for triggering a *different* palette action from inside a structured result view (`OperationResultView.tsx` renders `<GitHubView payload={...} />` with no action-dispatch callback passed through). Task 6 ships the "Ask" button as a clipboard-copy affordance (copies `"About {repo}/{path}"` with a toast, same mechanism as "Copy contents"/"Open on GitHub") rather than silently dropping the button or silently claiming it's wired end-to-end. A follow-up task should add a proper `onAskAbout`-style callback threaded from `OperationResultView` down through `GitHubView` if full mock parity (auto-switching to the ask view) is wanted — that's a larger, cross-cutting change than this plan's stated scope.
11. **The mock's feed-row `time`/`day` fields are precomputed fixture strings; this plan derives them from a raw `timestamp_unix` instead.** The mock's static `FEED` array hardcodes `time: '11m'`/`day: 'Today'` per row because it has no real backing data source. This plan's `FeedItem.timestamp_unix` (Rust) / `timestampUnix` (TS) is a real Unix timestamp parsed from each event's `created_at`, with `groupFeedByDay()` (Task 5) and `formatRelativeTime()` (Task 7) computing the day-bucket and relative-time string client-side at render time. This is intentional and correct (a real feed can't ship precomputed "11m ago" strings that go stale), not a divergence to fix — noted here only so a reviewer comparing screenshots side-by-side understands why the two won't show literally identical time strings for the same underlying event.
