import { File as FileIcon } from "lucide-react";
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";

import { ACTIONS, type RemotePaletteAction } from "@/lib/actions";
import { createAiEditFlow } from "@/lib/aiEditFlow";
import { type Client, executeAction, type PaletteConfig } from "@/lib/axonClient";
import {
  breadcrumbSegments,
  type DirListing,
  type FileContents,
  type FileEntry,
  type FilesPane,
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
import { createEmptyConnectionDraft } from "@/lib/sftpModel";
import { createSftpLifecycle } from "@/lib/useSftpLifecycle";
import { type BulkIndexState, FilesBulkIndexBar } from "./FilesBulkIndexBar";
import { FilesPaneView } from "./FilesPaneView";
import { SftpConnectionDialog } from "./SftpConnectionDialog";
import type { SftpTreeSectionHandle } from "./SftpTreeSection";
import { SftpTrustPrompt } from "./SftpTrustPrompt";

interface FilesViewProps {
  client: Client | null;
  config: PaletteConfig | null;
}

type IndexState =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; ok: boolean; message: string };

/** Above this many checked files, `bulkIndex` asks for confirmation before
 * running — a large batch that partially fails is hard to diagnose/retry, so
 * this is a speed bump, not a hard limit (see P2 #11). */
const BULK_INDEX_CONFIRM_THRESHOLD = 200;

/**
 * Local filesystem browser + preview/edit + source indexing. Owns its own
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
 *
 * The AI-edit propose/approve UI lives in `AiEditPanel.tsx`, the bulk
 * checkbox-select indexing bar in `FilesBulkIndexBar.tsx`, and the SFTP
 * tree-browsing section in `SftpTreeSection.tsx` — this component composes
 * them and owns cross-cutting orchestration (pane state, load-gen guarding,
 * SFTP connect/disconnect lifecycle).
 */
