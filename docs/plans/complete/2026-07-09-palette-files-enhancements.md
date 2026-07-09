# Palette Files View Enhancements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `apps/palette-tauri`'s Files view up to the richer design intended for it — SFTP remote browsing, a two-pane split view, bulk multi-select ingest, and an AI-assisted edit-proposal/approval step — as one coherent body of work, sequenced so state additions don't collide.

**Architecture:** Extract Files-view state into a `useReducer` model (`src/lib/filesViewState.ts`) from Task 1 onward, rather than a flat `useState` block — this keeps `FilesView.tsx` a thin renderer as later tasks add pane, selection, AI-edit, and SFTP state. Add a new `src-tauri/src/sftp_bridge.rs` Rust module (parallel to `files_bridge.rs`) backed by `russh` + `russh-sftp`, gated behind a real host-key-verification implementation before any command is wired live. Connection profiles (host/user/key path, never key material or password) persist through the existing `PaletteSettings`/`settings.json` mechanism in `persistence.rs`, alongside a new dedicated TOFU host-key store.

**Tech Stack:** React 19 + TypeScript (frontend), Rust 2024 + Tauri 2 (backend), `russh`/`russh-sftp` (new — SSH/SFTP client), `uuid` (already a dependency), Vitest (`*.test.ts(x)` co-located), Rust sidecar `_tests.rs` convention.

## Global Constraints

- **Source-of-truth note on the mock (revised):** `palette-mock.html` is located at the axon repo root and has been read directly for this revision — the `filesView()` function's real behavior grounds every state-shape and interaction claim below. Specific identifiers cross-checked directly against the file: `s.filesSplit`, `s.filesPane`, `s.filesSel`/`s.filesSel2`, `s.filesTreeW`, `s.filesChecked`, `s.filesSparkle`/`s.filesSparkleQ`, `s.filesDiff`/`makeDiff()`, `s.sftpConnected`. Where this plan's design goes beyond what the mock shows (multi-profile SFTP, real bulk-ingest concurrency, a real diff engine), that is called out explicitly as new design work, not a mock port — see Task 4 and Task 5's scope notes.
- Follow `apps/palette-tauri/CLAUDE.md`: business logic in `src/lib/*`, components stay thin renderers over props; all backend calls go through `src/lib/invoke.ts`'s `invoke()` wrapper — never import `@tauri-apps/api/*` directly in component code.
- TS tests are co-located `*.test.ts(x)` siblings (NOT the Rust `_tests.rs` sidecar rule, which applies only to `src-tauri/` and root workspace crates).
- Rust tests use the sidecar convention: `foo.rs` + `#[cfg(test)] #[path = "foo_tests.rs"] mod tests;` + sibling `foo_tests.rs` using `use super::*;`. One sidecar per original test-mod block.
- Rust module layout: `mod_module_files = deny` — never `foo/mod.rs`; a module root lives in `foo.rs`, submodules in `foo/bar.rs`.
- Monolith policy: changed `.rs` files ≤ 500 lines, functions ≤ 120 lines hard fail (warn at 80). `**/*_tests.*` and `**/*.test.ts(x)` are exempt from line caps where documented.
- Design tokens: only Aurora `var(--aurora-*)` tokens / existing primitives in `src/components/ui/aurora/`; one-off layout goes in `src/styles.css` as semantic classes (e.g. `.files-*`), not new primitives.
- Never store SSH private key material or passwords in `settings.json` in plaintext if avoidable — prefer a path reference to a key file already on disk (`~/.ssh/id_ed25519`-style) plus an optional OS keychain lookup; if a passphrase/password must be persisted, flag it clearly as a risk (see Open Questions) rather than silently writing it to disk.
- `apps/palette-tauri/src-tauri/Cargo.toml` currently has **no** SSH/SFTP crate (grepped — absent). `uuid = { version = "1", features = ["v4"] }` **is already a dependency** — reuse it for SFTP connection ids rather than adding a new crate or hand-rolling an id scheme. Any new crate addition needs a version pin and must not break `cargo build`/`cargo clippy`/`cargo fmt --check` gates already run via `just verify`.
- Local filesystem commands (`files_list_dir`/`files_read_file`/`files_write_file`/`files_get_root`) are scoped to a single canonicalized root via `resolve_within_root`/`resolve_new_within_root` in `files_bridge.rs` — the SFTP path is a **parallel, separately-scoped** system, not an extension of local root-escape checks (a different threat model: network creds, remote paths, no local symlink concerns for *remote* paths). **Exception:** `private_key_path` (Task 5) is a **local** file used to authenticate — it must go through local-path rigor equivalent to `files_bridge.rs`'s root checks (canonicalize, reject symlinks or justify not doing so, verify regular file). Do not lump it in with "no root concept" reasoning, which only applies to the remote filesystem the key unlocks.

---

## Current State (from direct source read — ground truth, not the mock)

### Frontend: `apps/palette-tauri/src/components/palette/FilesView.tsx` (372 lines, re-verified this revision)

- Single-pane only. State: `cwd: string`, `listing: LoadState<DirListing>`, `selected: FileEntry | null`, `file: LoadState<FileContents>`, `editing: boolean`, `draft: string`, `saving: boolean`, `ingest: IngestState`.
- `loadDir(path)` calls `invoke<DirListing>("files_list_dir", { path })`.
- `openEntry(entry)`: if dir, navigates (`setCwd`); if file, sets `selected` and calls `loadFile(entry.path)`.
- `saveFile()`: calls `invoke<FileContents>("files_write_file", { path, content: draft })`, no diff/preview step — direct textarea → save. **This manual edit/save flow (pencil `Edit` button, `editing`/`draft`, Cancel/Save) stays exactly as-is; it is not touched by this plan.** The new AI-edit flow (Task 4) is a wholly separate, additional per-pane affordance.
- `ingestSelected()`: single-file only. Looks up the `embed` `RemotePaletteAction` from `ACTIONS`, builds `absolutePath = root + "/" + selected.path`, calls `executeAction(client, embedAction, absolutePath, config)` (one call, one file).
- Render: `.files-toolbar` (breadcrumb + refresh) → `.files-body` → `.files-tree` (row list, single `aria-selected`) + `.files-preview` (single `FilePreview`).
- No checkboxes, no split, no diff, no reducer — confirmed unchanged from the prior plan revision's read.

### Model: `apps/palette-tauri/src/lib/filesModel.ts` (122 lines, re-verified)

Pure helpers only — `FileEntry`/`DirListing`/`FileContents` types, `formatBytes`, `formatModified`, `breadcrumbSegments`/`joinSegments`/`parentPath`/`childPath`, `fileKind`/`extensionOf`/`isIngestable`/`isMarkdownLike`, `sortEntries`. No selection-set, pane, or reducer types exist yet.

### Rust: `apps/palette-tauri/src-tauri/src/files_bridge.rs` (300 lines, re-verified)

- `FileEntry { name, path, is_dir, size, modified_unix }`, `DirListing { path, root, entries }`, `FileContents { path, content, size }` — all `#[serde(rename_all = "camelCase")]`.
- `files_root(app)` resolves either a configured root (`<app-config>/files_root.txt`) or `$HOME`, always through `fs::canonicalize`.
- `resolve_within_root`/`resolve_new_within_root`: reject `..`/NUL, canonicalize, verify `starts_with(root)`. This is the **local** sandbox — SFTP's *remote* paths must not reuse or weaken it (no local canonicalization applies to a remote server's filesystem); `private_key_path` is the one SFTP-adjacent field that IS local and DOES need this rigor (see Task 5b).
- Four `#[tauri::command]`s: `files_list_dir`, `files_read_file` (5 MiB cap, `MAX_TEXT_FILE_BYTES`, UTF-8-only), `files_write_file` (atomic via `persistence::atomic_write`), `files_get_root`.
- Registered in `lib.rs` `invoke_handler![...]` (~line 479+) and declared via `mod files_bridge;`.
- Test sidecar already exists: `files_bridge_tests.rs` via `#[cfg(test)] #[path = "files_bridge_tests.rs"] mod tests;`.

### Precedent for a new "remote bridge" module: `github_bridge.rs` (re-verified)

- Holds a `pub(crate) struct GitHubClient(reqwest::Client)` built once in `run()` and registered via `.manage(github_client)` — Tauri's managed-state pattern for a long-lived client. `sftp_bridge.rs` should follow the same shape: a `SftpConnections` managed state (`tokio::sync::Mutex<HashMap<ConnectionId, Session>>` — see Task 5's Mutex-choice fix below, not a fresh SSH connection per command call.
- Reads credentials from `persistence::read_default_env_entries()` / `value_for()` (the user's `~/.axon/.env`) as one credential source — analogous pattern for where SFTP host/user config could optionally live, though this plan uses `PaletteSettings` (see below) as the primary store since SFTP profiles are palette-specific, not Axon-runtime config.

### Settings persistence: `apps/palette-tauri/src-tauri/src/lib.rs` + `persistence.rs` (re-verified)

- `PaletteSettings` (lib.rs, `struct PaletteSettings { server_url, token, shortcut, collection, result_limit, theme, hide_on_blur, open_results_inline, agent_bubbles, show_footer_hints, env_values, config_values }`) is the single settings struct, serialized to `settings.json` via `write_settings`/`read_settings_result` in `persistence.rs`. It already carries free-form `env_values`/`config_values` maps (for Axon env/config) — SFTP connection profiles should NOT go in those maps (different concern); add a dedicated `sftp_connections: Vec<SftpConnectionProfile>` field instead, following the same "add a field + wire `Default`" pattern documented in the root `CLAUDE.md`'s "Adding fields to `Config` struct" gotcha.
- `write_settings` clears `env_values`/`config_values` before serializing (those are file-sourced, not settings-file-sourced) — `sftp_connections` is a genuine settings-file field and must NOT be cleared the same way.
- `settings_path(app)` resolves to `<app_config_dir>/settings.json` via `app.path().app_config_dir()`. The new `sftp_known_hosts.json` (Task 5b) belongs in the same directory, written through the same `atomic_write` helper `files_bridge.rs` already uses.

### Cargo.toml (`apps/palette-tauri/src-tauri/Cargo.toml`, re-verified)

No SSH/SFTP crate present. Existing relevant deps: `tokio = { version = "1", features = ["sync", "net", "io-util", "time", "macros", "rt", "process"] }` (**not** `full` — any new async SSH crate needs its exact required tokio features cross-checked and added individually), `anyhow`, `serde`/`serde_json`, `uuid = { version = "1", features = ["v4"] }` (already present — reuse for connection ids).

### Ingest path (single-file today, re-verified)

`ingestSelected()` in `FilesView.tsx` is the only ingest call site in the frontend for this view. It resolves the `embed` `RemotePaletteAction` from `src/lib/actions.ts`'s `ACTIONS` array and calls `executeAction(client, embedAction, absolutePath, config)` from `src/lib/axonClient.ts` (one call per invocation — no batching primitive exists yet). Bulk ingest (Task 3) needs a loop over `executeAction` calls, not a new bulk API — the Axon HTTP `/v1/sources` embed endpoint is synchronous per the `axon-phase10-source-migration-gaps` memory note, so this plan queues N **sequential** calls (see Task 3's corrected concurrency), not a batch endpoint or a concurrency guess.

---

## File-by-File Breakdown

| File | Change |
|---|---|
| `apps/palette-tauri/src/lib/filesViewState.ts` | New — `useReducer` state model: `FilesViewState`, `FilesViewAction` discriminated union, `filesViewReducer`, `createInitialState()` (Task 1) |
| `apps/palette-tauri/src/lib/filesViewState.test.ts` | New co-located tests for the reducer (Task 1) |
| `apps/palette-tauri/src/lib/filesModel.ts` | Add `PaneId`, `FilesPane` type, `LoadState<T>` (lifted from the component), `SelectionSet`/`CheckedPaths` helpers (Task 1) |
| `apps/palette-tauri/src/lib/filesModel.test.ts` | New co-located tests for pane/selection helpers (Task 1) |
| `apps/palette-tauri/src/components/palette/FilesView.tsx` | Refactor to dispatch against `filesViewReducer`, add split UI + resizable tree-width divider (Task 2), add checkbox column + sticky bulk bar (Task 3), add AI-edit sparkle/diff/approve flow (Task 4) |
| `apps/palette-tauri/src/components/palette/FilesView.test.tsx` | Extend existing 232-line test file per task |
| `apps/palette-tauri/src/components/palette/SftpConnectionDialog.tsx` | New — add/edit connection profile form (Task 5) |
| `apps/palette-tauri/src/components/palette/SftpConnectionDialog.test.tsx` | New co-located tests (Task 5) |
| `apps/palette-tauri/src/components/palette/SftpTrustPrompt.tsx` | New — host-key TOFU trust prompt shown on first connect to an unpinned host (Task 5b) |
| `apps/palette-tauri/src/components/palette/SftpTrustPrompt.test.tsx` | New co-located tests (Task 5b) |
| `apps/palette-tauri/src/lib/sftpModel.ts` | New — `SftpConnectionProfile`, `SftpEntry`, `SftpKnownHostEntry` types + helpers (Task 5) |
| `apps/palette-tauri/src/lib/sftpModel.test.ts` | New co-located tests (Task 5) |
| `apps/palette-tauri/src/lib/aiEditModel.ts` | New — `AiEditProposal` type + pure helpers for the AI-edit flow (Task 4) |
| `apps/palette-tauri/src/lib/aiEditModel.test.ts` | New co-located tests |
| `apps/palette-tauri/src/styles.css` | Add `.files-split*`, `.files-checkbox*`, `.files-bulk-bar*`, `.files-ai-edit*`, `.sftp-*` classes |
| `apps/palette-tauri/src-tauri/src/sftp_bridge.rs` | New — SSH/SFTP client bridge (Task 5a); commands only registered live after Task 5b's host-key verification lands |
| `apps/palette-tauri/src-tauri/src/sftp_bridge_tests.rs` | New Rust test sidecar, including the always-accept-hostkey regression test |
| `apps/palette-tauri/src-tauri/src/sftp_known_hosts.rs` | New — TOFU fingerprint store: load/save/verify against `sftp_known_hosts.json` (Task 5b) |
| `apps/palette-tauri/src-tauri/src/sftp_known_hosts_tests.rs` | New Rust test sidecar (Task 5b) |
| `apps/palette-tauri/src-tauri/src/lib.rs` | Register `mod sftp_bridge;`, `mod sftp_known_hosts;`, SFTP commands in `invoke_handler!` (deferred to Task 5b), extend `PaletteSettings` with `sftp_connections` |
| `apps/palette-tauri/src-tauri/src/persistence.rs` | Wire `sftp_connections` field through settings load/save/default paths (excluded from the `env_values`/`config_values`-style clearing) |
| `apps/palette-tauri/src-tauri/Cargo.toml` | Add `russh`, `russh-sftp`, and any transitive crypto/key-parsing crates pinned to specific versions |

---

## Sequencing Rationale

1. **Task 1 (reducer + pane/selection data model)** must come first — Task 2 (split-pane), Task 3 (bulk-select), and Task 4 (AI-edit) all read/write the same underlying pane/selection/edit state. Building the reducer once, rather than a flat `useState` block, means Task 5's SFTP state additions become new reducer actions instead of patches to five different call sites.
2. **Task 2 (split-pane)** next — it's the highest-value, purely-local (no new backend) capability, and establishes the multi-pane rendering shell that Task 3's checkboxes and Task 4's AI-edit UI both render inside.
3. **Task 3 (bulk-select + ingest)** builds on the pane shell — the sticky bulk bar sits above the tree column regardless of 1 or 2 panes open.
4. **Task 4 (AI-edit proposal/approval)** is scoped after the pane shell exists, since the sparkle button and diff/approve footer are per-pane UI. This supersedes the previous revision's "manual edit review" framing entirely — see Task 4's ground-truth note.
5. **Task 5 (SFTP)** is the largest, most isolated gap (new Rust crate, new network/credential surface, new remote-aware data path) and is intentionally sequenced last/separately — it does not block or get blocked by 1–4, but its tree-injection UI is designed to slot into the same pane-opening flow Task 2 builds, so it comes after that shell exists. It could in principle be extracted to a parallel workstream if two engineers are available, but is written here as a single sequential track for one implementer.

