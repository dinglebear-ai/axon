import {
  ChevronRight,
  Columns2,
  FileArchive,
  FileCode,
  FileCog,
  File as FileIcon,
  FileText,
  Folder,
  Loader2,
  Plug,
  PlugZap,
  RefreshCw,
  Save,
  Sparkles,
  Upload,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";

import { Button } from "@/components/ui/aurora/button";
import { computeLineDiff, type AiEditProposal } from "@/lib/aiEditModel";
import { ACTIONS, type RemotePaletteAction } from "@/lib/actions";
import { type Client, executeAction, type PaletteConfig } from "@/lib/axonClient";
import {
  breadcrumbSegments,
  type DirListing,
  type FileContents,
  type FileEntry,
  type FilesPane,
  fileKind,
  formatBytes,
  formatModified,
  isChecked,
  isIngestable,
  isMarkdownLike,
  joinSegments,
  type PaneId,
  sortEntries,
} from "@/lib/filesModel";
import {
  createInitialState,
  filesViewReducer,
  MAX_TREE_WIDTH,
  MIN_TREE_WIDTH,
} from "@/lib/filesViewState";
import { invoke, isTauriRuntime } from "@/lib/invoke";
import { strField, unwrapPayload } from "@/lib/payload";
import {
  createEmptyConnectionDraft,
  type SftpConnectionDraft,
  type SftpConnectionProfile,
  type SftpEntry,
  type SftpKnownHostEntry,
} from "@/lib/sftpModel";
import { SftpConnectionDialog } from "./SftpConnectionDialog";
import { SftpTrustPrompt } from "./SftpTrustPrompt";

/** Cap on rendered AI-edit diff lines — see the render site's comment (P2 #9). */
const MAX_RENDERED_DIFF_LINES = 500;

/** Mirrors `MAX_SFTP_DIR_ENTRIES` in sftp_bridge/commands.rs — used only for
 * the truncation hint text, not enforcement (the backend is authoritative). */
const MAX_SFTP_DIR_ENTRIES_HINT = 2000;

interface FilesViewProps {
  client: Client | null;
  config: PaletteConfig | null;
}

type IngestState =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; ok: boolean; message: string };

type BulkIngestState =
  | { kind: "idle" }
  | { kind: "running"; done: number; total: number }
  | { kind: "done"; succeeded: number; failed: number; failedPaths: string[] }
  | { kind: "cancelled"; done: number; total: number };

/** Above this many checked files, `bulkIngest` asks for confirmation before
 * running — a large batch that partially fails is hard to diagnose/retry, so
 * this is a speed bump, not a hard limit (see P2 #11). */
const BULK_INGEST_CONFIRM_THRESHOLD = 200;

/**
 * Local filesystem browser + preview/edit + ingest. Owns its own
 * navigation/selection/edit state via `filesViewReducer` (see
 * `src/lib/filesViewState.ts`) and calls the Tauri fs bridge
 * (`files_list_dir` / `files_read_file` / `files_write_file`) directly via the
 * shared `invoke()` wrapper. In the browser-dev fallback (no Tauri runtime)
 * those commands have no meaningful implementation, so this renders a clear
 * "requires the desktop app" message instead of attempting fs calls that will
 * always throw.
 *
 * Supports an optional split view (two panes side by side), each with
 * independent navigation/selection/edit state, plus a resizable local
 * file-tree column shared across both panes.
 */