export function FilesView({ client, config }: FilesViewProps) {
  const [state, dispatch] = useReducer(filesViewReducer, undefined, createInitialState);
  const loadGenRef = useRef<Record<PaneId, number>>({ left: 0, right: 0 });
  const [indexByPane, setIndexByPane] = useState<Record<PaneId, IndexState>>({
    left: { kind: "idle" },
    right: { kind: "idle" },
  });
  // Bulk-indexing progress is kept as component-local state (not a reducer
  // action) since it's a one-shot async operation's progress rather than a
  // persistent UI mode — the reducer models durable view state (panes,
  // selection, checked set), not ephemeral in-flight operation feedback.
  const [bulkIndexState, setBulkIndexState] = useState<BulkIndexState>({ kind: "idle" });
  const bulkIndexCancelRef = useRef(false);
  const sftpTreeRef = useRef<SftpTreeSectionHandle>(null);
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

  const { connectSftp, disconnectSftp, openSftpFile } = useMemo(
    () =>
      createSftpLifecycle({
        sftp: state.sftp,
        dispatch,
        isTauriRuntime,
        config,
        sftpTreeRef,
        loadGenRef,
        activePane: state.activePane,
      }),
    [state.sftp, state.activePane, config],
  );

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
          action.subcommand === "source" && action.kind !== "local",
      ) ?? null
    );
  }

  const { submitSparkleQuery, approveProposal } = useMemo(
    () => createAiEditFlow({ panes: state.panes, dispatch, client, config }),
    [state.panes, client, config],
  );

  function setIndexResult(id: PaneId, next: IndexState) {
    setIndexByPane((prev) => ({ ...prev, [id]: next }));
  }

  async function indexSelected(id: PaneId) {
    const pane = state.panes.find((p) => p.id === id);
    if (!pane?.selected || !client || !config) return;
    const embedAction = resolveEmbedAction();
    if (!embedAction) {
      setIndexResult(id, { kind: "done", ok: false, message: "Embed action is unavailable." });
      return;
    }
    const listing = state.listings[id];
    const root = listing.kind === "loaded" ? listing.value.root : "";
    const absolutePath = `${root.replace(/\/+$/, "")}/${pane.selected.path}`;
    setIndexResult(id, { kind: "running" });
    const result = await executeAction(client, embedAction, absolutePath, config);
    if (result.ok) {
      setIndexResult(id, { kind: "done", ok: true, message: "Queued for indexing." });
    } else {
      const payload = unwrapPayload(result.payload);
      const message =
        strField(payload, "message") ??
        strField(payload, "error") ??
        `Indexing failed (HTTP ${result.status}).`;
      setIndexResult(id, { kind: "done", ok: false, message });
    }
  }

  async function bulkIndex() {
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
      paths.length > BULK_INDEX_CONFIRM_THRESHOLD &&
      !window.confirm(`Index ${paths.length} files? This runs sequentially and may take a while.`)
    ) {
      return;
    }
    bulkIndexCancelRef.current = false;
    setBulkIndexState({ kind: "running", done: 0, total: paths.length });
    let succeeded = 0;
    let failed = 0;
    let done = 0;
    const failedPaths: string[] = [];
    for (const [index, path] of paths.entries()) {
      if (bulkIndexCancelRef.current) {
        setBulkIndexState({ kind: "cancelled", done, total: paths.length });
        return;
      }
      // Show the in-flight item (1-indexed) as "done" while its request is
      // outstanding, not just after it resolves — "Indexing 1/2..." means
      // "working on item 1 of 2", matching the mock's progress-label shape.
      setBulkIndexState({ kind: "running", done: index + 1, total: paths.length });
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
    setBulkIndexState({ kind: "done", succeeded, failed, failedPaths });
    dispatch({ type: "checked/clear" });
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
        state.listings.right.kind === "loaded"
          ? sortEntries(state.listings.right.value.entries)
          : [],
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
    const indexState = indexByPane[pane.id];
    const activeSftpProfile = state.sftp.connections.find(
      (c) => c.id === state.sftp.activeConnectionId,
    );
    const isLeftPane = pane.id === "left";

    return (
      <FilesPaneView
        key={pane.id}
        pane={pane}
        listing={listing}
        entries={entries}
        indexState={indexState}
        isLeftPane={isLeftPane}
        splitOpen={splitOpen}
        treeWidth={state.treeWidth}
        checked={state.checked}
        client={client}
        config={config}
        activeSftpConnectionId={state.sftp.activeConnectionId}
        activeSftpProfile={activeSftpProfile}
        sftpTreeRef={sftpTreeRef}
        onOpenEntry={(entry) => openEntry(pane.id, entry)}
        onOpenSftpFile={openSftpFile}
        onToggleChecked={(path) => dispatch({ type: "checked/toggle", path })}
        onSetCwd={(cwd) => dispatch({ type: "pane/setCwd", pane: pane.id, cwd })}
        onGoToBreadcrumb={(index) => goToBreadcrumb(pane.id, pane.cwd, index)}
        onActivatePane={() => activatePane(pane.id)}
        onToggleSplit={() => dispatch({ type: splitOpen ? "split/close" : "split/open" })}
        onToggleSftp={() =>
          state.sftp.activeConnectionId
            ? disconnectSftp()
            : dispatch({ type: "sftp/dialogOpen", draft: createEmptyConnectionDraft() })
        }
        onRefresh={() => loadDir(pane.id, pane.cwd)}
        onSetEditing={(editing) => dispatch({ type: "pane/setEditing", pane: pane.id, editing })}
        onCancelEdit={() => {
          dispatch({ type: "pane/setEditing", pane: pane.id, editing: false });
          if (pane.file.kind === "loaded") {
            dispatch({ type: "pane/setDraft", pane: pane.id, draft: pane.file.value.content });
          }
        }}
        onDraftChange={(value) => dispatch({ type: "pane/setDraft", pane: pane.id, draft: value })}
        onSave={() => void saveFile(pane.id)}
        onIndex={() => void indexSelected(pane.id)}
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
    );
  }

  return (
    <div className="output-body operation-view files-view">
      <FilesBulkIndexBar
        checkedCount={state.checked.size}
        bulkIndexState={bulkIndexState}
        canIndex={Boolean(client && config)}
        onClear={() => dispatch({ type: "checked/clear" })}
        onIndexAll={() => void bulkIndex()}
        onCancel={() => {
          bulkIndexCancelRef.current = true;
        }}
      />
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

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}