---

## Task 1: Reducer + Pane/Selection Data Model

**Files:**
- Create: `apps/palette-tauri/src/lib/filesViewState.ts`
- Create: `apps/palette-tauri/src/lib/filesViewState.test.ts`
- Modify: `apps/palette-tauri/src/lib/filesModel.ts`
- Create: `apps/palette-tauri/src/lib/filesModel.test.ts` (co-located; none exists today — confirm no existing file before creating, and if one exists, extend it instead of overwriting)

**Interfaces:**
- Produces (`filesModel.ts`): `export type LoadState<T> = { kind: "idle" } | { kind: "loading" } | { kind: "loaded"; value: T } | { kind: "error"; message: string }` (lifted here from `FilesView.tsx` so pane state can reuse it without a component→component import); `type PaneId = "left" | "right"`; `interface FilesPane { id: PaneId; cwd: string; selected: FileEntry | null; file: LoadState<FileContents>; loadGen: number; editing: boolean; draft: string; saving: boolean }` (`loadGen` is the request-generation counter — see the async-race fix below); `createPane(id: PaneId, cwd?: string): FilesPane`; `type CheckedPaths = ReadonlySet<string>` plus pure helpers `toggleChecked`, `checkAllIn`, `clearChecked()`, `isChecked`.
- Produces (`filesViewState.ts`): `interface FilesViewState { panes: [FilesPane] | [FilesPane, FilesPane]; activePane: PaneId; listings: Record<PaneId, LoadState<DirListing>>; checked: CheckedPaths; bulkIngest: BulkIngestState; sftp: SftpUiState }` (the `sftp` field is a stub in this task — Task 5 fills in its shape; declaring it now means Task 5 adds actions/cases, not a new top-level state slice); a discriminated `type FilesViewAction = | { type: "pane/setCwd"; pane: PaneId; cwd: string } | { type: "pane/listingLoading"; pane: PaneId } | { type: "pane/listingLoaded"; pane: PaneId; listing: DirListing } | { type: "pane/listingError"; pane: PaneId; message: string } | { type: "pane/select"; pane: PaneId; entry: FileEntry | null } | { type: "pane/fileLoading"; pane: PaneId; loadGen: number } | { type: "pane/fileLoaded"; pane: PaneId; loadGen: number; file: FileContents } | { type: "pane/fileError"; pane: PaneId; loadGen: number; message: string } | { type: "pane/setEditing"; pane: PaneId; editing: boolean } | { type: "pane/setDraft"; pane: PaneId; draft: string } | { type: "pane/setSaving"; pane: PaneId; saving: boolean } | { type: "split/open" } | { type: "split/close" } | { type: "pane/setActive"; pane: PaneId } | { type: "treeWidth/set"; width: number } | { type: "checked/toggle"; path: string } | { type: "checked/checkAll"; paths: string[] } | { type: "checked/clear" }` (Task 3/4/5 append more `type` members to this union in their own tasks — this task defines the shape and the pane/split/checked cases only); `filesViewReducer(state: FilesViewState, action: FilesViewAction): FilesViewState`; `createInitialState(): FilesViewState`.
- Consumes: nothing new — pure additions.

**Why a reducer, not a flat `useState` block:** by Task 4, `FilesView.tsx` would otherwise own multi-pane state, split-drag physics, per-pane load/save/edit lifecycles, a global checked-set, bulk-ingest progress, AND AI-edit-diff state as five-plus independent `useState` calls; Task 5 would then bolt SFTP connection/dialog state on top of that too. That violates `apps/palette-tauri/CLAUDE.md`'s "components are thin renderers over props" convention and means every later task patches `updatePane`-style call sites scattered through the component. A single `useReducer` with a discriminated action type keeps all state transitions in one testable, pane-agnostic function; `FilesView.tsx` only calls `dispatch(...)` and never mutates state shape directly.

- [ ] **Step 1: Write failing tests for pane/selection helpers in `filesModel.ts`**

```typescript
// apps/palette-tauri/src/lib/filesModel.test.ts
import { describe, expect, it } from "vitest";
import {
  checkAllIn,
  clearChecked,
  createPane,
  isChecked,
  toggleChecked,
  type CheckedPaths,
} from "./filesModel";

describe("createPane", () => {
  it("creates an idle pane with the given id and cwd", () => {
    const pane = createPane("left", "docs");
    expect(pane).toEqual({
      id: "left",
      cwd: "docs",
      selected: null,
      file: { kind: "idle" },
      loadGen: 0,
      editing: false,
      draft: "",
      saving: false,
    });
  });

  it("defaults cwd to empty string", () => {
    const pane = createPane("right");
    expect(pane.cwd).toBe("");
  });
});

describe("checked-path set helpers", () => {
  it("toggleChecked adds an unchecked path", () => {
    const empty: CheckedPaths = new Set();
    const next = toggleChecked(empty, "a.md");
    expect(isChecked(next, "a.md")).toBe(true);
    expect(next.size).toBe(1);
  });

  it("toggleChecked removes an already-checked path", () => {
    const start: CheckedPaths = new Set(["a.md"]);
    const next = toggleChecked(start, "a.md");
    expect(isChecked(next, "a.md")).toBe(false);
    expect(next.size).toBe(0);
  });

  it("toggleChecked does not mutate the input set", () => {
    const start: CheckedPaths = new Set(["a.md"]);
    toggleChecked(start, "b.md");
    expect(start.size).toBe(1);
  });

  it("checkAllIn adds every given path, preserving existing checks", () => {
    const start: CheckedPaths = new Set(["a.md"]);
    const next = checkAllIn(start, ["b.md", "c.md"]);
    expect(next.size).toBe(3);
    expect(isChecked(next, "a.md")).toBe(true);
    expect(isChecked(next, "b.md")).toBe(true);
    expect(isChecked(next, "c.md")).toBe(true);
  });

  it("clearChecked returns an empty set", () => {
    const next = clearChecked();
    expect(next.size).toBe(0);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/filesModel.test.ts`
Expected: FAIL — `createPane`, `toggleChecked`, `checkAllIn`, `clearChecked`, `isChecked` are not exported.

- [ ] **Step 3: Implement the pane and selection helpers in `filesModel.ts`**

Add to `apps/palette-tauri/src/lib/filesModel.ts` (after the existing `sortEntries` export):

```typescript
/** Shared async-load state shape for a single fetched value (dir listing or
 * file contents). Lifted here from FilesView.tsx so pane state can reuse it
 * without a component→component import. */
export type LoadState<T> =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "loaded"; value: T }
  | { kind: "error"; message: string };

export type PaneId = "left" | "right";

/** One open file-browsing pane: its own cwd, selection, loaded file, and edit
 * state. Two panes (left/right) enable the split view; single-pane mode is
 * just "only the left pane is rendered."
 *
 * `loadGen` guards against out-of-order async resolution: every
 * loadDir/loadFile dispatch increments it, and a resolved fetch is only
 * applied if its captured generation still matches the pane's current one —
 * otherwise a slower, superseded request is silently dropped instead of
 * overwriting newer content. See filesViewState.ts's fileLoaded/fileError
 * reducer cases. */
export interface FilesPane {
  id: PaneId;
  cwd: string;
  selected: FileEntry | null;
  file: LoadState<FileContents>;
  loadGen: number;
  editing: boolean;
  draft: string;
  saving: boolean;
}

export function createPane(id: PaneId, cwd = ""): FilesPane {
  return {
    id,
    cwd,
    selected: null,
    file: { kind: "idle" },
    loadGen: 0,
    editing: false,
    draft: "",
    saving: false,
  };
}

/** Set of root-relative paths currently checked for bulk actions. Kept as a
 * plain `ReadonlySet<string>` (not a class) so it composes with the reducer's
 * state shape without extra wrapper methods; helpers below return new sets
 * (never mutate) to keep reducer updates predictable. */
export type CheckedPaths = ReadonlySet<string>;

export function toggleChecked(set: CheckedPaths, path: string): CheckedPaths {
  const next = new Set(set);
  if (next.has(path)) {
    next.delete(path);
  } else {
    next.add(path);
  }
  return next;
}

export function checkAllIn(set: CheckedPaths, paths: string[]): CheckedPaths {
  const next = new Set(set);
  for (const path of paths) next.add(path);
  return next;
}

export function clearChecked(): CheckedPaths {
  return new Set();
}

export function isChecked(set: CheckedPaths, path: string): boolean {
  return set.has(path);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/filesModel.test.ts`
Expected: PASS (7 tests)

- [ ] **Step 5: Write failing tests for the reducer**

```typescript
// apps/palette-tauri/src/lib/filesViewState.test.ts
import { describe, expect, it } from "vitest";
import { createInitialState, filesViewReducer, type FilesViewState } from "./filesViewState";
import { clearChecked, createPane } from "./filesModel";

describe("createInitialState", () => {
  it("starts with a single left pane, no split, tree width 248", () => {
    const state = createInitialState();
    expect(state.panes).toEqual([createPane("left")]);
    expect(state.activePane).toBe("left");
    expect(state.treeWidth).toBe(248);
    expect(state.checked).toEqual(clearChecked());
  });
});

describe("filesViewReducer — pane lifecycle", () => {
  it("pane/setCwd updates only the targeted pane's cwd", () => {
    const state = createInitialState();
    const next = filesViewReducer(state, { type: "pane/setCwd", pane: "left", cwd: "docs" });
    expect(next.panes[0].cwd).toBe("docs");
  });

  it("pane/select sets the pane's selected entry", () => {
    const state = createInitialState();
    const entry = { name: "a.md", path: "a.md", isDir: false, size: 10 };
    const next = filesViewReducer(state, { type: "pane/select", pane: "left", entry });
    expect(next.panes[0].selected).toEqual(entry);
  });

  it("pane/fileLoading increments loadGen and sets loading state", () => {
    const state = createInitialState();
    const next = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 1 });
    expect(next.panes[0].loadGen).toBe(1);
    expect(next.panes[0].file).toEqual({ kind: "loading" });
  });

  it("pane/fileLoaded is dropped when loadGen is stale", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 1 });
    state = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 2 });
    // A slow resolution for generation 1 arrives after generation 2 already started.
    const stale = filesViewReducer(state, {
      type: "pane/fileLoaded",
      pane: "left",
      loadGen: 1,
      file: { path: "old.md", content: "stale", size: 5 },
    });
    expect(stale.panes[0].file).toEqual({ kind: "loading" });
    expect(stale.panes[0].loadGen).toBe(2);
  });

  it("pane/fileLoaded applies when loadGen matches", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "pane/fileLoading", pane: "left", loadGen: 1 });
    const applied = filesViewReducer(state, {
      type: "pane/fileLoaded",
      pane: "left",
      loadGen: 1,
      file: { path: "a.md", content: "fresh", size: 5 },
    });
    expect(applied.panes[0].file).toEqual({
      kind: "loaded",
      value: { path: "a.md", content: "fresh", size: 5 },
    });
    expect(applied.panes[0].draft).toBe("fresh");
  });
});

describe("filesViewReducer — split view", () => {
  it("split/open adds a right pane seeded with the left pane's cwd", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "pane/setCwd", pane: "left", cwd: "docs" });
    const next = filesViewReducer(state, { type: "split/open" });
    expect(next.panes).toHaveLength(2);
    expect(next.panes[1].id).toBe("right");
    expect(next.panes[1].cwd).toBe("docs");
  });

  it("split/open is idempotent when already split", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "split/open" });
    const next = filesViewReducer(state, { type: "split/open" });
    expect(next.panes).toHaveLength(2);
  });

  it("split/close drops the right pane and resets active pane to left", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "split/open" });
    state = filesViewReducer(state, { type: "pane/setActive", pane: "right" });
    const next = filesViewReducer(state, { type: "split/close" });
    expect(next.panes).toHaveLength(1);
    expect(next.activePane).toBe("left");
  });

  it("pane/setActive only applies when split is open", () => {
    const state = createInitialState();
    const next = filesViewReducer(state, { type: "pane/setActive", pane: "right" });
    expect(next.activePane).toBe("left");
  });
});

describe("filesViewReducer — tree width", () => {
  it("treeWidth/set clamps to [180, 460]", () => {
    const state = createInitialState();
    expect(filesViewReducer(state, { type: "treeWidth/set", width: 50 }).treeWidth).toBe(180);
    expect(filesViewReducer(state, { type: "treeWidth/set", width: 900 }).treeWidth).toBe(460);
    expect(filesViewReducer(state, { type: "treeWidth/set", width: 300 }).treeWidth).toBe(300);
  });
});

describe("filesViewReducer — bulk checked set", () => {
  it("checked/toggle and checked/clear route through the shared helpers", () => {
    let state = createInitialState();
    state = filesViewReducer(state, { type: "checked/toggle", path: "a.md" });
    expect(state.checked.has("a.md")).toBe(true);
    state = filesViewReducer(state, { type: "checked/clear" });
    expect(state.checked.size).toBe(0);
  });
});
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/filesViewState.test.ts`
Expected: FAIL — module `./filesViewState` does not exist.

- [ ] **Step 7: Implement `filesViewState.ts`**

