// SFTP connect/disconnect/open-file lifecycle for FilesView.tsx, extracted out
// of the component to keep it under the monolith line cap. This is
// orchestration logic (invoke() calls + dispatching filesViewReducer actions),
// not a reusable pure helper, so it lives here as a custom hook rather than in
// filesModel.ts/sftpModel.ts's plain-function style.

import type { RefObject } from "react";

import type { FilesViewAction } from "./filesViewState";
import { invoke } from "./invoke";
import type {
  SftpConnectionDraft,
  SftpConnectionProfile,
  SftpEntry,
  SftpKnownHostEntry,
} from "./sftpModel";

export interface SftpTreeSectionHandleLike {
  loadDir: (connectionId: string, path: string) => void;
  reset: () => void;
}

export interface SftpLifecycleState {
  activeConnectionId: string | null;
  connections: SftpConnectionProfile[];
}

/**
 * Builds the connect/disconnect/open-file callbacks used by FilesView. Takes
 * the pieces it needs (current sftp slice, dispatch, the load-gen ref, active
 * pane, and the SftpTreeSection imperative handle ref) as plain arguments
 * rather than a hook with internal state — FilesView still owns all the
 * underlying state; this just groups the three related async callbacks so
 * they don't bloat the component body.
 */
export function createSftpLifecycle({
  sftp,
  dispatch,
  isTauriRuntime,
  config,
  sftpTreeRef,
  loadGenRef,
  activePane,
}: {
  sftp: SftpLifecycleState;
  dispatch: (action: FilesViewAction) => void;
  isTauriRuntime: boolean;
  config: { sftpConnections?: SftpConnectionProfile[] } | null;
  sftpTreeRef: RefObject<SftpTreeSectionHandleLike | null>;
  loadGenRef: RefObject<Record<"left" | "right", number>>;
  activePane: "left" | "right";
}) {
  async function persistSftpConnections(connections: SftpConnectionProfile[]) {
    if (!isTauriRuntime || !config) return;
    try {
      await invoke("save_palette_settings", {
        settings: { ...config, sftpConnections: connections },
      });
    } catch {
      // Best-effort: a failed persist just means the profile won't survive a
      // restart. The live connection itself is unaffected.
    }
  }

  async function connectSftp(draft: SftpConnectionDraft, trustNewHost = false) {
    // Disconnect any previously active connection FIRST, and wait for it —
    // v1 supports one active SFTP connection at a time (see Task 5d's Open
    // Question resolution), and the Rust side now hard-rejects a new
    // `sftp_connect` while a session is still open (see
    // sftp_bridge/commands.rs). Disconnecting after would race that cap.
    if (sftp.activeConnectionId) {
      await invoke("sftp_disconnect", { connectionId: sftp.activeConnectionId }).catch(() => {});
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
    sftpTreeRef.current?.reset();
    sftpTreeRef.current?.loadDir(result.connectionId, "");
    // Persist the newly-connected profile so it survives an app restart (see
    // fix for P1 #3) — save_palette_prefs merges/writes sftp_connections
    // alongside the rest of settings.json.
    void persistSftpConnections([
      ...sftp.connections.filter((c) => c.id !== profile.id),
      profile,
    ]);
  }

  function disconnectSftp() {
    if (sftp.activeConnectionId) {
      void invoke("sftp_disconnect", { connectionId: sftp.activeConnectionId }).catch(() => {});
    }
    dispatch({ type: "sftp/disconnect" });
    sftpTreeRef.current?.reset();
  }

  function openSftpFile(connectionId: string, entry: SftpEntry) {
    // Capture the target pane and bump/capture `gen` BEFORE the async read
    // starts — mirrors `loadFile`'s exact pattern. Reading `loadGenRef`
    // inside the `.then()` (the previous bug) reads whatever generation is
    // current when the read *resolves*, not when it *started* — so a stale
    // remote read that resolves after a newer local file was opened would
    // read the newer gen and pass the staleness check, silently overwriting
    // the newer selection instead of being dropped as superseded.
    const pane = activePane;
    const gen = loadGenRef.current[pane] + 1;
    loadGenRef.current[pane] = gen;
    dispatch({ type: "pane/fileLoading", pane, loadGen: gen });
    invoke<{ path: string; content: string }>("sftp_read_file", {
      connectionId,
      path: entry.path,
    })
      .then((file) => {
        dispatch({
          type: "pane/select",
          pane,
          entry: {
            name: entry.name,
            path: entry.path,
            isDir: false,
            size: entry.size,
            origin: "sftp",
          },
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
          message: err instanceof Error ? err.message : String(err),
        }),
      );
  }

  return { connectSftp, disconnectSftp, openSftpFile };
}