export function FilesView({ client, config }: FilesViewProps) {
  const [state, dispatch] = useReducer(filesViewReducer, undefined, createInitialState);
  const loadGenRef = useRef<Record<PaneId, number>>({ left: 0, right: 0 });
  const [ingestByPane, setIngestByPane] = useState<Record<PaneId, IngestState>>({
    left: { kind: "idle" },
    right: { kind: "idle" },
  });
  // Bulk-ingest progress is kept as component-local state (not a reducer
  // action) since it's a one-shot async operation's progress rather than a
  // persistent UI mode — the reducer models durable view state (panes,
  // selection, checked set), not ephemeral in-flight operation feedback.
  const [bulkIngestState, setBulkIngestState] = useState<BulkIngestState>({ kind: "idle" });
  const bulkIngestCancelRef = useRef(false);
  // SFTP is v1 read-only browsing: a single active connection at a time (see
  // Task 5d's Open Question resolution — connecting a new profile
  // disconnects the previous one), rendered as one extra tree section
  // alongside the local tree. Kept as component-local state (not part of the
  // reducer's per-pane state) since it's shared across both panes when
  // split, mirroring `state.sftp`'s "global to the view" scope.
  const [sftpCwd, setSftpCwd] = useState("");
  const [sftpEntries, setSftpEntries] = useState<SftpEntry[]>([]);
  const [sftpSelected, setSftpSelected] = useState<SftpEntry | null>(null);
  // Set when the backend truncated a directory listing at MAX_SFTP_DIR_ENTRIES
  // (see sftp_bridge/commands.rs) — surfaced so a truncated remote listing
  // doesn't silently look like a complete one.
  const [sftpTruncated, setSftpTruncated] = useState(false);
  const splitOpen = state.panes.length === 2;

  const loadDir = useCallback((id: PaneId, path: string) => {
    dispatch({ type: "pane/listingLoading", pane: id });
    invoke<DirListing>("files_list_dir", { path: path || null })
      .then((value) => dispatch({ type: "pane/listingLoaded", pane: id, listing: value }))
      .catch((err) =>
        dispatch({ type: "pane/listingError", pane: id, message: errorMessage(err) }),
      );
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

  // Intentionally keyed on the joined "id:cwd" signature (not state.panes
  // itself) so this only re-runs when a pane's id or cwd actually changes,
  // not on every dispatch that produces a new panes array reference (e.g.
  // selection or draft edits).
  // biome-ignore lint/correctness/useExhaustiveDependencies: see above.
  useEffect(() => {
    if (!isTauriRuntime) return;
    for (const pane of state.panes) {
      loadDir(pane.id, pane.cwd);
    }
  }, [state.panes.map((p) => `${p.id}:${p.cwd}`).join("|"), loadDir]);

  // Hydrate persisted SFTP connection profiles from settings.json on mount
  // (round-tripping the `sftp_connections` field the Rust side already
  // persists — see lib.rs's `PaletteSettings`). Without this the reducer's
  // `sftp/connectionsLoaded` action was dead code and every profile saved by
  // a prior session was invisible until reconnect.
  useEffect(() => {
    if (config?.sftpConnections?.length) {
      dispatch({ type: "sftp/connectionsLoaded", connections: config.sftpConnections });
    }
    // Only re-hydrate when the loaded config's connection list identity
    // changes (e.g. settings reloaded), not on every unrelated config field
    // edit — config is otherwise replaced wholesale on every settings save.
  }, [config?.sftpConnections]);

  const persistSftpConnections = useCallback(
    async (connections: SftpConnectionProfile[]) => {
      if (!isTauriRuntime || !config) return;
      try {
        await invoke("save_palette_settings", {
          settings: { ...config, sftpConnections: connections },
        });
      } catch {
        // Best-effort: a failed persist just means the profile won't survive
        // a restart. The live connection itself is unaffected.
      }
    },
    [config],
  );

  const loadSftpDir = useCallback((connectionId: string, path: string) => {
    invoke<{ path: string; entries: SftpEntry[]; truncated?: boolean }>("sftp_list_dir", {
      connectionId,
      path: path || null,
    })
      .then((listing) => {
        setSftpCwd(listing.path);
        setSftpEntries(listing.entries);
        setSftpTruncated(Boolean(listing.truncated));
      })
      .catch(() => {
        setSftpEntries([]);
        setSftpTruncated(false);
      });
  }, []);

  async function connectSftp(draft: SftpConnectionDraft, trustNewHost = false) {
    // Disconnect any previously active connection FIRST, and wait for it —
    // v1 supports one active SFTP connection at a time (see Task 5d's Open
    // Question resolution), and the Rust side now hard-rejects a new
    // `sftp_connect` while a session is still open (see
    // sftp_bridge/commands.rs). Disconnecting after would race that cap.
    if (state.sftp.activeConnectionId) {
      await invoke("sftp_disconnect", { connectionId: state.sftp.activeConnectionId }).catch(() => {});
    }

    const result = await invoke<
      | { kind: "connected"; connectionId: string }
      | { kind: "pendingTrust"; entry: SftpKnownHostEntry }
    >("sftp_connect", {
      profile: {
        host: draft.host,
        port: draft.port,
        username: draft.username,
        privateKeyPath: draft.privateKeyPath,
        trustNewHost,
      },
    }).catch((err) => {
      dispatch({ type: "sftp/dialogClose" });
      throw err;
    });

    if (result.kind === "pendingTrust") {
      dispatch({ type: "sftp/pendingTrust", entry: result.entry });
      return;
    }

    const profile: SftpConnectionProfile = {
      id: `${draft.host}:${draft.port}:${draft.username}`,
      label: draft.label,
      host: draft.host,
      port: draft.port,
      username: draft.username,
      privateKeyPath: draft.privateKeyPath,
    };
    dispatch({ type: "sftp/connected", connectionId: result.connectionId, profile });
    setSftpSelected(null);
    loadSftpDir(result.connectionId, "");
    // Persist the newly-connected profile so it survives an app restart (see
    // fix for P1 #3) — save_palette_prefs merges/writes sftp_connections
    // alongside the rest of settings.json.
    void persistSftpConnections([
      ...state.sftp.connections.filter((c) => c.id !== profile.id),
      profile,
    ]);
  }

  function disconnectSftp() {
    if (state.sftp.activeConnectionId) {
      void invoke("sftp_disconnect", { connectionId: state.sftp.activeConnectionId }).catch(() => {});
    }
    dispatch({ type: "sftp/disconnect" });
    setSftpCwd("");
    setSftpEntries([]);
    setSftpSelected(null);
  }

  function openSftpEntry(entry: SftpEntry) {
    if (!state.sftp.activeConnectionId) return;
    if (entry.isDir) {
      loadSftpDir(state.sftp.activeConnectionId, entry.path);
      setSftpSelected(null);
      return;
    }
    setSftpSelected(entry);
    // Capture the target pane and bump/capture `gen` BEFORE the async read
    // starts — mirrors `loadFile`'s exact pattern. Reading `loadGenRef`
    // inside the `.then()` (the previous bug) reads whatever generation is
    // current when the read *resolves*, not when it *started* — so a stale
    // remote read that resolves after a newer local file was opened would
    // read the newer gen and pass the staleness check, silently overwriting
    // the newer selection instead of being dropped as superseded.
    const pane = state.activePane;
    const gen = loadGenRef.current[pane] + 1;
    loadGenRef.current[pane] = gen;
    dispatch({ type: "pane/fileLoading", pane, loadGen: gen });
    invoke<{ path: string; content: string }>("sftp_read_file", {
      connectionId: state.sftp.activeConnectionId,
      path: entry.path,
    })
      .then((file) => {
        dispatch({
          type: "pane/select",
          pane,
          entry: { name: entry.name, path: entry.path, isDir: false, size: entry.size, origin: "sftp" },
        });
        dispatch({
          type: "pane/fileLoaded",
          pane,
          loadGen: gen,
          file: { path: entry.path, content: file.content, size: entry.size },
        });
      })
      .catch((err) =>
        dispatch({
          type: "pane/fileError",
          pane,
          loadGen: gen,
          message: errorMessage(err),
        }),
      );
  }

  function openEntry(id: PaneId, entry: FileEntry) {
    if (entry.isDir) {
      dispatch({ type: "pane/setCwd", pane: id, cwd: entry.path });
      return;
    }
    dispatch({ type: "pane/select", pane: id, entry });
    loadFile(id, entry.path);
  }

  function goToBreadcrumb(id: PaneId, cwd: string, index: number) {
    const segments = breadcrumbSegments(cwd);
    dispatch({ type: "pane/setCwd", pane: id, cwd: joinSegments(segments.slice(0, index + 1)) });
  }

  function activatePane(id: PaneId) {
    if (splitOpen) dispatch({ type: "pane/setActive", pane: id });
  }

  async function saveFile(id: PaneId) {
    const pane = state.panes.find((p) => p.id === id);
    if (!pane?.selected) return;
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
      dispatch({
        type: "pane/fileError",
        pane: id,
        loadGen: loadGenRef.current[id],
        message: errorMessage(err),
      });
    } finally {
      dispatch({ type: "pane/setSaving", pane: id, saving: false });
    }
  }

  function resolveEmbedAction(): RemotePaletteAction | null {
    return (
      ACTIONS.find(
        (action): action is RemotePaletteAction =>
          action.subcommand === "embed" && action.kind !== "local",
      ) ?? null
    );
  }

  // The AI-edit "Edit with the model" flow reuses the palette's existing
  // `chat` action (POST /v1/chat — "Direct LLM chat answer", no RAG
  // retrieval) rather than a new Rust-side LLM proxy command: `chat` already
  // routes through the same `executeAction`/`axon_http_request` path as
  // ingest, and the alternative (`/v1/ask`) is a RAG-search endpoint that
  // would treat the file content as a search query instead of context to
  // transform — the wrong tool for "rewrite this file per this instruction."
  function resolveChatAction(): RemotePaletteAction | null {
    return (
      ACTIONS.find(
        (action): action is RemotePaletteAction =>
          action.subcommand === "chat" && action.kind !== "local",
      ) ?? null
    );
  }

  function buildEditPrompt(fileContent: string, instruction: string): string {
    return (
      "You are editing a single file. Apply exactly this instruction and " +
      "return the FULL new file content, with no commentary, no code " +
      `fences, and no explanation — only the raw file body.\n\nInstruction: ${instruction}` +
      `\n\nCurrent file content:\n${fileContent}`
    );
  }

  function setIngestResult(id: PaneId, next: IngestState) {
    setIngestByPane((prev) => ({ ...prev, [id]: next }));
  }

  async function ingestSelected(id: PaneId) {
    const pane = state.panes.find((p) => p.id === id);
    if (!pane?.selected || !client || !config) return;
    const embedAction = resolveEmbedAction();
    if (!embedAction) {
      setIngestResult(id, { kind: "done", ok: false, message: "Embed action is unavailable." });
      return;
    }
    const listing = state.listings[id];
    const root = listing.kind === "loaded" ? listing.value.root : "";
    const absolutePath = `${root.replace(/\/+$/, "")}/${pane.selected.path}`;
    setIngestResult(id, { kind: "running" });
    const result = await executeAction(client, embedAction, absolutePath, config);
    if (result.ok) {
      setIngestResult(id, { kind: "done", ok: true, message: "Queued for ingest." });
    } else {
      const payload = unwrapPayload(result.payload);
      const message =
        strField(payload, "message") ??
        strField(payload, "error") ??
        `Ingest failed (HTTP ${result.status}).`;
      setIngestResult(id, { kind: "done", ok: false, message });
    }
  }

  async function bulkIngest() {
    if (!client || !config) return;
    const embedAction = resolveEmbedAction();
    if (!embedAction) return;
    const leftListing = state.listings.left;
    const root = leftListing.kind === "loaded" ? leftListing.value.root : "";
    const paths = Array.from(state.checked);
    // A large batch that partially fails is hard to diagnose/retry — ask for
    // confirmation above the threshold rather than silently kicking off a
    // huge sequential run (see P2 #11). window.confirm is a deliberate
    // minimal choice here — no new modal component for a one-off guard.
    if (
      paths.length > BULK_INGEST_CONFIRM_THRESHOLD &&
      !window.confirm(
        `Ingest ${paths.length} files? This runs sequentially and may take a while.`,
      )
    ) {
      return;
    }
    bulkIngestCancelRef.current = false;
    setBulkIngestState({ kind: "running", done: 0, total: paths.length });
    let succeeded = 0;
    let failed = 0;
    let done = 0;
    const failedPaths: string[] = [];
    for (const [index, path] of paths.entries()) {
      if (bulkIngestCancelRef.current) {
        setBulkIngestState({ kind: "cancelled", done, total: paths.length });
        return;
      }
      // Show the in-flight item (1-indexed) as "done" while its request is
      // outstanding, not just after it resolves — "Ingesting 1/2..." means
      // "working on item 1 of 2", matching the mock's progress-label shape.
      setBulkIngestState({ kind: "running", done: index + 1, total: paths.length });
      const absolutePath = `${root.replace(/\/+$/, "")}/${path}`;
      // Sequential (concurrency 1) is the deliberate v1 choice — the embed
      // endpoint is confirmed synchronous server-side (see the
      // axon-phase10-source-migration-gaps memory note), so a naive
      // concurrency guess would just queue requests the server processes one
      // at a time anyway. Revisit only after a real load test.
      const result = await executeAction(client, embedAction, absolutePath, config);
      if (result.ok) {
        succeeded += 1;
      } else {
        failed += 1;
        failedPaths.push(path);
      }
      done += 1;
    }
    setBulkIngestState({ kind: "done", succeeded, failed, failedPaths });
    dispatch({ type: "checked/clear" });
  }

  async function submitSparkleQuery(id: PaneId) {
    const pane = state.panes.find((p) => p.id === id);
    if (!pane?.sparkleQuery.trim() || pane.file.kind !== "loaded" || !pane.selected) return;
    if (!client || !config) {
      dispatch({
        type: "pane/proposalError",
        pane: id,
        message: "Connect to an Axon server to use AI-assisted edits.",
      });
      return;
    }
    const chatAction = resolveChatAction();
    if (!chatAction) {
      dispatch({ type: "pane/proposalError", pane: id, message: "Chat action is unavailable." });
      return;
    }
    dispatch({ type: "pane/proposalPending", pane: id });
    const prompt = buildEditPrompt(pane.file.value.content, pane.sparkleQuery);
    const result = await executeAction(client, chatAction, prompt, config);
    if (!result.ok) {
      const payload = unwrapPayload(result.payload);
      const message =
        strField(payload, "message") ??
        strField(payload, "error") ??
        `Edit generation failed (HTTP ${result.status}).`;
      dispatch({ type: "pane/proposalError", pane: id, message });
      return;
    }
    const payload = unwrapPayload(result.payload);
    const proposedContent = strField(payload, "answer");
    if (proposedContent == null) {
      dispatch({
        type: "pane/proposalError",
        pane: id,
        message: "The model did not return a rewritten file body.",
      });
      return;
    }
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
  }

  async function approveProposal(id: PaneId) {
    const pane = state.panes.find((p) => p.id === id);
    if (!pane?.proposal || !pane.selected) return;
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

  // Sorting the (potentially 1,000+ entry) directory listing on every render
  // — regardless of whether the listing itself changed — caused visible
  // input lag elsewhere in the view (e.g. a checkbox toggle re-sorting the
  // whole tree). Memoized per pane, keyed on the listing object reference
  // (stable per the loadGen-guarded reducer, so this only recomputes when a
  // pane's listing actually changes).
  const sortedEntriesByPane: Record<PaneId, FileEntry[]> = {
    left: useMemo(
      () =>
        state.listings.left.kind === "loaded" ? sortEntries(state.listings.left.value.entries) : [],
      [state.listings.left],
    ),
    right: useMemo(
      () =>
        state.listings.right.kind === "loaded" ? sortEntries(state.listings.right.value.entries) : [],
      [state.listings.right],
    ),
  };

  if (!isTauriRuntime) {
    return (
      <div className="output-body operation-view files-view aurora-scrollbar">
        <div className="files-unavailable">
          <FileIcon size={22} strokeWidth={1.5} />
          <p>Files requires the desktop app.</p>
          <p className="operation-muted">
            Local filesystem access is only available when running the Axon Palette as a Tauri
            desktop app, not in the browser dev preview.
          </p>
        </div>
      </div>
    );
  }

  function renderPane(pane: FilesPane) {
    const listing = state.listings[pane.id];
    const entries = sortedEntriesByPane[pane.id];
    const segments = breadcrumbSegments(pane.cwd);
    const ingest = ingestByPane[pane.id];

    // Pane activation on mousedown is a pointer-only convenience for the split
    // view (mirrors clicking anywhere in a window to focus it) — the pane's
    // own focusable controls (rows, buttons, textarea) remain independently
    // keyboard-reachable via normal tab order, so no keyboard equivalent is
    // lost here.
    return (
      // biome-ignore lint/a11y/noStaticElementInteractions: see comment above.
      <div
        key={pane.id}
        className="files-pane"
        style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}
        onMouseDown={() => activatePane(pane.id)}
      >
        <div className="files-toolbar">
          <nav className="files-breadcrumb" aria-label="Current directory">
            <Button
              variant="plain"
              size="unstyled"
              type="button"
              onClick={() => dispatch({ type: "pane/setCwd", pane: pane.id, cwd: "" })}
            >
              ~
            </Button>
            {segments.map((segment, index) => (
              <span
                key={segments.slice(0, index + 1).join("/")}
                className="files-breadcrumb-segment"
              >
                <ChevronRight size={12} />
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  onClick={() => goToBreadcrumb(pane.id, pane.cwd, index)}
                >
                  {segment}
                </Button>
              </span>
            ))}
          </nav>
          {pane.id === "left" && (
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
          )}
          {pane.id === "left" && (
            <Button
              variant="plain"
              size="unstyled"
              type="button"
              title={state.sftp.activeConnectionId ? "Disconnect SFTP" : "Connect SFTP"}
              aria-label={state.sftp.activeConnectionId ? "Disconnect SFTP" : "Connect SFTP"}
              onClick={() =>
                state.sftp.activeConnectionId
                  ? disconnectSftp()
                  : dispatch({ type: "sftp/dialogOpen", draft: createEmptyConnectionDraft() })
              }
            >
              {state.sftp.activeConnectionId ? <PlugZap size={14} /> : <Plug size={14} />}
            </Button>
          )}
          <Button
            variant="plain"
            size="unstyled"
            type="button"
            onClick={() => loadDir(pane.id, pane.cwd)}
            title="Refresh"
            aria-label="Refresh directory listing"
          >
            <RefreshCw size={14} />
          </Button>
        </div>
        <div className="files-body">
          <div
            className="files-tree aurora-scrollbar"
            role="listbox"
            aria-label="Directory entries"
            style={{ width: state.treeWidth, flex: `0 0 ${state.treeWidth}px` }}
          >
            {listing.kind === "loading" ? (
              <div className="files-empty">
                <Loader2 size={16} className="files-spin" />
                <span>Loading...</span>
              </div>
            ) : listing.kind === "error" ? (
              <div className="files-empty operation-muted">{listing.message}</div>
            ) : entries.length === 0 ? (
              <div className="files-empty operation-muted">Empty directory</div>
            ) : (
              entries.map((entry) => (
                <button
                  key={entry.path}
                  type="button"
                  role="option"
                  aria-selected={pane.selected?.path === entry.path}
                  className={`files-row${pane.selected?.path === entry.path ? " files-row-active" : ""}`}
                  onClick={() => openEntry(pane.id, entry)}
                >
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
                  <EntryIcon entry={entry} />
                  <span className="files-row-name">{entry.name}</span>
                  {!entry.isDir && (
                    <span className="files-row-size">{formatBytes(entry.size)}</span>
                  )}
                </button>
              ))
            )}
            {pane.id === "left" && state.sftp.activeConnectionId && (
              <div className="files-sftp-section">
                <div className="files-sftp-section-header">
                  <span
                    className="files-sftp-connected-dot"
                    aria-hidden="true"
                    title="Connected"
                  />
                  SFTP · {state.sftp.connections.find((c) => c.id === state.sftp.activeConnectionId)?.label}
                  {sftpCwd && <span className="files-sftp-cwd"> · /{sftpCwd}</span>}
                </div>
                {sftpEntries.length === 0 ? (
                  <div className="files-empty operation-muted">Empty directory</div>
                ) : (
                  sftpEntries.map((entry) => (
                    <button
                      key={entry.path}
                      type="button"
                      role="option"
                      aria-selected={sftpSelected?.path === entry.path}
                      className={`files-row files-row-sftp${sftpSelected?.path === entry.path ? " files-row-active" : ""}`}
                      onClick={() => openSftpEntry(entry)}
                    >
                      <EntryIcon entry={{ ...entry, origin: "sftp" }} />
                      <span className="files-row-name">{entry.name}</span>
                      {!entry.isDir && (
                        <span className="files-row-size">{formatBytes(entry.size)}</span>
                      )}
                    </button>
                  ))
                )}
                {sftpTruncated && (
                  <div className="files-empty operation-muted">
                    Listing truncated at {MAX_SFTP_DIR_ENTRIES_HINT} entries
                  </div>
                )}
              </div>
            )}
          </div>
          <div className="files-preview aurora-scrollbar">
            {!pane.selected ? (
              <div className="files-empty operation-muted">Select a file</div>
            ) : pane.file.kind === "loading" ? (
              <div className="files-empty">
                <Loader2 size={16} className="files-spin" />
                <span>Loading...</span>
              </div>
            ) : pane.file.kind === "error" ? (
              <div className="files-empty operation-muted">{pane.file.message}</div>
            ) : pane.file.kind === "loaded" ? (
              <FilePreview
                selectedPath={pane.selected.path}
                modifiedUnix={pane.selected.modifiedUnix}
                file={pane.file.value}
                editing={pane.editing}
                draft={pane.draft}
                saving={pane.saving}
                ingest={ingest}
                canIngest={Boolean(client && config) && isIngestable(pane.selected.name)}
                canEdit={pane.selected.origin !== "sftp"}
                onEdit={() => dispatch({ type: "pane/setEditing", pane: pane.id, editing: true })}
                onCancelEdit={() => {
                  dispatch({ type: "pane/setEditing", pane: pane.id, editing: false });
                  if (pane.file.kind === "loaded") {
                    dispatch({
                      type: "pane/setDraft",
                      pane: pane.id,
                      draft: pane.file.value.content,
                    });
                  }
                }}
                onDraftChange={(value) =>
                  dispatch({ type: "pane/setDraft", pane: pane.id, draft: value })
                }
                onSave={() => void saveFile(pane.id)}
                onIngest={() => void ingestSelected(pane.id)}
                sparkleOpen={pane.sparkleOpen}
                sparkleQuery={pane.sparkleQuery}
                proposal={pane.proposal}
                proposalState={pane.proposalState}
                proposalErrorMessage={pane.proposalErrorMessage}
                onSparkleToggle={() =>
                  dispatch(
                    pane.sparkleOpen
                      ? { type: "pane/sparkleClose", pane: pane.id }
                      : { type: "pane/sparkleOpen", pane: pane.id },
                  )
                }
                onSparkleQueryChange={(value) =>
                  dispatch({ type: "pane/sparkleQueryChange", pane: pane.id, query: value })
                }
                onSparkleSubmit={() => void submitSparkleQuery(pane.id)}
                onProposalDeny={() => dispatch({ type: "pane/proposalDeny", pane: pane.id })}
                onProposalApprove={() => void approveProposal(pane.id)}
              />
            ) : null}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="output-body operation-view files-view">
      {state.checked.size > 0 && (
        <div className="files-bulk-bar">
          <span>{state.checked.size} selected</span>
          {bulkIngestState.kind !== "running" && (
            <Button
              variant="ghost"
              size="sm"
              type="button"
              onClick={() => dispatch({ type: "checked/clear" })}
            >
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
            <Button
              variant="ghost"
              size="sm"
              type="button"
              onClick={() => {
                bulkIngestCancelRef.current = true;
              }}
            >
              Cancel
            </Button>
          )}
          {bulkIngestState.kind === "cancelled" && (
            <span className="files-bulk-status">
              Cancelled after {bulkIngestState.done}/{bulkIngestState.total}
            </span>
          )}
          {bulkIngestState.kind === "done" && (
            <span className="files-bulk-status">
              {bulkIngestState.succeeded} succeeded
              {bulkIngestState.failed > 0 && `, ${bulkIngestState.failed} failed`}
            </span>
          )}
        </div>
      )}
      {bulkIngestState.kind === "done" && bulkIngestState.failedPaths.length > 0 && (
        // Surfacing the actual failed paths (not just a bare count) so a
        // partial-failure run can be diagnosed/retried — see P2 #11.
        <div className="files-bulk-failures operation-muted">
          <span>Failed to ingest:</span>
          <ul>
            {bulkIngestState.failedPaths.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </div>
      )}
      <div className="files-split-container">
        {/* A semantic <hr> can't carry pointer-drag/keyboard-resize handlers
            or aria-valuenow, so this stays a div with an explicit separator
            role — the WAI-ARIA "window splitter" pattern. */}
        {/* biome-ignore lint/a11y/useSemanticElements: see comment above. */}
        <div
          className="files-tree-resize"
          role="separator"
          tabIndex={0}
          aria-label="Resize file tree"
          aria-orientation="vertical"
          aria-valuenow={state.treeWidth}
          aria-valuemin={MIN_TREE_WIDTH}
          aria-valuemax={MAX_TREE_WIDTH}
          onMouseDown={startResize}
          onKeyDown={(event) => {
            if (event.key === "ArrowLeft") {
              dispatch({ type: "treeWidth/set", width: state.treeWidth - 10 });
            } else if (event.key === "ArrowRight") {
              dispatch({ type: "treeWidth/set", width: state.treeWidth + 10 });
            }
          }}
        />
        {renderPane(state.panes[0])}
        {splitOpen && state.panes[1] && renderPane(state.panes[1])}
      </div>
      {state.sftp.dialogOpen && state.sftp.editingProfile && (
        <SftpConnectionDialog
          draft={state.sftp.editingProfile}
          onChange={(draft) => dispatch({ type: "sftp/dialogOpen", draft })}
          onSubmit={(draft) => void connectSftp(draft)}
          onClose={() => dispatch({ type: "sftp/dialogClose" })}
        />
      )}
      {state.sftp.pendingTrust && (
        <SftpTrustPrompt
          entry={state.sftp.pendingTrust}
          onTrust={() => {
            const draft = state.sftp.editingProfile;
            dispatch({ type: "sftp/trustConfirmed" });
            if (draft) void connectSftp(draft, true);
          }}
          onCancel={() => dispatch({ type: "sftp/trustConfirmed" })}
        />
      )}
    </div>
  );
}

function EntryIcon({ entry }: { entry: FileEntry }) {
  if (entry.isDir) return <Folder size={15} className="files-icon-dir" aria-hidden="true" />;
  const kind = fileKind(entry.name);
  switch (kind) {
    case "doc":
      return <FileText size={15} className="files-icon-doc" aria-hidden="true" />;
    case "code":
      return <FileCode size={15} className="files-icon-code" aria-hidden="true" />;
    case "config":
      return <FileCog size={15} className="files-icon-config" aria-hidden="true" />;
    case "archive":
      return <FileArchive size={15} className="files-icon-muted" aria-hidden="true" />;
    default:
      return <FileIcon size={15} className="files-icon-muted" aria-hidden="true" />;
  }
}

function FilePreview({
  selectedPath,
  modifiedUnix,
  file,
  editing,
  draft,
  saving,
  ingest,
  canIngest,
  canEdit,
  onEdit,
  onCancelEdit,
  onDraftChange,
  onSave,
  onIngest,
  sparkleOpen,
  sparkleQuery,
  proposal,
  proposalState,
  proposalErrorMessage,
  onSparkleToggle,
  onSparkleQueryChange,
  onSparkleSubmit,
  onProposalDeny,
  onProposalApprove,
}: {
  selectedPath: string;
  modifiedUnix?: number | null;
  file: FileContents;
  editing: boolean;
  draft: string;
  saving: boolean;
  ingest: IngestState;
  canIngest: boolean;
  /** SFTP is v1 read-only browsing: both the manual Edit button and the
   * "Edit with the model" sparkle button are hard-disabled (not rendered)
   * for any file whose pane resolves to an SFTP-origin entry. */
  canEdit: boolean;
  onEdit: () => void;
  onCancelEdit: () => void;
  onDraftChange: (value: string) => void;
  onSave: () => void;
  onIngest: () => void;
  sparkleOpen: boolean;
  sparkleQuery: string;
  proposal: AiEditProposal | null;
  proposalState: FilesPane["proposalState"];
  proposalErrorMessage: string | null;
  onSparkleToggle: () => void;
  onSparkleQueryChange: (value: string) => void;
  onSparkleSubmit: () => void;
  onProposalDeny: () => void;
  onProposalApprove: () => void;
}) {
  const name = selectedPath.split("/").pop() ?? selectedPath;
  return (
    <div className="files-preview-inner">
      <div className="files-preview-header">
        <span className="files-preview-name">{name}</span>
        <span className="files-preview-meta">
          {formatBytes(file.size)} · modified {formatModified(modifiedUnix)}
        </span>
        <div className="files-preview-actions">
          {editing ? (
            <>
              <Button variant="ghost" size="sm" type="button" onClick={onCancelEdit}>
                Cancel
              </Button>
              <Button variant="aurora" size="sm" type="button" onClick={onSave} disabled={saving}>
                <Save size={13} />
                {saving ? "Saving..." : "Save"}
              </Button>
            </>
          ) : (
            <>
              {canEdit && (
                <Button variant="ghost" size="sm" type="button" onClick={onEdit}>
                  Edit
                </Button>
              )}
              {canEdit && (
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  title="Edit with the model"
                  aria-label="Edit with the model"
                  onClick={onSparkleToggle}
                >
                  <Sparkles size={14} />
                </Button>
              )}
              {canIngest && (
                <Button
                  variant="aurora"
                  size="sm"
                  type="button"
                  onClick={onIngest}
                  disabled={ingest.kind === "running"}
                >
                  <Upload size={13} />
                  {ingest.kind === "running" ? "Ingesting..." : "Ingest"}
                </Button>
              )}
            </>
          )}
        </div>
      </div>
      {ingest.kind === "done" && (
        <div className={`files-ingest-status${ingest.ok ? "" : " files-ingest-status-error"}`}>
          {ingest.message}
        </div>
      )}
      {editing ? (
        <textarea
          className="files-editor"
          value={draft}
          onChange={(event) => onDraftChange(event.target.value)}
          spellCheck={false}
        />
      ) : isMarkdownLike(name) ? (
        <pre className="files-preview-text">{file.content}</pre>
      ) : (
        <pre className="files-preview-text files-preview-code">
          <code>{file.content}</code>
        </pre>
      )}
      {proposal ? (
        <div className="files-ai-edit-review">
          <p className="files-ai-edit-heading">
            Proposed edit · {proposal.diff.filter((l) => l.kind !== "same").length} lines
          </p>
          <pre className="files-ai-edit-body">
            {/* Diff lines have no stable identity of their own — line text can
                repeat, and the rendered order never reorders after the
                proposal is set — so an index key is safe here. */}
            {/* computeLineDiff is index-aligned, not LCS: a single top-of-file
                insertion cascades into marking the entire remainder of the
                file as removed+added. Without a cap, a several-thousand-line
                file renders one <div> per line with no virtualization and
                visibly janks. Capping the rendered count (not a full
                virtualization rewrite) keeps this proportionate — see P2 #9. */}
            {proposal.diff.slice(0, MAX_RENDERED_DIFF_LINES).map((line, index) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: see comment above.
              <div key={index} className={`files-ai-edit-line files-ai-edit-${line.kind}`}>
                <span className="files-ai-edit-marker">
                  {line.kind === "added" ? "+" : line.kind === "removed" ? "-" : " "}
                </span>
                {line.text}
              </div>
            ))}
            {proposal.diff.length > MAX_RENDERED_DIFF_LINES && (
              <div className="files-ai-edit-line files-ai-edit-truncated operation-muted">
                … {proposal.diff.length - MAX_RENDERED_DIFF_LINES} more lines not shown
              </div>
            )}
          </pre>
          {proposalState === "error" && proposalErrorMessage && (
            <p className="files-ai-edit-error">{proposalErrorMessage}</p>
          )}
          <div className="files-ai-edit-actions">
            <span className="files-ai-edit-note">The model proposes this edit — review it.</span>
            <Button variant="ghost" size="sm" type="button" onClick={onProposalDeny}>
              Deny
            </Button>
            <Button
              variant="rose"
              size="sm"
              type="button"
              onClick={onProposalApprove}
              disabled={proposalState === "approving"}
            >
              {proposalState === "approving" ? "Applying..." : "Approve"}
            </Button>
          </div>
        </div>
      ) : sparkleOpen ? (
        <div className="files-ai-edit-prompt">
          <Sparkles size={14} />
          {/* Autofocus is intentional here: this input only mounts when the
              user explicitly clicks "Edit with the model", so focusing it
              immediately (like a command-palette input opening) is expected,
              not a surprise page-load autofocus. */}
          <input
            // biome-ignore lint/a11y/noAutofocus: see comment above the input.
            autoFocus
            value={sparkleQuery}
            placeholder="Describe the edit — the model rewrites the file…"
            onChange={(event) => onSparkleQueryChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter" && sparkleQuery.trim()) onSparkleSubmit();
              if (event.key === "Escape") onSparkleToggle();
            }}
          />
          <Button
            variant="rose"
            size="icon"
            title="Generate edit"
            aria-label="Generate edit"
            type="button"
            onClick={onSparkleSubmit}
            disabled={proposalState === "pending"}
          >
            <Sparkles size={14} />
          </Button>
          {proposalState === "error" && proposalErrorMessage && (
            <p className="files-ai-edit-error">{proposalErrorMessage}</p>
          )}
        </div>
      ) : null}
    </div>
  );
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