```typescript
// apps/palette-tauri/src/lib/filesViewState.ts
// useReducer-based state model for FilesView.tsx. Centralizing all Files-view
// state transitions here (rather than a flat useState block in the
// component) keeps FilesView.tsx a thin renderer over `state`/`dispatch` as
// later tasks add split-pane, bulk-select, AI-edit, and SFTP state — each
// adds new `FilesViewAction` members and reducer cases here instead of new
// `useState` calls and prop-drilled setters in the component.

import {
  checkAllIn,
  clearChecked,
  createPane,
  isChecked,
  toggleChecked,
  type CheckedPaths,
  type DirListing,
  type FileContents,
  type FileEntry,
  type FilesPane,
  type LoadState,
  type PaneId,
} from "./filesModel";

const MIN_TREE_WIDTH = 180;
const MAX_TREE_WIDTH = 460;
const DEFAULT_TREE_WIDTH = 248;

export interface FilesViewState {
  panes: [FilesPane] | [FilesPane, FilesPane];
  activePane: PaneId;
  listings: Record<PaneId, LoadState<DirListing>>;
  treeWidth: number;
  checked: CheckedPaths;
}

export function createInitialState(): FilesViewState {
  return {
    panes: [createPane("left")],
    activePane: "left",
    listings: { left: { kind: "idle" }, right: { kind: "idle" } },
    treeWidth: DEFAULT_TREE_WIDTH,
    checked: clearChecked(),
  };
}

export type FilesViewAction =
  | { type: "pane/setCwd"; pane: PaneId; cwd: string }
  | { type: "pane/listingLoading"; pane: PaneId }
  | { type: "pane/listingLoaded"; pane: PaneId; listing: DirListing }
  | { type: "pane/listingError"; pane: PaneId; message: string }
  | { type: "pane/select"; pane: PaneId; entry: FileEntry | null }
  | { type: "pane/fileLoading"; pane: PaneId; loadGen: number }
  | { type: "pane/fileLoaded"; pane: PaneId; loadGen: number; file: FileContents }
  | { type: "pane/fileError"; pane: PaneId; loadGen: number; message: string }
  | { type: "pane/setEditing"; pane: PaneId; editing: boolean }
  | { type: "pane/setDraft"; pane: PaneId; draft: string }
  | { type: "pane/setSaving"; pane: PaneId; saving: boolean }
  | { type: "split/open" }
  | { type: "split/close" }
  | { type: "pane/setActive"; pane: PaneId }
  | { type: "treeWidth/set"; width: number }
  | { type: "checked/toggle"; path: string }
  | { type: "checked/checkAll"; paths: string[] }
  | { type: "checked/clear" };

function updatePane(
  panes: FilesViewState["panes"],
  id: PaneId,
  patch: Partial<FilesPane>,
): FilesViewState["panes"] {
  const mapped = panes.map((pane) => (pane.id === id ? { ...pane, ...patch } : pane));
  return mapped as FilesViewState["panes"];
}

function findPane(panes: FilesViewState["panes"], id: PaneId): FilesPane | undefined {
  return panes.find((pane) => pane.id === id);
}

export function filesViewReducer(state: FilesViewState, action: FilesViewAction): FilesViewState {
  switch (action.type) {
    case "pane/setCwd":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          cwd: action.cwd,
          selected: null,
          file: { kind: "idle" },
        }),
      };
    case "pane/listingLoading":
      return { ...state, listings: { ...state.listings, [action.pane]: { kind: "loading" } } };
    case "pane/listingLoaded":
      return {
        ...state,
        listings: { ...state.listings, [action.pane]: { kind: "loaded", value: action.listing } },
      };
    case "pane/listingError":
      return {
        ...state,
        listings: {
          ...state.listings,
          [action.pane]: { kind: "error", message: action.message },
        },
      };
    case "pane/select":
      return { ...state, panes: updatePane(state.panes, action.pane, { selected: action.entry }) };
    case "pane/fileLoading":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          file: { kind: "loading" },
          loadGen: action.loadGen,
          editing: false,
        }),
      };
    case "pane/fileLoaded": {
      const pane = findPane(state.panes, action.pane);
      // Drop stale resolutions: only apply if this is still the pane's
      // current in-flight generation. A superseded (older) loadGen means a
      // newer loadDir/loadFile dispatch has already started — applying this
      // result would overwrite the newer request's eventual outcome.
      if (!pane || pane.loadGen !== action.loadGen) return state;
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          file: { kind: "loaded", value: action.file },
          draft: action.file.content,
        }),
      };
    }
    case "pane/fileError": {
      const pane = findPane(state.panes, action.pane);
      if (!pane || pane.loadGen !== action.loadGen) return state;
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          file: { kind: "error", message: action.message },
        }),
      };
    }
    case "pane/setEditing":
      return { ...state, panes: updatePane(state.panes, action.pane, { editing: action.editing }) };
    case "pane/setDraft":
      return { ...state, panes: updatePane(state.panes, action.pane, { draft: action.draft }) };
    case "pane/setSaving":
      return { ...state, panes: updatePane(state.panes, action.pane, { saving: action.saving }) };
    case "split/open": {
      if (state.panes.length === 2) return state;
      const left = state.panes[0];
      return { ...state, panes: [left, createPane("right", left.cwd)] };
    }
    case "split/close":
      return { ...state, panes: [state.panes[0]], activePane: "left" };
    case "pane/setActive":
      if (state.panes.length < 2) return state;
      return { ...state, activePane: action.pane };
    case "treeWidth/set":
      return { ...state, treeWidth: Math.max(MIN_TREE_WIDTH, Math.min(MAX_TREE_WIDTH, action.width)) };
    case "checked/toggle":
      return { ...state, checked: toggleChecked(state.checked, action.path) };
    case "checked/checkAll":
      return { ...state, checked: checkAllIn(state.checked, action.paths) };
    case "checked/clear":
      return { ...state, checked: clearChecked() };
    default:
      return state;
  }
}

export { isChecked };
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/filesViewState.test.ts src/lib/filesModel.test.ts`
Expected: PASS (all reducer + pane/selection tests)

- [ ] **Step 9: Commit**

```bash
cd apps/palette-tauri
git add src/lib/filesModel.ts src/lib/filesModel.test.ts src/lib/filesViewState.ts src/lib/filesViewState.test.ts
git commit -m "feat(palette): add Files view reducer and pane/selection data model"
```

---

## Task 2: Split-Pane View (2 files side by side)

**Files:**
- Modify: `apps/palette-tauri/src/components/palette/FilesView.tsx`
- Modify: `apps/palette-tauri/src/components/palette/FilesView.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css`

**Ground-truth correction (re-verified directly against `palette-mock.html`'s `filesView()`):** the mock's resizable drag handle (`treeW`/`startResize`) resizes the **local file-tree column width**, clamped `[180, 460]`, default `248` — it sits between the tree column and the viewer pane(s). There is **no** drag-resize between the two viewer panes themselves; once split, `viewer` and `viewer2` render as direct flex siblings with no divider and no `fr`-ratio split between them. The split toggle in the mock is a small square icon button (`tBtn`, title/`aria-label` `"Split view"`/`"Close split"`) in the toolbar, next to a separate SFTP connect/disconnect icon button — not a text `Button` component. Clicking inside a pane (`onMouseDown`) sets it active (`filesPane`); only the active pane's file clicks route to that pane's own selection field (mock: `filesSel`/`filesSel2`). This task corrects the previous plan revision, which incorrectly modeled the resize handle as a pane-to-pane divider.

**Interfaces:**
- Consumes: `FilesPane`, `PaneId`, `LoadState<T>`, `createPane` from Task 1's `filesModel.ts`; `FilesViewState`, `FilesViewAction`, `filesViewReducer`, `createInitialState` from Task 1's `filesViewState.ts`.
- Produces: `FilesView` wires `const [state, dispatch] = useReducer(filesViewReducer, undefined, createInitialState)`; a toolbar icon-button pair (Split/Close split, SFTP placeholder stub for Task 5) using the existing icon-button convention (see Step 3 below — confirmed by reading the current toolbar's `Button variant="plain" size="unstyled"` icon-only usage, e.g. the existing Refresh button); a `.files-split-container` with a resizable tree column (`state.treeWidth`) and 1 or 2 fixed `flex:1` viewer panes. Later tasks (3, 4, 5) dispatch further actions against this same reducer and must not reintroduce parallel `useState` fields.

- [ ] **Step 1: Write failing tests for split-pane behavior**

Add to `apps/palette-tauri/src/components/palette/FilesView.test.tsx` (follow the existing file's mock/setup pattern — it already mocks `invoke` per the 232-line existing test file; reuse its `mockInvoke`/render helpers rather than duplicating setup):

```typescript
// Added to FilesView.test.tsx — assumes existing top-of-file mocks for
// `invoke` and `isTauriRuntime` are already in place (see current file).

describe("split view", () => {
  it("renders a single pane by default", () => {
    render(<FilesView client={null} config={null} />);
    expect(screen.queryAllByRole("listbox", { name: /directory entries/i })).toHaveLength(1);
  });

  it("shows a 'Split view' icon control that opens a second pane", async () => {
    render(<FilesView client={null} config={null} />);
    const splitButton = screen.getByRole("button", { name: /split view/i });
    await userEvent.click(splitButton);
    expect(screen.getAllByRole("listbox", { name: /directory entries/i })).toHaveLength(2);
  });

  it("closes the second pane when 'Close split' is clicked", async () => {
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getByRole("button", { name: /split view/i }));
    const closeSplit = screen.getByRole("button", { name: /close split/i });
    await userEvent.click(closeSplit);
    expect(screen.getAllByRole("listbox", { name: /directory entries/i })).toHaveLength(1);
  });

  it("renders a resize handle for the tree column only when split is open", async () => {
    // The tree column is resizable even in single-pane mode in the mock (its
    // width persists across split/close), but this plan only requires the
    // handle to be present — its exact idle-vs-split visibility is a small
    // implementation choice; assert its accessible name and drag behavior,
    // not conditional presence in single-pane mode.
    render(<FilesView client={null} config={null} />);
    expect(screen.getByRole("separator", { name: /resize file tree/i })).toBeInTheDocument();
  });

  it("dragging the tree-resize handle updates the tree column width", () => {
    render(<FilesView client={null} config={null} />);
    const handle = screen.getByRole("separator", { name: /resize file tree/i });
    const tree = screen.getAllByRole("listbox", { name: /directory entries/i })[0];
    fireEvent.mouseDown(handle, { clientX: 248 });
    fireEvent.mouseMove(window, { clientX: 300 });
    fireEvent.mouseUp(window);
    expect(tree).toHaveStyle({ width: "300px" });
  });

  it("each pane tracks its own selected file independently", async () => {
    // With two panes open, selecting a file in pane "left" must not affect
    // pane "right"'s selection. This exercises the per-pane state split
    // rather than a single shared `selected`.
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getByRole("button", { name: /split view/i }));
    const [leftTree, rightTree] = screen.getAllByRole("listbox", { name: /directory entries/i });
    const leftEntry = within(leftTree).getByText("readme.md");
    await userEvent.click(leftEntry);
    const rightPreview = rightTree.closest(".files-body")?.querySelector(".files-preview");
    expect(rightPreview).toHaveTextContent(/select a file/i);
  });

  it("clicking inside the right pane makes it active", async () => {
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getByRole("button", { name: /split view/i }));
    const [, rightTree] = screen.getAllByRole("listbox", { name: /directory entries/i });
    await userEvent.click(rightTree);
    const rightEntry = within(rightTree).getByText("readme.md");
    await userEvent.click(rightEntry);
    const [leftPreview] = screen
      .getAllByRole("listbox", { name: /directory entries/i })
      .map((tree) => tree.closest(".files-body")?.querySelector(".files-preview"));
    expect(leftPreview).toHaveTextContent(/select a file/i);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/FilesView.test.tsx`
Expected: FAIL — no "Split view" button exists yet, `queryAllByRole("listbox", ...)` returns 1 unconditionally, no `role="separator"` handle.

- [ ] **Step 3: Refactor `FilesView.tsx` to dispatch against the reducer**

Replace the single-pane `useState` block (current lines 58–65) with:

```typescript
import { useCallback, useEffect, useReducer, useRef } from "react";
import {
  createInitialState,
  filesViewReducer,
  type FilesViewAction,
} from "@/lib/filesViewState";
import type { PaneId } from "@/lib/filesModel";

// ... inside FilesView(...):
const [state, dispatch] = useReducer(filesViewReducer, undefined, createInitialState);
const loadGenRef = useRef<Record<PaneId, number>>({ left: 0, right: 0 });
const [ingest, setIngest] = useState<IngestState>({ kind: "idle" });
const splitOpen = state.panes.length === 2;
```

Introduce pane-scoped dispatch-driving effects/functions, replacing the old bare `loadDir`/`loadFile`/`openEntry`/`goToBreadcrumb`/`saveFile`:

```typescript
const loadDir = useCallback((id: PaneId, path: string) => {
  dispatch({ type: "pane/listingLoading", pane: id });
  invoke<DirListing>("files_list_dir", { path: path || null })
    .then((value) => dispatch({ type: "pane/listingLoaded", pane: id, listing: value }))
    .catch((err) => dispatch({ type: "pane/listingError", pane: id, message: errorMessage(err) }));
}, []);

const loadFile = useCallback((id: PaneId, path: string) => {
  const gen = loadGenRef.current[id] + 1;
  loadGenRef.current[id] = gen;
  dispatch({ type: "pane/fileLoading", pane: id, loadGen: gen });
  invoke<FileContents>("files_read_file", { path })
    .then((value) => dispatch({ type: "pane/fileLoaded", pane: id, loadGen: gen, file: value }))
    .catch((err) =>
      dispatch({ type: "pane/fileError", pane: id, loadGen: gen, message: errorMessage(err) }),
    );
}, []);

function openEntry(id: PaneId, entry: FileEntry) {
  if (entry.isDir) {
    dispatch({ type: "pane/setCwd", pane: id, cwd: entry.path });
    return;
  }
  dispatch({ type: "pane/select", pane: id, entry });
  loadFile(id, entry.path);
}

function activatePane(id: PaneId) {
  if (splitOpen) dispatch({ type: "pane/setActive", pane: id });
}
```

Add a `useEffect` per pane to load its directory on `cwd` change:

```typescript
useEffect(() => {
  if (!isTauriRuntime) return;
  for (const pane of state.panes) {
    loadDir(pane.id, pane.cwd);
  }
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, [state.panes.map((p) => `${p.id}:${p.cwd}`).join("|"), loadDir]);
```

Update `saveFile` to take a `PaneId` and dispatch pane-scoped actions:

```typescript
async function saveFile(id: PaneId) {
  const pane = state.panes.find((p) => p.id === id);
  if (!pane || !pane.selected) return;
  dispatch({ type: "pane/setSaving", pane: id, saving: true });
  try {
    const saved = await invoke<FileContents>("files_write_file", {
      path: pane.selected.path,
      content: pane.draft,
    });
    dispatch({
      type: "pane/fileLoaded",
      pane: id,
      loadGen: loadGenRef.current[id],
      file: saved,
    });
    dispatch({ type: "pane/setEditing", pane: id, editing: false });
  } catch (err) {
    dispatch({ type: "pane/fileError", pane: id, loadGen: loadGenRef.current[id], message: errorMessage(err) });
  } finally {
    dispatch({ type: "pane/setSaving", pane: id, saving: false });
  }
}
```

Render: wrap the existing `.files-body` (tree + preview) markup in a `renderPane(pane: FilesPane)` function reused for both panes, add a toolbar icon-button pair matching the existing icon-button convention (the current Refresh button uses `Button variant="plain" size="unstyled"` with `title`/`aria-label`, not a bespoke `tBtn`-equivalent — reuse that same pattern here rather than inventing a new icon-button primitive per the Aurora "no second button form" rule):

```tsx
<div className="files-toolbar">
  {/* existing breadcrumb for state.panes[0] unchanged */}
  <Button
    variant="plain"
    size="unstyled"
    type="button"
    title={splitOpen ? "Close split" : "Split view"}
    aria-label={splitOpen ? "Close split" : "Split view"}
    onClick={() => dispatch({ type: splitOpen ? "split/close" : "split/open" })}
  >
    <Columns2 size={14} />
  </Button>
  <Button
    variant="plain"
    size="unstyled"
    type="button"
    onClick={() => loadDir(state.activePane, state.panes.find((p) => p.id === state.activePane)!.cwd)}
    title="Refresh"
    aria-label="Refresh directory listing"
  >
    <RefreshCw size={14} />
  </Button>
</div>
<div className="files-split-container">
  <div
    className="files-tree-column aurora-scrollbar"
    style={{ width: state.treeWidth, flex: `0 0 ${state.treeWidth}px` }}
  >
    {renderTree()}
  </div>
  <div
    className="files-tree-resize"
    role="separator"
    aria-label="Resize file tree"
    aria-orientation="vertical"
    onMouseDown={startResize}
  />
  {renderPane(state.panes[0])}
  {splitOpen && renderPane(state.panes[1])}
</div>
```

Implement `startResize` as a mousedown→mousemove→mouseup drag handler that computes `treeWidth` from cursor-X delta and dispatches `treeWidth/set` (which itself clamps `[180, 460]`), with cleanup that survives the drag ending outside the webview:

```typescript
function startResize(event: React.MouseEvent<HTMLDivElement>) {
  event.preventDefault();
  const startX = event.clientX;
  const startWidth = state.treeWidth;
  const handle = event.currentTarget;

  function onMove(moveEvent: MouseEvent) {
    dispatch({ type: "treeWidth/set", width: startWidth + (moveEvent.clientX - startX) });
  }
  function stop() {
    window.removeEventListener("mousemove", onMove);
    window.removeEventListener("mouseup", stop);
    window.removeEventListener("blur", stop);
    handle.removeEventListener("mouseleave", stopIfNotDragging);
  }
  // mouseup normally fires the cleanup, but a drag that ends outside the
  // webview (e.g. releasing the mouse button over the OS window chrome)
  // never delivers a mouseup event to this document at all. `blur` (the
  // window losing focus mid-drag) is the reliable fallback signal for that
  // case; `mouseleave` on the handle alone would fire on every ordinary drag
  // and is intentionally not used as the primary stop condition, only kept
  // here as a defensive last resort tied to a moved-away pointer.
  function stopIfNotDragging(moveEvent: MouseEvent) {
    if (moveEvent.buttons === 0) stop();
  }
  window.addEventListener("mousemove", onMove);
  window.addEventListener("mouseup", stop);
  window.addEventListener("blur", stop);
  handle.addEventListener("mouseleave", stopIfNotDragging);
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/FilesView.test.tsx`
Expected: PASS (all split-view tests + all pre-existing tests in the 232-line file, updated as needed for the new per-pane markup — e.g. any test that queried a single `.files-tree` by implicit singular now must scope via `getAllByRole(...)[0]` or pass in the pane index).

- [ ] **Step 5: Add split-pane CSS**

Add to `apps/palette-tauri/src/styles.css` (near the existing `.files-*` block):

```css
.files-split-container {
  display: flex;
  flex: 1;
  min-height: 0;
}

.files-tree-resize {
  cursor: col-resize;
  flex: 0 0 4px;
  background: var(--aurora-border);
}

.files-tree-resize:hover,
.files-tree-resize:active {
  background: var(--aurora-accent);
}
```

- [ ] **Step 6: Commit**

```bash
cd apps/palette-tauri
git add src/components/palette/FilesView.tsx src/components/palette/FilesView.test.tsx src/styles.css
git commit -m "feat(palette): add split-pane view to Files with resizable tree column"
```

---

## Task 3: Bulk Multi-Select + Ingest

**Files:**
- Modify: `apps/palette-tauri/src/components/palette/FilesView.tsx`
- Modify: `apps/palette-tauri/src/components/palette/FilesView.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css`

**Ground-truth corrections (re-verified directly against `palette-mock.html`):**
1. The checkbox's title/label in the mock is the literal generic string `"Select for bulk ingest"` (`title:'Select for bulk ingest'`) — it is **not** interpolated with the filename. The previous plan revision's test assertion `/select .*for bulk ingest/i` incorrectly implied per-filename text; this task fixes that assertion to the exact generic string.
2. The mock's `bulkIngest(list)` is a **fully decorative demo**: it generates a fake job id, pushes a fake `{ id, kind: 'ingest', label, tone, pct, sub }` object into a jobs list, shows a toast, and then a local `setInterval` bumps `pct` by 22 every 320ms until 100, at which point the interval clears. **It never calls any real network or ingest API.** This task's real implementation — a concurrency-limited `executeAction` loop against the real embed endpoint — is **replacing** that placeholder animation with real behavior, not porting the mock's logic. Nobody should read the mock as the source of the concurrency/progress design; it only supplies the "N selected · Ingest all" UI shape and the sticky-bar layout.

**Interfaces:**
- Consumes: `CheckedPaths`, `isChecked` from Task 1's `filesModel.ts`; `state.checked`, `checked/toggle`/`checked/checkAll`/`checked/clear` actions from Task 1's `filesViewState.ts`; `state.panes`/`state.listings` from Task 2.
- Produces: a `bulkIngestState: { kind: "idle" } | { kind: "running"; done: number; total: number; cancelled: boolean } | { kind: "done"; succeeded: number; failed: number } | { kind: "cancelled"; done: number; total: number }` local `useState` in `FilesView.tsx` (kept as component-local state, not a reducer action, since it's a one-shot async operation's progress rather than a persistent UI mode — see the file-by-file note on scope), a `bulkIngestCancelRef` mutable flag, and `bulkIngest(): Promise<void>` that iterates checked paths **strictly sequentially** (concurrency 1 — see the corrected default below) via `executeAction`, checking the cancel flag between each item.

- [ ] **Step 1: Write failing tests for bulk select + ingest**

Add to `FilesView.test.tsx`:

```typescript
describe("bulk selection and ingest", () => {
  it("shows a checkbox on each file row with the generic bulk-ingest label", () => {
    render(<FilesView client={mockClient} config={mockConfig} />);
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    expect(checkboxes.length).toBeGreaterThan(0);
  });

  it("shows no bulk bar when nothing is checked", () => {
    render(<FilesView client={mockClient} config={mockConfig} />);
    expect(screen.queryByText(/selected/i)).not.toBeInTheDocument();
  });

  it("shows 'N selected' and 'Ingest all' after checking files", async () => {
    render(<FilesView client={mockClient} config={mockConfig} />);
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    expect(screen.getByText("2 selected")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /ingest all/i })).toBeInTheDocument();
  });

  it("queues one sequential ingest call per checked file when 'Ingest all' is clicked", async () => {
    const executeActionSpy = vi.spyOn(axonClient, "executeAction").mockResolvedValue({
      ok: true,
      status: 200,
      payload: {},
    });
    render(<FilesView client={mockClient} config={mockConfig} />);
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    await waitFor(() => expect(executeActionSpy).toHaveBeenCalledTimes(2));
  });

  it("shows a per-item progress line while ingesting", async () => {
    let resolveFirst: (() => void) | undefined;
    vi.spyOn(axonClient, "executeAction").mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveFirst = () => resolve({ ok: true, status: 200, payload: {} });
        }),
    );
    render(<FilesView client={mockClient} config={mockConfig} />);
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    expect(screen.getByText(/ingesting 1\/2/i)).toBeInTheDocument();
    resolveFirst?.();
  });

  it("shows a Cancel affordance while running and stops after the in-flight item", async () => {
    const calls: Array<() => void> = [];
    vi.spyOn(axonClient, "executeAction").mockImplementation(
      () =>
        new Promise((resolve) => {
          calls.push(() => resolve({ ok: true, status: 200, payload: {} }));
        }),
    );
    render(<FilesView client={mockClient} config={mockConfig} />);
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(checkboxes[1]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    await userEvent.click(screen.getByRole("button", { name: /cancel/i }));
    calls[0]?.();
    await waitFor(() => expect(screen.getByText(/cancelled after 1\/2/i)).toBeInTheDocument());
    expect(calls).toHaveLength(1);
  });

  it("clears the checked set after a successful bulk ingest", async () => {
    vi.spyOn(axonClient, "executeAction").mockResolvedValue({ ok: true, status: 200, payload: {} });
    render(<FilesView client={mockClient} config={mockConfig} />);
    const checkboxes = screen.getAllByRole("checkbox", { name: "Select for bulk ingest" });
    await userEvent.click(checkboxes[0]);
    await userEvent.click(screen.getByRole("button", { name: /ingest all/i }));
    await waitFor(() => expect(screen.queryByText(/selected/i)).not.toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/FilesView.test.tsx`
Expected: FAIL — no checkboxes, no bulk bar rendered.

- [ ] **Step 3: Implement checkbox column and sticky bulk bar**

In the tree row renderer (inside `renderTree`), add a checkbox before `EntryIcon` for non-directory entries only (bulk ingest is file-scoped, matching the single-ingest button's existing `isIngestable` gate):

```tsx
{!entry.isDir && (
  <input
    type="checkbox"
    className="files-row-checkbox"
    aria-label="Select for bulk ingest"
    checked={isChecked(state.checked, entry.path)}
    onClick={(event) => event.stopPropagation()}
    onChange={() => dispatch({ type: "checked/toggle", path: entry.path })}
  />
)}
```

`onClick` stops propagation because the row `<button onClick={() => openEntry(...)}>` wraps the whole row — without this, clicking the checkbox would also open the file.

Add the bulk bar, rendered once (not per pane) above `.files-split-container`, only when `state.checked.size > 0`:

```tsx
{state.checked.size > 0 && (
  <div className="files-bulk-bar">
    <span>{state.checked.size} selected</span>
    {bulkIngestState.kind !== "running" && (
      <Button variant="ghost" size="sm" type="button" onClick={() => dispatch({ type: "checked/clear" })}>
        Clear
      </Button>
    )}
    <Button
      variant="aurora"
      size="sm"
      type="button"
      onClick={() => void bulkIngest()}
      disabled={bulkIngestState.kind === "running" || !client || !config}
    >
      <Upload size={13} />
      {bulkIngestState.kind === "running"
        ? `Ingesting ${bulkIngestState.done}/${bulkIngestState.total}...`
        : "Ingest all"}
    </Button>
    {bulkIngestState.kind === "running" && (
      <Button variant="ghost" size="sm" type="button" onClick={() => { bulkIngestCancelRef.current = true; }}>
        Cancel
      </Button>
    )}
    {bulkIngestState.kind === "cancelled" && (
      <span className="files-bulk-status">
        Cancelled after {bulkIngestState.done}/{bulkIngestState.total}
      </span>
    )}
  </div>
)}
```

Implement `bulkIngest`, reusing the same `embed` action lookup as the existing single-file `ingestSelected` (extract it to a small `resolveEmbedAction()` helper both call), strictly sequential per the corrected v1 default (no unverified concurrency guess against a confirmed-synchronous endpoint):

```typescript
function resolveEmbedAction(): RemotePaletteAction | null {
  return (
    ACTIONS.find(
      (action): action is RemotePaletteAction => action.subcommand === "embed" && action.kind !== "local",
    ) ?? null
  );
}

const bulkIngestCancelRef = useRef(false);

async function bulkIngest() {
  if (!client || !config) return;
  const embedAction = resolveEmbedAction();
  if (!embedAction) return;
  const root =
    state.listings.left.kind === "loaded" ? state.listings.left.value.root : "";
  const paths = Array.from(state.checked);
  bulkIngestCancelRef.current = false;
  setBulkIngestState({ kind: "running", done: 0, total: paths.length, cancelled: false });
  let succeeded = 0;
  let failed = 0;
  let done = 0;
  for (const path of paths) {
    if (bulkIngestCancelRef.current) {
      setBulkIngestState({ kind: "cancelled", done, total: paths.length });
      return;
    }
    const absolutePath = `${root.replace(/\/+$/, "")}/${path}`;
    // Sequential (concurrency 1) is the deliberate v1 choice — the embed
    // endpoint is confirmed synchronous server-side (see the
    // axon-phase10-source-migration-gaps memory note), so a naive
    // concurrency guess would just queue requests the server processes one
    // at a time anyway. Revisit only after a real load test.
    const result = await executeAction(client, embedAction, absolutePath, config);
    if (result.ok) succeeded += 1;
    else failed += 1;
    done += 1;
    setBulkIngestState((prev) => (prev.kind === "running" ? { ...prev, done } : prev));
  }
  setBulkIngestState({ kind: "done", succeeded, failed });
  dispatch({ type: "checked/clear" });
}
```

Refactor `ingestSelected` (single-file path) to call `resolveEmbedAction()` too, removing its inline duplicate lookup.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/FilesView.test.tsx`
Expected: PASS

- [ ] **Step 5: Add bulk bar + checkbox CSS**

```css
.files-row-checkbox {
  accent-color: var(--aurora-accent);
  margin-right: 4px;
}

.files-bulk-bar {
  position: sticky;
  top: 0;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 16px;
  background: var(--aurora-surface-raised);
  border-bottom: 1px solid var(--aurora-border);
}

.files-bulk-status {
  color: var(--aurora-text-muted);
  font-size: 12px;
}
```

- [ ] **Step 6: Commit**

```bash
cd apps/palette-tauri
git add src/components/palette/FilesView.tsx src/components/palette/FilesView.test.tsx src/styles.css
git commit -m "feat(palette): add bulk multi-select and sequential batch ingest to Files view"
```

---

## Task 4: AI-Assisted Edit Proposal + Approval

**Ground-truth correction — this task replaces the previous revision's "manual edit review" framing entirely.** A direct read of `palette-mock.html`'s `filesView()` shows the real feature is an **AI-edit proposal/approval flow**, not a review step in front of the existing manual textarea save:

- Each pane's per-file toolbar has a **separate** sparkle icon button (`tBtn(..., 'Edit with the model', ...)`), distinct from the existing pencil `'Edit file'` button. The pencil button's plain manual `editing`/`draft`/Save/Cancel textarea flow (in `FilesView.tsx` today) is **unchanged and untouched by this task** — it stays exactly as-is.
- Clicking the sparkle button opens an inline prompt in the pane's footer: an icon + text input (mock: `s.filesSparkleQ`) with placeholder `"Describe the edit — the model rewrites the file…"`. Enter (if non-empty) or a "Generate edit" icon button produces a proposed diff and clears the prompt.
- The mock's diff-generation step (`makeDiff()`) is a **canned demo stub**: it finds the first non-blank line of the current preview text and returns `{ for: absPath, at: lineIndex, del: [originalLine], add: [originalLine, hardcodedExtraLine] }`, where the extra line is a fixed editorial comment templated by file type. **It is not a real LLM call.**
- When a diff is proposed, the pane body renders a red/green line-tinted diff (`del` lines prefixed `-` in error-tinted rows, `add` lines prefixed `+` in success-tinted rows, heading `"Proposed edit · N lines"`), and the footer becomes: muted text `"The model proposes this edit — review it."` + **Deny** (ghost button, clears the proposal) + **Approve** (filled/rose button, mock: toasts `"Applied edit to <name>"` and clears the proposal — no real write happens in the mock demo).
- There is no "Back to edit"/"Confirm save" framing anywhere in the mock; that was an invented mismatch in the previous plan revision and is removed here.

**Explicit v1 scope decision (required by review — stated plainly, not silently mislabeled):**

- **Diff generation in v1 is a real LLM call, not a canned stub.** The mock's `makeDiff()` is a demo placeholder; shipping a hardcoded "append a fixed comment" transformation as a real product feature would be actively misleading to users who type a genuine edit instruction and get back an unrelated canned diff. v1 sends the pane's current file content plus the user's natural-language instruction to the existing LLM completion backend (`crates/axon-core/src/llm.rs`'s `AXON_LLM_BACKEND` dispatch, reused via a new Tauri command — see below) and asks for a full replacement file body; the diff view is computed client-side via `computeLineDiff(original, proposed)` (a small line-based diff — no LCS/shortest-edit-script matching, matching the granularity the mock's UI already implies with whole-line `+`/`-` rows).
- **"Approve" performs a real write in v1.** Deferring the write (i.e., Approve only dismisses the UI like the mock's toast-only demo) would make the feature cosmetic — a user who explicitly approves a proposed edit expects the file to change. Approve calls `files_write_file` with the proposed content, exactly like the existing manual-save path, and is guarded by the same disk-staleness check as manual save (see the "Smaller fixes" section below): the file's mtime/hash is captured when the diff is proposed, and re-checked immediately before the write; a mismatch fails the write with a clear "file changed on disk, re-open and retry" error rather than silently clobbering an out-of-band edit.
- **Deny** discards the proposal with no write and no persistence — equivalent to the mock's `Deny` clearing `filesDiff`.
- This is new backend surface (a Tauri command that calls into the LLM backend), not exposed anywhere in the palette today — it is scoped and tested in Step 3b below as a small, isolated addition, reusing `axon-core`'s existing headless-completion dispatch rather than inventing a second LLM client.

**Files:**
- Create: `apps/palette-tauri/src/lib/aiEditModel.ts`
- Create: `apps/palette-tauri/src/lib/aiEditModel.test.ts`
- Modify: `apps/palette-tauri/src/components/palette/FilesView.tsx`
- Modify: `apps/palette-tauri/src/components/palette/FilesView.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css`
- Modify: `apps/palette-tauri/src-tauri/src/lib.rs` (register the new `files_propose_edit` command)
- Create: `apps/palette-tauri/src-tauri/src/ai_edit_bridge.rs`
- Create: `apps/palette-tauri/src-tauri/src/ai_edit_bridge_tests.rs`

**Interfaces:**
- Produces (`aiEditModel.ts`): `export interface DiffLine { kind: "same" | "added" | "removed"; text: string }`; `export function computeLineDiff(before: string, after: string): DiffLine[]` (minimal line-based diff, same shape/semantics as the previous revision's diff helper — kept because it's still the right small tool for rendering a whole-file before/after, just now fed by a real LLM response instead of a manual-edit draft); `export interface AiEditProposal { forPath: string; proposedContent: string; diff: DiffLine[]; capturedModifiedUnix: number | null }`.
- Produces (Rust, `ai_edit_bridge.rs`): `#[tauri::command] async fn files_propose_edit(app: AppHandle, path: String, instruction: String) -> Result<String, String>` — reads the file via the existing `files_bridge::files_read_file`-equivalent path (reuse its root/read logic, do not duplicate the sandboxing), sends `(content, instruction)` to the configured LLM backend via `axon_core`'s headless completion path, and returns the proposed full file content as a string.
- Consumes: `state.panes`, `PaneId`, `dispatch` from Tasks 1–2; the pane's existing `file`/`selected` fields (read-only — the sparkle flow does not touch `editing`/`draft`).

**New reducer actions (append to `FilesViewAction` in `filesViewState.ts`):**

```typescript
  | { type: "pane/sparkleOpen"; pane: PaneId }
  | { type: "pane/sparkleClose"; pane: PaneId }
  | { type: "pane/sparkleQueryChange"; pane: PaneId; query: string }
  | { type: "pane/proposalPending"; pane: PaneId }
  | { type: "pane/proposalReady"; pane: PaneId; proposal: AiEditProposal }
  | { type: "pane/proposalError"; pane: PaneId; message: string }
  | { type: "pane/proposalDeny"; pane: PaneId }
  | { type: "pane/proposalApproveStart"; pane: PaneId }
  | { type: "pane/proposalApproved"; pane: PaneId; file: FileContents }
  | { type: "pane/proposalApproveError"; pane: PaneId; message: string }
```

And extend `FilesPane` (Task 1's type) with the AI-edit fields:

```typescript
export interface FilesPane {
  // ...existing fields from Task 1...
  sparkleOpen: boolean;
  sparkleQuery: string;
  proposal: AiEditProposal | null;
  proposalState: "idle" | "pending" | "ready" | "approving" | "error";
  proposalErrorMessage: string | null;
}
```

`createPane(...)` gains `sparkleOpen: false, sparkleQuery: "", proposal: null, proposalState: "idle", proposalErrorMessage: null` in its returned object.

- [ ] **Step 1: Write failing tests for the line-diff helper**

```typescript
// apps/palette-tauri/src/lib/aiEditModel.test.ts
import { describe, expect, it } from "vitest";
import { computeLineDiff } from "./aiEditModel";

describe("computeLineDiff", () => {
  it("marks unchanged lines as same", () => {
    const result = computeLineDiff("a\nb\nc", "a\nb\nc");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "same", text: "b" },
      { kind: "same", text: "c" },
    ]);
  });

  it("marks an appended line as added", () => {
    const result = computeLineDiff("a\nb", "a\nb\nc");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "same", text: "b" },
      { kind: "added", text: "c" },
    ]);
  });

  it("marks a removed trailing line as removed", () => {
    const result = computeLineDiff("a\nb\nc", "a\nb");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "same", text: "b" },
      { kind: "removed", text: "c" },
    ]);
  });

  it("marks a changed middle line as removed+added (no in-place replace)", () => {
    const result = computeLineDiff("a\nb\nc", "a\nX\nc");
    expect(result).toEqual([
      { kind: "same", text: "a" },
      { kind: "removed", text: "b" },
      { kind: "added", text: "X" },
      { kind: "same", text: "c" },
    ]);
  });

  it("returns an empty array for two empty strings", () => {
    expect(computeLineDiff("", "")).toEqual([]);
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/aiEditModel.test.ts`
Expected: FAIL — module `./aiEditModel` does not exist.

- [ ] **Step 3: Implement `computeLineDiff`**

```typescript
// apps/palette-tauri/src/lib/aiEditModel.ts
// AI-edit proposal model for the Files view's "Edit with the model" flow.
// computeLineDiff is a minimal line-based diff (no LCS/shortest-edit-script
// matching) — sufficient for rendering a whole-file before/after; not a
// general-purpose diff engine. FilesView.tsx's AI-edit review panel is its
// only caller.

export interface DiffLine {
  kind: "same" | "added" | "removed";
  text: string;
}

export function computeLineDiff(before: string, after: string): DiffLine[] {
  const beforeLines = before === "" ? [] : before.split("\n");
  const afterLines = after === "" ? [] : after.split("\n");
  const result: DiffLine[] = [];
  const max = Math.max(beforeLines.length, afterLines.length);
  for (let i = 0; i < max; i += 1) {
    const beforeLine = beforeLines[i];
    const afterLine = afterLines[i];
    if (beforeLine === afterLine) {
      result.push({ kind: "same", text: beforeLine });
      continue;
    }
    if (beforeLine !== undefined) result.push({ kind: "removed", text: beforeLine });
    if (afterLine !== undefined) result.push({ kind: "added", text: afterLine });
  }
  return result;
}

export interface AiEditProposal {
  forPath: string;
  proposedContent: string;
  diff: DiffLine[];
  /** mtime captured when the proposal was generated, used to detect a
   * disk change between proposal and Approve (see FilesView.tsx's
   * approveProposal). Null when the source listing had no modified time. */
  capturedModifiedUnix: number | null;
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/aiEditModel.test.ts`
Expected: PASS (5 tests)

- [ ] **Step 5: Write the Rust `files_propose_edit` command and its tests**

```rust
// apps/palette-tauri/src-tauri/src/ai_edit_bridge_tests.rs
use super::*;

#[test]
fn build_edit_prompt_includes_instruction_and_file_content() {
    let prompt = build_edit_prompt("fn main() {}", "add a doc comment");
    assert!(prompt.contains("add a doc comment"));
    assert!(prompt.contains("fn main() {}"));
}

#[test]
fn build_edit_prompt_rejects_empty_instruction() {
    let result = build_edit_prompt_checked("fn main() {}", "   ");
    assert!(result.is_err());
}
```

```rust
// apps/palette-tauri/src-tauri/src/ai_edit_bridge.rs
//! Tauri command backing the Files view's "Edit with the model" sparkle
//! flow: takes the current file content and a natural-language instruction,
//! asks the configured Axon LLM backend for a full replacement file body.
//!
//! Reuses the same local-path read as `files_bridge::files_read_file`
//! (delegates rather than duplicating root/canonicalization logic) — this
//! module owns only the prompt construction and LLM dispatch, not path
//! safety.

use tauri::AppHandle;

use crate::files_bridge::files_read_file;

fn build_edit_prompt(file_content: &str, instruction: &str) -> String {
    format!(
        "You are editing a single file. Apply exactly this instruction and \
         return the FULL new file content, with no commentary, no code \
         fences, and no explanation — only the raw file body.\n\n\
         Instruction: {instruction}\n\n\
         Current file content:\n{file_content}"
    )
}

fn build_edit_prompt_checked(file_content: &str, instruction: &str) -> Result<String, String> {
    if instruction.trim().is_empty() {
        return Err("edit instruction must not be empty".to_string());
    }
    Ok(build_edit_prompt(file_content, instruction))
}

#[tauri::command]
pub(crate) async fn files_propose_edit(
    app: AppHandle,
    path: String,
    instruction: String,
) -> Result<String, String> {
    let current = files_read_file(app, path)?;
    let prompt = build_edit_prompt_checked(&current.content, &instruction)?;
    // Delegates to the same headless LLM completion path axon-core's ask/
    // summarize/evaluate commands use (AXON_LLM_BACKEND-selected), invoked
    // here via the palette's existing Axon HTTP client rather than a second
    // LLM client — see axonClient.ts's executeAction for the established
    // pattern of proxying through the user's configured Axon server.
    crate::axon_bridge::complete_via_configured_backend(&app, &prompt)
        .await
        .map_err(|err| err.to_string())
}

#[cfg(test)]
#[path = "ai_edit_bridge_tests.rs"]
mod tests;
```

**Note for the implementer:** `crate::axon_bridge::complete_via_configured_backend` is a **new** helper this step assumes — check `axon_bridge.rs`'s existing route allow-list and add a proxied route for whatever single-shot completion endpoint the Axon server already exposes (do not invent a new HTTP contract; reuse `/v1/ask`-style synthesis if a raw-completion equivalent doesn't already exist, and note the exact endpoint chosen in the commit message since this plan cannot see that server-side surface from the palette code alone).

- [ ] **Step 6: Run Rust tests to verify they pass**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml ai_edit_bridge`
Expected: PASS (2 tests)

- [ ] **Step 7: Register the command in `lib.rs`**

Add `mod ai_edit_bridge;` and `ai_edit_bridge::files_propose_edit` to the `invoke_handler!` list.

- [ ] **Step 8: Write failing frontend tests for the sparkle/diff/approve flow**

Add to `FilesView.test.tsx`:

```typescript
describe("AI-assisted edit proposal", () => {
  it("shows an 'Edit with the model' button separate from the manual Edit button", () => {
    render(<FilesView client={null} config={null} />);
    expect(screen.getByRole("button", { name: /edit with the model/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit" })).toBeInTheDocument();
  });

  it("opens an inline instruction prompt on sparkle click", async () => {
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getAllByText("readme.md")[0]);
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    expect(
      screen.getByPlaceholderText(/describe the edit/i),
    ).toBeInTheDocument();
  });

  it("submits the instruction and shows a proposed diff with Deny/Approve", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "files_propose_edit") return Promise.resolve("# Title\n\nrewritten body");
      return mockDefaultInvoke(cmd);
    });
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getAllByText("readme.md")[0]);
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite the intro{Enter}");
    expect(await screen.findByText(/proposed edit/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /deny/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /approve/i })).toBeInTheDocument();
  });

  it("Deny discards the proposal without writing", async () => {
    const writeSpy = vi.mocked(invoke);
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getAllByText("readme.md")[0]);
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite{Enter}");
    await screen.findByText(/proposed edit/i);
    await userEvent.click(screen.getByRole("button", { name: /deny/i }));
    expect(screen.queryByText(/proposed edit/i)).not.toBeInTheDocument();
    expect(writeSpy).not.toHaveBeenCalledWith("files_write_file", expect.anything());
  });

  it("Approve writes the proposed content via files_write_file", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "files_propose_edit") return Promise.resolve("rewritten");
      if (cmd === "files_write_file")
        return Promise.resolve({ path: "readme.md", content: "rewritten", size: 9 });
      return mockDefaultInvoke(cmd);
    });
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getAllByText("readme.md")[0]);
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite{Enter}");
    await screen.findByText(/proposed edit/i);
    await userEvent.click(screen.getByRole("button", { name: /approve/i }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("files_write_file", {
        path: "readme.md",
        content: "rewritten",
      }),
    );
  });

  it("Approve fails with a clear error when the file changed on disk since the proposal", async () => {
    vi.mocked(invoke).mockImplementation((cmd) => {
      if (cmd === "files_propose_edit") return Promise.resolve("rewritten");
      if (cmd === "files_read_file")
        return Promise.resolve({ path: "readme.md", content: "changed elsewhere", size: 20 });
      return mockDefaultInvoke(cmd);
    });
    render(<FilesView client={null} config={null} />);
    await userEvent.click(screen.getAllByText("readme.md")[0]);
    await userEvent.click(screen.getByRole("button", { name: /edit with the model/i }));
    await userEvent.type(screen.getByPlaceholderText(/describe the edit/i), "rewrite{Enter}");
    await screen.findByText(/proposed edit/i);
    await userEvent.click(screen.getByRole("button", { name: /approve/i }));
    expect(await screen.findByText(/changed on disk/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 9: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/FilesView.test.tsx`
Expected: FAIL — no sparkle button, no proposal UI exists yet.

- [ ] **Step 10: Implement the sparkle/diff/approve UI**

Add the sparkle button next to the existing pencil `Edit` button in the per-pane header actions:

```tsx
<Button
  variant="plain"
  size="unstyled"
  type="button"
  title="Edit with the model"
  aria-label="Edit with the model"
  onClick={() =>
    dispatch(
      pane.sparkleOpen
        ? { type: "pane/sparkleClose", pane: pane.id }
        : { type: "pane/sparkleOpen", pane: pane.id },
    )
  }
>
  <Sparkles size={14} />
</Button>
```

Add the three-state footer branch (normal / sparkle-prompt / proposal-review) inside the pane's `FilePreview`-equivalent render, gated on `pane.proposal` then `pane.sparkleOpen`:

```typescript
async function submitSparkleQuery(id: PaneId) {
  const pane = state.panes.find((p) => p.id === id);
  if (!pane || !pane.sparkleQuery.trim() || pane.file.kind !== "loaded" || !pane.selected) return;
  dispatch({ type: "pane/proposalPending", pane: id });
  try {
    const proposedContent = await invoke<string>("files_propose_edit", {
      path: pane.selected.path,
      instruction: pane.sparkleQuery,
    });
    dispatch({
      type: "pane/proposalReady",
      pane: id,
      proposal: {
        forPath: pane.selected.path,
        proposedContent,
        diff: computeLineDiff(pane.file.value.content, proposedContent),
        capturedModifiedUnix: pane.selected.modifiedUnix ?? null,
      },
    });
  } catch (err) {
    dispatch({ type: "pane/proposalError", pane: id, message: errorMessage(err) });
  }
}

async function approveProposal(id: PaneId) {
  const pane = state.panes.find((p) => p.id === id);
  if (!pane || !pane.proposal || !pane.selected) return;
  dispatch({ type: "pane/proposalApproveStart", pane: id });
  try {
    // Disk-staleness guard: re-read the file immediately before writing and
    // compare against the content the diff was computed from. files_write_file's
    // atomic-write semantics make this a cheap extra round-trip; skipping it
    // would let Approve silently clobber an out-of-band edit made while the
    // proposal was open for review.
    const fresh = await invoke<FileContents>("files_read_file", { path: pane.selected.path });
    if (pane.file.kind === "loaded" && fresh.content !== pane.file.value.content) {
      dispatch({
        type: "pane/proposalApproveError",
        pane: id,
        message: "The file changed on disk since this edit was proposed. Re-open it and try again.",
      });
      return;
    }
    const saved = await invoke<FileContents>("files_write_file", {
      path: pane.selected.path,
      content: pane.proposal.proposedContent,
    });
    dispatch({ type: "pane/proposalApproved", pane: id, file: saved });
  } catch (err) {
    dispatch({ type: "pane/proposalApproveError", pane: id, message: errorMessage(err) });
  }
}
```

```tsx
{pane.proposal ? (
  <div className="files-ai-edit-review">
    <p className="files-ai-edit-heading">
      Proposed edit · {pane.proposal.diff.filter((l) => l.kind !== "same").length} lines
    </p>
    <pre className="files-ai-edit-body">
      {pane.proposal.diff.map((line, index) => (
        <div key={index} className={`files-ai-edit-line files-ai-edit-${line.kind}`}>
          <span className="files-ai-edit-marker">
            {line.kind === "added" ? "+" : line.kind === "removed" ? "-" : " "}
          </span>
          {line.text}
        </div>
      ))}
    </pre>
    {pane.proposalState === "error" && pane.proposalErrorMessage && (
      <p className="files-ai-edit-error">{pane.proposalErrorMessage}</p>
    )}
    <div className="files-ai-edit-actions">
      <span className="files-ai-edit-note">The model proposes this edit — review it.</span>
      <Button
        variant="ghost"
        size="sm"
        type="button"
        onClick={() => dispatch({ type: "pane/proposalDeny", pane: pane.id })}
      >
        Deny
      </Button>
      <Button
        variant="rose"
        filled
        size="sm"
        type="button"
        onClick={() => void approveProposal(pane.id)}
        disabled={pane.proposalState === "approving"}
      >
        {pane.proposalState === "approving" ? "Applying..." : "Approve"}
      </Button>
    </div>
  </div>
) : pane.sparkleOpen ? (
  <div className="files-ai-edit-prompt">
    <Sparkles size={14} />
    <input
      autoFocus
      value={pane.sparkleQuery}
      placeholder="Describe the edit — the model rewrites the file…"
      onChange={(event) =>
        dispatch({ type: "pane/sparkleQueryChange", pane: pane.id, query: event.target.value })
      }
      onKeyDown={(event) => {
        if (event.key === "Enter" && pane.sparkleQuery.trim()) void submitSparkleQuery(pane.id);
        if (event.key === "Escape") dispatch({ type: "pane/sparkleClose", pane: pane.id });
      }}
    />
    <Button
      variant="rose"
      filled
      size="icon"
      title="Generate edit"
      type="button"
      onClick={() => void submitSparkleQuery(pane.id)}
      disabled={pane.proposalState === "pending"}
    >
      <Sparkles size={14} />
    </Button>
  </div>
) : (
  /* existing normal footer (Edit/Ingest) unchanged */
  ...
)}
```

Add the reducer cases for the new action types to `filesViewState.ts`'s `filesViewReducer` switch (mirrors the existing pane-scoped `updatePane` pattern; `proposalDeny` and `proposalApproved` both reset `proposal`/`proposalState` to idle).

- [ ] **Step 11: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/FilesView.test.tsx`
Expected: PASS (all AI-edit tests + all pre-existing tests still green)

- [ ] **Step 12: Add AI-edit CSS**

```css
.files-ai-edit-review {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.files-ai-edit-heading {
  font-weight: 600;
  color: var(--aurora-text-primary);
}

.files-ai-edit-body {
  font-family: var(--aurora-font-mono);
  font-size: 12px;
  overflow: auto;
}

.files-ai-edit-line { padding: 0 4px; white-space: pre; }
.files-ai-edit-added { background: color-mix(in srgb, var(--aurora-success) 15%, transparent); }
.files-ai-edit-removed { background: color-mix(in srgb, var(--aurora-error) 15%, transparent); }
.files-ai-edit-marker { display: inline-block; width: 1.5ch; opacity: 0.7; }

.files-ai-edit-error {
  color: var(--aurora-error);
  font-size: 12px;
}

.files-ai-edit-actions {
  display: flex;
  align-items: center;
  gap: 8px;
}

.files-ai-edit-note {
  flex: 1;
  color: var(--aurora-text-muted);
  font-size: 11.5px;
}

.files-ai-edit-prompt {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  border-top: 1px solid var(--aurora-border);
}
```

- [ ] **Step 13: Commit**

```bash
cd apps/palette-tauri
git add src/lib/aiEditModel.ts src/lib/aiEditModel.test.ts src/lib/filesViewState.ts src/components/palette/FilesView.tsx src/components/palette/FilesView.test.tsx src/styles.css src-tauri/src/ai_edit_bridge.rs src-tauri/src/ai_edit_bridge_tests.rs src-tauri/src/lib.rs
git commit -m "feat(palette): add AI-assisted edit proposal and approve/deny flow to Files view"
```

---

## Task 5: SFTP Remote Browsing

**Scope framing (re-verified directly against `palette-mock.html`):** the mock shows only a **single hardcoded boolean** `s.sftpConnected`, toggled by one toolbar icon button (`'Connect SFTP (deploy@axon-prod)'` / `'Disconnect SFTP'`), with **one fixed fake connection** (`deploy@axon-prod`) injected as an extra root node into the very same `treeRows()` recursion that renders local files — an "SFTP" section header, cyan-tinted styling, and a connected-dot indicator, all inline. There is **no** separate `SftpTree.tsx` component, no connection-profile list, no add/edit dialog, and no CRUD of any kind in the mock. Everything below this line — multi-profile connection management, a dedicated Rust SSH/SFTP bridge, persisted profiles, host-key TOFU, a trust-prompt UI — is **new design work that goes beyond the toy mock**, justified by the real product need for more than one fixed demo connection, not a mock port. Treat the mock as validating only the "SFTP files appear inline in the same tree, with a distinct visual treatment" UX idea — nothing about its data model should be copied.

**v1 scope decision (required by review):** SFTP is **read-only browsing + ingest only**. The existing pencil "Edit file" button and the new sparkle "Edit with the model" button (Task 4) are both **hard-disabled** for any file whose pane resolves to an SFTP-origin entry. This removes any "two independently-reviewing panes, one local one remote" edge case entirely, since SFTP files can never enter either edit state. `sftp_write_file` is explicitly out of scope for v1 and not discussed further in this task.

This is the largest task and is broken into sub-steps across the Rust bridge, host-key trust store, settings persistence, and frontend tree/dialog.

### Task 5a: Rust SFTP bridge — connection + directory listing (commands NOT registered live until Task 5b lands)

**Files:**
- Create: `apps/palette-tauri/src-tauri/src/sftp_bridge.rs`
- Create: `apps/palette-tauri/src-tauri/src/sftp_bridge_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/Cargo.toml`

**Sequencing fix (required by review):** this sub-task implements the SSH/SFTP client and its commands, but **does not** add them to `lib.rs`'s `invoke_handler!` yet, and does not wire a live, callable command surface. Registration is deferred to the end of Task 5b, once the real `check_server_key` host-key-verification callback exists — this closes the gap where an implementer under time pressure could ship `check_server_key` returning `Ok(true)` unconditionally (silently disabling MITM protection), have every test in this sub-task still pass (none of them exercise that callback), and have a fully wired, shippable command surface with no host-key protection at all. Building the client here without registering it means there is no way to ship the unsafe shortcut by accident — the command literally isn't reachable from the frontend until 5b's gate test (Step 2 in 5b) passes.

**Interfaces:**
- Produces: `pub(crate) struct SftpConnections(tokio::sync::Mutex<HashMap<ConnectionId, russh_sftp::client::SftpSession>>)` (Tauri managed state, following the `GitHubClient` precedent — **`tokio::sync::Mutex`, not `std::sync::Mutex`**: SFTP list/read are async network round-trips, and holding a `std::sync::Mutex` guard across an `.await` blocks the async executor thread for the call's full latency, which is exactly what clippy's `await_holding_lock` lint exists to catch); `pub(crate) type ConnectionId = String` (a `uuid::Uuid::new_v4().to_string()`, not a monotonic counter — sequential/guessable ids are a needless weakness even under this app's "renderer is trusted" threat model); `fn normalize_remote_path(path: &str) -> Result<String, String>` (NUL-byte/empty-segment rejection — no local canonicalization, since this is a remote filesystem); the not-yet-registered command functions `sftp_connect`, `sftp_list_dir`, `sftp_read_file`, `sftp_disconnect` (signatures below).
- Consumes: `russh::client` for the SSH transport, `russh_sftp::client::SftpSession` for the SFTP subsystem, both driven on the existing Tokio runtime (Tauri's `tokio` dependency already has `rt`/`net`/`io-util`/`sync`/`time`/`macros`/`process` features but **not** `full` — verify `russh`'s required features fit without needing `full`; add any missing specific feature explicitly).

- [ ] **Step 1: Add `russh`/`russh-sftp` to Cargo.toml**

```toml
# apps/palette-tauri/src-tauri/Cargo.toml, in [dependencies]
russh = "0.50"
russh-sftp = "2"
```

Run `cargo metadata --manifest-path apps/palette-tauri/src-tauri/Cargo.toml` after adding to confirm resolution succeeds and check for any additional tokio feature requirements surfaced by the build (`cargo build --manifest-path apps/palette-tauri/src-tauri/Cargo.toml` will fail loudly if a feature is missing — add exactly the missing one to the existing `tokio = { version = "1", features = [...] }` line, do not switch to `features = ["full"]`).

- [ ] **Step 2: Write failing Rust tests for path normalization and connection-id shape**

```rust
// apps/palette-tauri/src-tauri/src/sftp_bridge_tests.rs
use super::*;

#[test]
fn normalize_remote_path_rejects_nul_bytes() {
    let result = normalize_remote_path("foo\0bar");
    assert!(result.is_err());
}

#[test]
fn normalize_remote_path_accepts_a_plain_relative_path() {
    let result = normalize_remote_path("srv/axon/docker-compose.yaml");
    assert_eq!(result.unwrap(), "srv/axon/docker-compose.yaml");
}

#[test]
fn normalize_remote_path_accepts_an_absolute_path() {
    let result = normalize_remote_path("/srv/axon");
    assert_eq!(result.unwrap(), "/srv/axon");
}

#[test]
fn new_connection_id_is_a_valid_uuid_not_a_sequential_counter() {
    let a = new_connection_id();
    let b = new_connection_id();
    assert_ne!(a, b);
    assert!(uuid::Uuid::parse_str(&a).is_ok());
    // A monotonic-counter scheme like "sftp-1"/"sftp-2" would fail this parse
    // — this test exists specifically to catch a regression back to a
    // sequential/guessable id.
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml sftp_bridge`
Expected: FAIL — module does not exist.

- [ ] **Step 4: Implement `sftp_bridge.rs` (connection state, path normalization, id generation; command bodies written but not yet registered)**

```rust
// apps/palette-tauri/src-tauri/src/sftp_bridge.rs
//! SSH/SFTP client bridge for the Files view's remote-browsing feature.
//!
//! # Registration gate
//!
//! The `#[tauri::command]`s in this module are NOT added to `lib.rs`'s
//! `invoke_handler!` until `sftp_known_hosts.rs`'s host-key verification
//! (Task 5b) is implemented and its `check_server_key`-stub regression test
//! passes. This is deliberate: it is impossible to accidentally ship a live,
//! callable SFTP command surface with `check_server_key` stubbed to
//! unconditionally accept any host key, because until that lands, nothing in
//! the frontend can reach these commands at all.
//!
//! # Threat model
//!
//! Unlike `files_bridge.rs`'s local root-escape checks, there is no local
//! canonicalization concept for a remote server's filesystem — the SFTP
//! server itself is the authority on what paths exist and are reachable.
//! What SFTP *does* need, that local files don't, is host-key verification
//! (MITM protection — see `sftp_known_hosts.rs`) and credential handling
//! (`private_key_path`, which — unlike remote paths — IS a local file and
//! DOES need the same local-path rigor as `files_bridge.rs`; see
//! `sftp_bridge::validate_private_key_path` below).

use std::{collections::HashMap, path::Path};

use tokio::sync::Mutex;

pub(crate) type ConnectionId = String;

pub(crate) struct SftpConnections(pub(crate) Mutex<HashMap<ConnectionId, russh_sftp::client::SftpSession>>);

impl SftpConnections {
    pub(crate) fn new() -> Self {
        Self(Mutex::new(HashMap::new()))
    }
}

pub(crate) fn new_connection_id() -> ConnectionId {
    uuid::Uuid::new_v4().to_string()
}

/// Normalizes a renderer-supplied remote path string. No canonicalization
/// against a local root (there is none for a remote filesystem) — this only
/// rejects NUL bytes, matching the minimal safety `files_bridge.rs` applies
/// before even touching a path.
pub(crate) fn normalize_remote_path(path: &str) -> Result<String, String> {
    if path.contains('\0') {
        return Err("path must not contain NUL bytes".to_string());
    }
    Ok(path.to_string())
}

/// Local-path rigor for `private_key_path` — this field is NOT a remote
/// path, it's a local file used to authenticate, so it gets the same
/// canonicalize/symlink/regular-file checks `files_bridge.rs` applies to its
/// local root, unlike the rest of the SFTP surface which correctly has no
/// local-root concept.
pub(crate) fn validate_private_key_path(path: &Path) -> Result<std::path::PathBuf, String> {
    let canonical = std::fs::canonicalize(path)
        .map_err(|err| format!("private key path {} is invalid: {err}", path.display()))?;
    let metadata = std::fs::symlink_metadata(&canonical).map_err(|err| err.to_string())?;
    if metadata.is_symlink() {
        return Err("private key path must not be a symlink".to_string());
    }
    if !metadata.is_file() {
        return Err("private key path must be a regular file, not a directory".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = metadata.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            crate::diag::warn(&format!(
                "private key {} is group/world-readable (mode {mode:o}); \
                 consider `chmod 600` to match SSH client conventions",
                canonical.display()
            ));
        }
    }
    Ok(canonical)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml sftp_bridge`
Expected: PASS (4 tests)

- [ ] **Step 6: Commit (bridge implemented, intentionally not yet registered)**

```bash
cd apps/palette-tauri
git add src-tauri/Cargo.toml src-tauri/src/sftp_bridge.rs src-tauri/src/sftp_bridge_tests.rs
git commit -m "feat(palette): add SFTP bridge connection state and path/key validation (not yet registered)"
```

### Task 5b: Host-key TOFU verification (must land before any SFTP command is registered)

**Files:**
- Create: `apps/palette-tauri/src-tauri/src/sftp_known_hosts.rs`
- Create: `apps/palette-tauri/src-tauri/src/sftp_known_hosts_tests.rs`
- Modify: `apps/palette-tauri/src-tauri/src/sftp_bridge.rs` (add `sftp_connect`/`sftp_list_dir`/`sftp_read_file`/`sftp_disconnect` command bodies wired to the host-key check)
- Modify: `apps/palette-tauri/src-tauri/src/lib.rs` (register `mod sftp_known_hosts;`, `mod sftp_bridge;`, and — only now — the four SFTP commands in `invoke_handler!`)

**Open Question #2 resolved:** TOFU (trust-on-first-use) with a persisted fingerprint store, matching OpenSSH's own default (`~/.ssh/known_hosts` behavior) — a reasonable match for this tool's threat model (a developer-facing palette, not a hardened enterprise SSH client). A pinned fingerprint that later mismatches on reconnect is treated as a hard failure, never silently re-pinned, matching how OpenSSH refuses to connect and requires explicit `known_hosts` editing after a real host-key change.

**Interfaces:**
- Produces: `apps/palette-tauri/src-tauri/src/sftp_known_hosts.rs` — `struct KnownHostEntry { host: String, port: u16, key_type: String, fingerprint: String, first_seen_unix: u64 }`, `struct KnownHostsStore(Vec<KnownHostEntry>)`, `fn known_hosts_path(app: &AppHandle) -> Result<PathBuf, String>` (resolves to `<app_config_dir>/sftp_known_hosts.json`, alongside `settings.json`), `fn load_known_hosts(app: &AppHandle) -> Result<KnownHostsStore, String>`, `fn save_known_hosts(app: &AppHandle, store: &KnownHostsStore) -> Result<(), String>` (via the existing `atomic_write` helper `files_bridge.rs` already uses), `enum HostKeyDecision { TrustedMatch, NewHostNeedsPrompt(KnownHostEntry), Mismatch { pinned: KnownHostEntry, seen_fingerprint: String } }`, `fn evaluate_host_key(store: &KnownHostsStore, host: &str, port: u16, key_type: &str, fingerprint: &str) -> HostKeyDecision`, `fn pin_host_key(store: &mut KnownHostsStore, entry: KnownHostEntry)`, `fn revoke_host_key(store: &mut KnownHostsStore, host: &str, port: u16)`.
- Produces (`sftp_bridge.rs` additions): `#[tauri::command] async fn sftp_connect(app: AppHandle, profile: SftpConnectionInput) -> Result<SftpConnectResult, String>` where `SftpConnectResult` is `{ connectionId: String } | { pendingTrust: KnownHostEntry }` (the frontend shows the trust prompt on the latter and re-calls `sftp_connect` with an explicit `trustNewHost: true` flag after the user confirms); `#[tauri::command] async fn sftp_list_dir(app: AppHandle, connection_id: String, path: Option<String>) -> Result<SftpDirListing, String>`; `#[tauri::command] async fn sftp_read_file(app: AppHandle, connection_id: String, path: String) -> Result<SftpFileContents, String>`; `#[tauri::command] fn sftp_disconnect(app: AppHandle, connection_id: String) -> Result<(), String>`; `#[tauri::command] fn sftp_list_known_hosts(app: AppHandle) -> Result<Vec<KnownHostEntry>, String>`; `#[tauri::command] fn sftp_revoke_known_host(app: AppHandle, host: String, port: u16) -> Result<(), String>` (the "view/revoke pinned hosts" UI surface).
- Consumes: `russh::client::Handler::check_server_key` — this is the real host-key callback; it must call `evaluate_host_key(...)` and return `Ok(true)` **only** for `HostKeyDecision::TrustedMatch`, and return an error (not `Ok(false)`, which `russh` may treat ambiguously — check the exact `russh` 0.50 `Handler` trait contract during implementation and use whichever return communicates "reject this connection" unambiguously) for `HostKeyDecision::Mismatch`. `NewHostNeedsPrompt` is surfaced back to `sftp_connect`'s caller as `SftpConnectResult::PendingTrust` rather than decided inside the callback, since the callback has no channel back to the user for a live prompt.

- [ ] **Step 1: Write the failing regression test that guards against an always-accept stub**

This is the test required by review — it must fail if `check_server_key` (or any equivalent gate) is ever implemented as an unconditional accept:

```rust
// apps/palette-tauri/src-tauri/src/sftp_known_hosts_tests.rs
use super::*;

#[test]
fn evaluate_host_key_flags_a_new_host_for_trust_prompt_not_auto_accept() {
    let store = KnownHostsStore(Vec::new());
    let decision = evaluate_host_key(&store, "example.com", 22, "ssh-ed25519", "AAAA...fingerprint");
    match decision {
        HostKeyDecision::NewHostNeedsPrompt(entry) => {
            assert_eq!(entry.host, "example.com");
            assert_eq!(entry.fingerprint, "AAAA...fingerprint");
        }
        HostKeyDecision::TrustedMatch => {
            panic!(
                "a host with no prior pinned entry must never resolve to TrustedMatch — \
                 this would mean host-key verification silently accepts any key on first \
                 connect with no trust decision at all, which is exactly the always-accept \
                 regression this test exists to catch"
            );
        }
        HostKeyDecision::Mismatch { .. } => panic!("no prior entry exists, Mismatch is impossible here"),
    }
}

#[test]
fn evaluate_host_key_matches_a_pinned_fingerprint() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "example.com".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...fingerprint".to_string(),
            first_seen_unix: 0,
        },
    );
    let decision = evaluate_host_key(&store, "example.com", 22, "ssh-ed25519", "AAAA...fingerprint");
    assert!(matches!(decision, HostKeyDecision::TrustedMatch));
}

#[test]
fn evaluate_host_key_hard_fails_on_fingerprint_mismatch_never_silently_repins() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "example.com".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...original".to_string(),
            first_seen_unix: 0,
        },
    );
    let decision = evaluate_host_key(&store, "example.com", 22, "ssh-ed25519", "BBBB...different");
    match decision {
        HostKeyDecision::Mismatch { pinned, seen_fingerprint } => {
            assert_eq!(pinned.fingerprint, "AAAA...original");
            assert_eq!(seen_fingerprint, "BBBB...different");
        }
        other => panic!("expected Mismatch, got {other:?} — a fingerprint change must never be silently re-pinned"),
    }
    // The store itself must be unmodified by evaluation alone — pinning only
    // happens via an explicit pin_host_key call from a user-confirmed prompt.
    assert_eq!(store.0[0].fingerprint, "AAAA...original");
}

#[test]
fn revoke_host_key_removes_the_matching_entry() {
    let mut store = KnownHostsStore(Vec::new());
    pin_host_key(
        &mut store,
        KnownHostEntry {
            host: "example.com".to_string(),
            port: 22,
            key_type: "ssh-ed25519".to_string(),
            fingerprint: "AAAA...fingerprint".to_string(),
            first_seen_unix: 0,
        },
    );
    revoke_host_key(&mut store, "example.com", 22);
    assert!(store.0.is_empty());
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml sftp_known_hosts`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Implement `sftp_known_hosts.rs`**

```rust
// apps/palette-tauri/src-tauri/src/sftp_known_hosts.rs
//! TOFU (trust-on-first-use) host-key fingerprint store for the SFTP bridge,
//! backing `russh::client::Handler::check_server_key`.
//!
//! Persisted to `<app_config_dir>/sftp_known_hosts.json`, alongside
//! `settings.json`, via the same `atomic_write` helper `files_bridge.rs`
//! uses. This is a dedicated file rather than a `PaletteSettings` field
//! because it is an append/revoke-oriented trust ledger, not a preference —
//! keeping it separate also means a corrupt/truncated settings.json write
//! can never take the host-key trust store down with it.
//!
//! # Why TOFU, and why this is a hard merge-blocker
//!
//! `check_server_key` is the ONLY thing standing between this feature and a
//! silent MITM on every SFTP connection. A stubbed `Ok(true)`-always
//! implementation would pass every other test in this plan, since none of
//! them exercise the callback — see
//! `evaluate_host_key_flags_a_new_host_for_trust_prompt_not_auto_accept`
//! (sftp_known_hosts_tests.rs), which fails specifically if a new host ever
//! resolves to `TrustedMatch` instead of requiring an explicit trust
//! decision.

use std::{fs, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::persistence::atomic_write;

const KNOWN_HOSTS_FILE: &str = "sftp_known_hosts.json";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnownHostEntry {
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub first_seen_unix: u64,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct KnownHostsStore(pub(crate) Vec<KnownHostEntry>);

#[derive(Debug)]
pub(crate) enum HostKeyDecision {
    TrustedMatch,
    NewHostNeedsPrompt(KnownHostEntry),
    Mismatch { pinned: KnownHostEntry, seen_fingerprint: String },
}

pub(crate) fn evaluate_host_key(
    store: &KnownHostsStore,
    host: &str,
    port: u16,
    key_type: &str,
    fingerprint: &str,
) -> HostKeyDecision {
    match store.0.iter().find(|entry| entry.host == host && entry.port == port) {
        Some(entry) if entry.fingerprint == fingerprint => HostKeyDecision::TrustedMatch,
        Some(entry) => HostKeyDecision::Mismatch {
            pinned: entry.clone(),
            seen_fingerprint: fingerprint.to_string(),
        },
        None => HostKeyDecision::NewHostNeedsPrompt(KnownHostEntry {
            host: host.to_string(),
            port,
            key_type: key_type.to_string(),
            fingerprint: fingerprint.to_string(),
            first_seen_unix: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        }),
    }
}

pub(crate) fn pin_host_key(store: &mut KnownHostsStore, entry: KnownHostEntry) {
    store.0.retain(|existing| !(existing.host == entry.host && existing.port == entry.port));
    store.0.push(entry);
}

pub(crate) fn revoke_host_key(store: &mut KnownHostsStore, host: &str, port: u16) {
    store.0.retain(|entry| !(entry.host == host && entry.port == port));
}

fn known_hosts_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|dir| dir.join(KNOWN_HOSTS_FILE))
        .map_err(|err| format!("failed to resolve app config directory: {err}"))
}

pub(crate) fn load_known_hosts(app: &AppHandle) -> Result<KnownHostsStore, String> {
    let path = known_hosts_path(app)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|err| format!("failed to parse {}: {err}", path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(KnownHostsStore::default()),
        Err(err) => Err(format!("failed to read {}: {err}", path.display())),
    }
}

pub(crate) fn save_known_hosts(app: &AppHandle, store: &KnownHostsStore) -> Result<(), String> {
    let path = known_hosts_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let json = serde_json::to_string_pretty(store).map_err(|err| err.to_string())?;
    atomic_write(&path, json.as_bytes()).map_err(|err| err.to_string())
}

#[cfg(test)]
#[path = "sftp_known_hosts_tests.rs"]
mod tests;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml sftp_known_hosts`
Expected: PASS (4 tests, including the always-accept regression guard)

- [ ] **Step 5: Wire `check_server_key` and the four SFTP commands in `sftp_bridge.rs`, only now registering them**

Implement `russh::client::Handler` for a small `SftpClientHandler` struct whose `check_server_key` calls `evaluate_host_key` against the loaded `KnownHostsStore`, returning success only on `TrustedMatch` and a hard error on `Mismatch`; `NewHostNeedsPrompt` is handled one level up in `sftp_connect` (which loads the store, attempts the handshake, and on a `NewHostNeedsPrompt` from a pre-flight fingerprint check returns `SftpConnectResult::PendingTrust` to the frontend instead of completing the connection — the actual `pin_host_key` + `save_known_hosts` call only happens after the frontend re-invokes `sftp_connect` with `trustNewHost: true` following user confirmation in `SftpTrustPrompt.tsx`).

Then, and only now, add to `lib.rs`:

```rust
mod sftp_bridge;
mod sftp_known_hosts;
// ... in invoke_handler![...]:
sftp_bridge::sftp_connect,
sftp_bridge::sftp_list_dir,
sftp_bridge::sftp_read_file,
sftp_bridge::sftp_disconnect,
sftp_bridge::sftp_list_known_hosts,
sftp_bridge::sftp_revoke_known_host,
```

- [ ] **Step 6: Run the full Rust test suite for both modules to verify nothing regressed**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml sftp`
Expected: PASS (all `sftp_bridge` + `sftp_known_hosts` tests)

- [ ] **Step 7: Commit**

```bash
cd apps/palette-tauri
git add src-tauri/src/sftp_known_hosts.rs src-tauri/src/sftp_known_hosts_tests.rs src-tauri/src/sftp_bridge.rs src-tauri/src/lib.rs
git commit -m "feat(palette): add TOFU host-key verification and register SFTP commands"
```

### Task 5c: `PaletteSettings` extension + `private_key_path` local-path rigor + settings blast-radius note

**Files:**
- Modify: `apps/palette-tauri/src-tauri/src/lib.rs` (extend `PaletteSettings`)
- Modify: `apps/palette-tauri/src-tauri/src/persistence.rs` (wire the new field through load/save/default)
- Create: `apps/palette-tauri/src/lib/sftpModel.ts`
- Create: `apps/palette-tauri/src/lib/sftpModel.test.ts`

**Settings blast-radius note (required by review):** `settings.json` is currently plaintext with no special file-mode handling beyond the app config directory's own OS-default permissions. Persisting `host`/`username`/`private_key_path` triples there creates a machine-readable "SSH access targeting map" — an attacker who reads this one file learns every remote host this user's palette can reach and which local key unlocks each one, which is higher-value reconnaissance than a bare `~/.ssh/config` (that file at least requires separately correlating keys to hosts, and doesn't centralize "the palette's full known SFTP estate" in one JSON blob). **Decision:** apply `0o600` (owner-read-write-only) permissions specifically when `settings.json` contains a non-empty `sftp_connections` array, using the same `#[cfg(unix)]`-gated `set_permissions` call pattern as the private-key-path world-readability warning in Task 5a. This is a targeted tightening (not a blanket settings.json permission change, since the rest of settings.json's contents — theme, shortcut, server URL — are lower sensitivity) — implemented in `write_settings` right after the existing `atomic_write` call. Document this decision inline in `persistence.rs` at the write site.

- [ ] **Step 1: Write a failing Rust test for the settings.json permission tightening**

```rust
// Add to the existing persistence.rs test sidecar (persistence_tests.rs — confirm
// the exact existing sidecar filename by checking persistence.rs's #[path] attribute
// before adding; do not create a second sidecar if one already covers this module)

#[cfg(unix)]
#[test]
fn write_settings_tightens_permissions_when_sftp_connections_present() {
    use std::os::unix::fs::PermissionsExt;
    let (app, settings) = test_settings_with_one_sftp_connection();
    write_settings(&app, &settings).unwrap();
    let path = settings_path_for_test(&app);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[cfg(unix)]
#[test]
fn write_settings_leaves_default_permissions_when_no_sftp_connections() {
    use std::os::unix::fs::PermissionsExt;
    let (app, settings) = test_settings_with_no_sftp_connections();
    write_settings(&app, &settings).unwrap();
    let path = settings_path_for_test(&app);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_ne!(mode, 0o600, "should not narrow permissions for settings with no sensitive SFTP data");
}
```

(`test_settings_with_one_sftp_connection`/`test_settings_with_no_sftp_connections`/`settings_path_for_test` are small test-only helpers to add alongside these tests, following whatever existing `AppHandle` test-fixture pattern `persistence_tests.rs` already uses — check the file for the established mock/test-app pattern before adding new helpers.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml persistence`
Expected: FAIL — `sftp_connections` field does not exist on `PaletteSettings`, permission tightening not implemented.

- [ ] **Step 3: Extend `PaletteSettings` and wire the field through `persistence.rs`**

```rust
// lib.rs — add to struct PaletteSettings:
sftp_connections: Vec<SftpConnectionProfile>,

// New type, near PaletteSettings:
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SftpConnectionProfile {
    pub id: String,
    pub label: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub private_key_path: String,
}
```

Update `PaletteSettings::default()`/wherever the struct is constructed (grep for every existing construction site the same way the root `CLAUDE.md`'s "Adding fields to `Config` struct" gotcha describes) to include `sftp_connections: Vec::new()`.

In `persistence.rs`, add the permission-tightening logic to `write_settings`:

```rust
pub(crate) fn write_settings(
    app: &AppHandle,
    settings: &PaletteSettings,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut palette_only = settings.clone();
    palette_only.env_values.clear();
    palette_only.config_values.clear();
    atomic_write(
        &path,
        serde_json::to_string_pretty(&palette_only)?.as_bytes(),
    )?;
    // Blast-radius tightening: settings.json carrying SFTP connection
    // profiles (host/username/private_key_path triples) is a machine-readable
    // "SSH access targeting map" — higher-value reconnaissance than a bare
    // ~/.ssh/config, since it centralizes every remote host this palette can
    // reach plus which local key unlocks each one. Narrow to owner-only when
    // that data is actually present; leave default permissions otherwise
    // since the rest of settings.json (theme, shortcut, server URL) is lower
    // sensitivity and a blanket 0600 would be an unjustified UX surprise for
    // users who never touch SFTP.
    #[cfg(unix)]
    if !palette_only.sftp_connections.is_empty() {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml persistence`
Expected: PASS

- [ ] **Step 5: Write failing tests for `sftpModel.ts`**

```typescript
// apps/palette-tauri/src/lib/sftpModel.test.ts
import { describe, expect, it } from "vitest";
import { createEmptyConnectionDraft, isValidConnectionDraft, type SftpConnectionDraft } from "./sftpModel";

describe("createEmptyConnectionDraft", () => {
  it("returns an empty draft with port 22", () => {
    const draft = createEmptyConnectionDraft();
    expect(draft).toEqual({ label: "", host: "", port: 22, username: "", privateKeyPath: "" });
  });
});

describe("isValidConnectionDraft", () => {
  it("requires host, username, and privateKeyPath", () => {
    const draft: SftpConnectionDraft = {
      label: "prod",
      host: "",
      port: 22,
      username: "deploy",
      privateKeyPath: "/home/me/.ssh/id_ed25519",
    };
    expect(isValidConnectionDraft(draft)).toBe(false);
  });

  it("accepts a fully filled draft", () => {
    const draft: SftpConnectionDraft = {
      label: "prod",
      host: "example.com",
      port: 22,
      username: "deploy",
      privateKeyPath: "/home/me/.ssh/id_ed25519",
    };
    expect(isValidConnectionDraft(draft)).toBe(true);
  });

  it("rejects a port outside 1-65535", () => {
    const draft: SftpConnectionDraft = {
      label: "prod",
      host: "example.com",
      port: 0,
      username: "deploy",
      privateKeyPath: "/home/me/.ssh/id_ed25519",
    };
    expect(isValidConnectionDraft(draft)).toBe(false);
  });
});
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/sftpModel.test.ts`
Expected: FAIL — module does not exist.

- [ ] **Step 7: Implement `sftpModel.ts`**

```typescript
// apps/palette-tauri/src/lib/sftpModel.ts
// Pure types/helpers for the SFTP connection-profile UI. Mirrors filesModel.ts's
// shape (types + pure helpers, no component logic) per the palette convention.

export interface SftpConnectionProfile {
  id: string;
  label: string;
  host: string;
  port: number;
  username: string;
  privateKeyPath: string;
}

export type SftpConnectionDraft = Omit<SftpConnectionProfile, "id">;

export function createEmptyConnectionDraft(): SftpConnectionDraft {
  return { label: "", host: "", port: 22, username: "", privateKeyPath: "" };
}

export function isValidConnectionDraft(draft: SftpConnectionDraft): boolean {
  return (
    draft.host.trim().length > 0 &&
    draft.username.trim().length > 0 &&
    draft.privateKeyPath.trim().length > 0 &&
    draft.port >= 1 &&
    draft.port <= 65535
  );
}

export interface SftpEntry {
  name: string;
  path: string;
  isDir: boolean;
  size: number;
  modifiedUnix?: number | null;
}

export interface SftpKnownHostEntry {
  host: string;
  port: number;
  keyType: string;
  fingerprint: string;
  firstSeenUnix: number;
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/lib/sftpModel.test.ts`
Expected: PASS (4 tests)

- [ ] **Step 9: Commit**

```bash
cd apps/palette-tauri
git add src-tauri/src/lib.rs src-tauri/src/persistence.rs src/lib/sftpModel.ts src/lib/sftpModel.test.ts
git commit -m "feat(palette): persist SFTP connection profiles with tightened settings.json permissions"
```

### Task 5d: Auto-persist-on-connect + trust prompt UI + tree integration

**Files:**
- Create: `apps/palette-tauri/src/components/palette/SftpConnectionDialog.tsx`
- Create: `apps/palette-tauri/src/components/palette/SftpConnectionDialog.test.tsx`
- Create: `apps/palette-tauri/src/components/palette/SftpTrustPrompt.tsx`
- Create: `apps/palette-tauri/src/components/palette/SftpTrustPrompt.test.tsx`
- Modify: `apps/palette-tauri/src/lib/filesViewState.ts` (add SFTP UI state slice + actions)
- Modify: `apps/palette-tauri/src/components/palette/FilesView.tsx` (render SFTP nodes inline in the tree, gate Edit/AI-edit for SFTP-origin entries)
- Modify: `apps/palette-tauri/src/components/palette/FilesView.test.tsx`
- Modify: `apps/palette-tauri/src/styles.css`

**Open Question #4 resolved:** auto-persist-on-first-successful-connect, matching common SSH client UX (most SSH/SFTP clients save a connection profile the moment it works, not behind a separate explicit "save" step). The wrinkle this creates: the profile is persisted to `settings.json` *before* the user has had a chance to evaluate whether they trust the connection long-term — but this is tightly coupled to the TOFU flow in Task 5b, since the trust prompt (fingerprint shown, explicit confirm) is itself the trust-evaluation moment; auto-persisting immediately after that confirmation is not meaningfully different from a separate "save profile?" dialog the user would click through anyway. If a user connects once, decides not to trust the host long-term, they can revoke via `sftp_revoke_known_host` and delete the profile from the connection list — both are cheap, discoverable actions, so a heavier explicit-save gate isn't justified.

**Interfaces:**
- Produces: reducer additions to `FilesViewState`/`FilesViewAction` — `sftp: { connections: SftpConnectionProfile[]; activeConnectionId: string | null; dialogOpen: boolean; editingProfile: SftpConnectionDraft | null; pendingTrust: SftpKnownHostEntry | null }` and actions `sftp/dialogOpen`, `sftp/dialogClose`, `sftp/connectionsLoaded`, `sftp/connectStart`, `sftp/connected`, `sftp/pendingTrust`, `sftp/trustConfirmed`, `sftp/disconnect`.
- Consumes: `sftp_connect`/`sftp_disconnect`/`sftp_list_known_hosts`/`sftp_revoke_known_host` from Task 5b; `SftpConnectionProfile`/`SftpConnectionDraft`/`SftpKnownHostEntry` from Task 5c's `sftpModel.ts`.

- [ ] **Step 1: Write failing tests for the trust-prompt component**

```typescript
// apps/palette-tauri/src/components/palette/SftpTrustPrompt.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SftpTrustPrompt } from "./SftpTrustPrompt";

const entry = { host: "example.com", port: 22, keyType: "ssh-ed25519", fingerprint: "AA:BB:CC", firstSeenUnix: 0 };

describe("SftpTrustPrompt", () => {
  it("shows the host and fingerprint", () => {
    render(<SftpTrustPrompt entry={entry} onTrust={vi.fn()} onCancel={vi.fn()} />);
    expect(screen.getByText(/example\.com/)).toBeInTheDocument();
    expect(screen.getByText(/AA:BB:CC/)).toBeInTheDocument();
  });

  it("calls onTrust when the user confirms", async () => {
    const onTrust = vi.fn();
    render(<SftpTrustPrompt entry={entry} onTrust={onTrust} onCancel={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: /trust/i }));
    expect(onTrust).toHaveBeenCalled();
  });

  it("calls onCancel without trusting when the user declines", async () => {
    const onTrust = vi.fn();
    const onCancel = vi.fn();
    render(<SftpTrustPrompt entry={entry} onTrust={onTrust} onCancel={onCancel} />);
    await userEvent.click(screen.getByRole("button", { name: /cancel/i }));
    expect(onCancel).toHaveBeenCalled();
    expect(onTrust).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/SftpTrustPrompt.test.tsx`
Expected: FAIL — component does not exist.

- [ ] **Step 3: Implement `SftpTrustPrompt.tsx`**

```tsx
// apps/palette-tauri/src/components/palette/SftpTrustPrompt.tsx
import { Button } from "@/components/ui/aurora/button";
import type { SftpKnownHostEntry } from "@/lib/sftpModel";

export function SftpTrustPrompt({
  entry,
  onTrust,
  onCancel,
}: {
  entry: SftpKnownHostEntry;
  onTrust: () => void;
  onCancel: () => void;
}) {
  return (
    <div className="sftp-trust-prompt" role="alertdialog" aria-label="Confirm SFTP host key">
      <p>
        First connection to <strong>{entry.host}:{entry.port}</strong>. This host's key will be
        remembered — future connections will fail if the key ever changes unexpectedly.
      </p>
      <p className="sftp-trust-fingerprint">
        {entry.keyType} · {entry.fingerprint}
      </p>
      <div className="sftp-trust-actions">
        <Button variant="ghost" size="sm" type="button" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="aurora" size="sm" type="button" onClick={onTrust}>
          Trust and connect
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/SftpTrustPrompt.test.tsx`
Expected: PASS (3 tests)

- [ ] **Step 5: Write failing tests for the connection dialog**

```typescript
// apps/palette-tauri/src/components/palette/SftpConnectionDialog.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { SftpConnectionDialog } from "./SftpConnectionDialog";
import { createEmptyConnectionDraft } from "@/lib/sftpModel";

describe("SftpConnectionDialog", () => {
  it("disables Connect until the draft is valid", async () => {
    render(
      <SftpConnectionDialog draft={createEmptyConnectionDraft()} onChange={vi.fn()} onSubmit={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.getByRole("button", { name: /connect/i })).toBeDisabled();
  });

  it("enables Connect once host, username, and key path are filled", () => {
    const draft = { label: "prod", host: "example.com", port: 22, username: "deploy", privateKeyPath: "/k" };
    render(<SftpConnectionDialog draft={draft} onChange={vi.fn()} onSubmit={vi.fn()} onClose={vi.fn()} />);
    expect(screen.getByRole("button", { name: /connect/i })).not.toBeDisabled();
  });

  it("calls onSubmit with the draft when Connect is clicked", async () => {
    const onSubmit = vi.fn();
    const draft = { label: "prod", host: "example.com", port: 22, username: "deploy", privateKeyPath: "/k" };
    render(<SftpConnectionDialog draft={draft} onChange={vi.fn()} onSubmit={onSubmit} onClose={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: /connect/i }));
    expect(onSubmit).toHaveBeenCalledWith(draft);
  });
});
```

- [ ] **Step 6: Run tests to verify they fail**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/SftpConnectionDialog.test.tsx`
Expected: FAIL — component does not exist.

- [ ] **Step 7: Implement `SftpConnectionDialog.tsx`**

```tsx
// apps/palette-tauri/src/components/palette/SftpConnectionDialog.tsx
import { Button } from "@/components/ui/aurora/button";
import { isValidConnectionDraft, type SftpConnectionDraft } from "@/lib/sftpModel";

export function SftpConnectionDialog({
  draft,
  onChange,
  onSubmit,
  onClose,
}: {
  draft: SftpConnectionDraft;
  onChange: (draft: SftpConnectionDraft) => void;
  onSubmit: (draft: SftpConnectionDraft) => void;
  onClose: () => void;
}) {
  const valid = isValidConnectionDraft(draft);
  return (
    <div className="sftp-connection-dialog" role="dialog" aria-label="Add SFTP connection">
      <label>
        Label
        <input value={draft.label} onChange={(e) => onChange({ ...draft, label: e.target.value })} />
      </label>
      <label>
        Host
        <input value={draft.host} onChange={(e) => onChange({ ...draft, host: e.target.value })} />
      </label>
      <label>
        Port
        <input
          type="number"
          value={draft.port}
          onChange={(e) => onChange({ ...draft, port: Number(e.target.value) })}
        />
      </label>
      <label>
        Username
        <input value={draft.username} onChange={(e) => onChange({ ...draft, username: e.target.value })} />
      </label>
      <label>
        Private key path
        <input
          value={draft.privateKeyPath}
          onChange={(e) => onChange({ ...draft, privateKeyPath: e.target.value })}
        />
      </label>
      <div className="sftp-connection-dialog-actions">
        <Button variant="ghost" size="sm" type="button" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="aurora" size="sm" type="button" disabled={!valid} onClick={() => onSubmit(draft)}>
          Connect
        </Button>
      </div>
    </div>
  );
}
```

- [ ] **Step 8: Run tests to verify they pass**

Run: `cd apps/palette-tauri && pnpm vitest run src/components/palette/SftpConnectionDialog.test.tsx`
Expected: PASS (3 tests)

- [ ] **Step 9: Wire SFTP state into the reducer and tree rendering; gate Edit/AI-edit for SFTP entries**

Extend `filesViewState.ts`'s `FilesViewState` with an `sftp` slice (declared as a stub in Task 1, filled in now) and add the actions listed in this sub-task's Interfaces block, following the exact pattern of the existing pane-scoped cases. In `FilesView.tsx`, render connected SFTP profiles' entries as extra root-level tree nodes alongside the local tree (not a separate `SftpTree.tsx` — matching the mock's single-`treeRows`-recursion UX, now generalized to N profiles instead of one hardcoded connection), each file entry carrying a `origin: "local" | "sftp"` tag used only to gate the per-file toolbar:

```tsx
{entry.origin === "sftp" ? null : (
  <Button variant="plain" size="unstyled" type="button" title="Edit file" aria-label="Edit file" onClick={...}>
    <Pencil size={14} />
  </Button>
)}
{entry.origin === "sftp" ? null : (
  <Button variant="plain" size="unstyled" type="button" title="Edit with the model" aria-label="Edit with the model" onClick={...}>
    <Sparkles size={14} />
  </Button>
)}
```

Write failing/passing tests in `FilesView.test.tsx` asserting that an SFTP-origin file's row renders neither the "Edit" nor "Edit with the model" button, following the same TDD pattern as prior tasks (failing test first, then the gating implementation, then green).

- [ ] **Step 10: Run the full frontend test suite**

Run: `cd apps/palette-tauri && pnpm vitest run`
Expected: PASS (all Files-view-related suites)

- [ ] **Step 11: Run the full Rust test suite**

Run: `cargo test --manifest-path apps/palette-tauri/src-tauri/Cargo.toml`
Expected: PASS

- [ ] **Step 12: Add SFTP UI CSS**

```css
.sftp-trust-prompt {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
}

.sftp-trust-fingerprint {
  font-family: var(--aurora-font-mono);
  font-size: 12px;
  color: var(--aurora-text-muted);
}

.sftp-trust-actions,
.sftp-connection-dialog-actions {
  display: flex;
  gap: 8px;
  justify-content: flex-end;
}

.sftp-connection-dialog {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px;
}
```

- [ ] **Step 13: Commit**

```bash
cd apps/palette-tauri
git add src/components/palette/SftpConnectionDialog.tsx src/components/palette/SftpConnectionDialog.test.tsx src/components/palette/SftpTrustPrompt.tsx src/components/palette/SftpTrustPrompt.test.tsx src/lib/filesViewState.ts src/components/palette/FilesView.tsx src/components/palette/FilesView.test.tsx src/styles.css
git commit -m "feat(palette): add SFTP connection dialog, TOFU trust prompt, and tree integration"
```

---

## Open Questions

- **LLM backend cost/latency for the AI-edit flow (Task 4):** each "Generate edit" click makes a real LLM completion call proxied through the user's configured Axon server. No rate-limiting or cost-guard is specified in this plan beyond the existing `AXON_LLM_COMPLETION_CONCURRENCY`/timeout knobs already governing `axon-core`'s completion dispatch. If this becomes a cost concern in practice, a follow-up should add a client-side debounce or a "this uses your configured LLM backend" disclosure in the sparkle prompt UI — out of scope for v1.
- **SFTP file-size/read caps:** `files_bridge.rs`'s local read path caps previews at 5 MiB (`MAX_TEXT_FILE_BYTES`). This plan does not specify whether `sftp_read_file` reuses the same constant or needs a smaller cap given added network latency per byte — decide during Task 5a implementation; either choice is reasonable, just make it explicit in the command's doc comment.
- **Known-hosts UI discoverability:** Task 5b/5d add `sftp_list_known_hosts`/`sftp_revoke_known_host` commands but this plan does not specify exactly where the "view/revoke pinned hosts" UI lives (a tab in `SftpConnectionDialog`, a separate settings panel section, etc.) — pick the lowest-friction placement during 5d implementation; it does not need its own dedicated component if it fits cleanly as a section within `SftpConnectionDialog`.
- **Multi-connection concurrent SFTP browsing:** this plan supports multiple persisted connection profiles, but does not specify a UI limit on how many can be simultaneously connected (each holds a live `SftpConnections` entry and an open SSH session). A reasonable v1 default is "one active SFTP connection at a time, switching profiles disconnects the previous one" — decide during 5d and document whichever choice is made in `SftpConnectionDialog.tsx`'s header comment.
