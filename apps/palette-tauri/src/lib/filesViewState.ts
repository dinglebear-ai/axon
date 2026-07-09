// useReducer-based state model for FilesView.tsx. Centralizing all Files-view
// state transitions here (rather than a flat useState block in the
// component) keeps FilesView.tsx a thin renderer over `state`/`dispatch` as
// later tasks add split-pane, bulk-select, AI-edit, and SFTP state — each
// adds new `FilesViewAction` members and reducer cases here instead of new
// `useState` calls and prop-drilled setters in the component.

import type { AiEditProposal } from "./aiEditModel";
import {
  type CheckedPaths,
  checkAllIn,
  clearChecked,
  createPane,
  type DirListing,
  type FileContents,
  type FileEntry,
  type FilesPane,
  isChecked,
  type LoadState,
  type PaneId,
  toggleChecked,
} from "./filesModel";
import type { SftpConnectionDraft, SftpConnectionProfile, SftpKnownHostEntry } from "./sftpModel";

export const MIN_TREE_WIDTH = 180;
export const MAX_TREE_WIDTH = 460;
const DEFAULT_TREE_WIDTH = 248;

/** SFTP connection-management UI state: persisted profiles, which one (if
 * any) is actively connected, the add/edit dialog's open/draft state, and a
 * pending TOFU trust decision awaiting user confirmation. Kept as its own
 * slice (not spread across pane state) since it's global to the view, not
 * per-pane — a single active SFTP connection is shared across both panes
 * when split. */
export interface SftpUiState {
  connections: SftpConnectionProfile[];
  activeConnectionId: string | null;
  dialogOpen: boolean;
  editingProfile: SftpConnectionDraft | null;
  pendingTrust: SftpKnownHostEntry | null;
}

export interface FilesViewState {
  panes: [FilesPane] | [FilesPane, FilesPane];
  activePane: PaneId;
  listings: Record<PaneId, LoadState<DirListing>>;
  treeWidth: number;
  checked: CheckedPaths;
  sftp: SftpUiState;
}

export function createInitialState(): FilesViewState {
  return {
    panes: [createPane("left")],
    activePane: "left",
    listings: { left: { kind: "idle" }, right: { kind: "idle" } },
    treeWidth: DEFAULT_TREE_WIDTH,
    checked: clearChecked(),
    sftp: {
      connections: [],
      activeConnectionId: null,
      dialogOpen: false,
      editingProfile: null,
      pendingTrust: null,
    },
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
  | { type: "checked/clear" }
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
  | { type: "sftp/dialogOpen"; draft: SftpConnectionDraft }
  | { type: "sftp/dialogClose" }
  | { type: "sftp/connectionsLoaded"; connections: SftpConnectionProfile[] }
  | { type: "sftp/connectStart" }
  | { type: "sftp/connected"; connectionId: string; profile: SftpConnectionProfile }
  | { type: "sftp/pendingTrust"; entry: SftpKnownHostEntry }
  | { type: "sftp/trustConfirmed" }
  | { type: "sftp/disconnect" };

function updatePane(
  panes: FilesViewState["panes"],
  id: PaneId,
  patch: Partial<FilesPane>,
): FilesViewState["panes"] {
  // Reconstruct as a literal tuple (not Array.prototype.map + a cast back to
  // the tuple type) so `tsc` actually verifies the 1-or-2-length invariant —
  // a `.map(...) as FilesViewState["panes"]` cast would silently accept a
  // future regression that produced e.g. a 3-length array.
  const first = panes[0].id === id ? { ...panes[0], ...patch } : panes[0];
  if (panes.length === 1) return [first];
  const second = panes[1].id === id ? { ...panes[1], ...patch } : panes[1];
  return [first, second];
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
      return {
        ...state,
        treeWidth: Math.max(MIN_TREE_WIDTH, Math.min(MAX_TREE_WIDTH, action.width)),
      };
    case "checked/toggle":
      return { ...state, checked: toggleChecked(state.checked, action.path) };
    case "checked/checkAll":
      return { ...state, checked: checkAllIn(state.checked, action.paths) };
    case "checked/clear":
      return { ...state, checked: clearChecked() };
    case "pane/sparkleOpen":
      return { ...state, panes: updatePane(state.panes, action.pane, { sparkleOpen: true }) };
    case "pane/sparkleClose":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, { sparkleOpen: false, sparkleQuery: "" }),
      };
    case "pane/sparkleQueryChange":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, { sparkleQuery: action.query }),
      };
    case "pane/proposalPending":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          proposalState: "pending",
          proposalErrorMessage: null,
        }),
      };
    case "pane/proposalReady":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          proposal: action.proposal,
          proposalState: "ready",
          proposalErrorMessage: null,
          sparkleOpen: false,
          sparkleQuery: "",
        }),
      };
    case "pane/proposalError":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          proposalState: "error",
          proposalErrorMessage: action.message,
        }),
      };
    case "pane/proposalDeny":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          proposal: null,
          proposalState: "idle",
          proposalErrorMessage: null,
        }),
      };
    case "pane/proposalApproveStart":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          proposalState: "approving",
          proposalErrorMessage: null,
        }),
      };
    case "pane/proposalApproved":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          file: { kind: "loaded", value: action.file },
          draft: action.file.content,
          proposal: null,
          proposalState: "idle",
          proposalErrorMessage: null,
        }),
      };
    case "pane/proposalApproveError":
      return {
        ...state,
        panes: updatePane(state.panes, action.pane, {
          proposalState: "error",
          proposalErrorMessage: action.message,
        }),
      };
    case "sftp/dialogOpen":
      return {
        ...state,
        sftp: { ...state.sftp, dialogOpen: true, editingProfile: action.draft },
      };
    case "sftp/dialogClose":
      return { ...state, sftp: { ...state.sftp, dialogOpen: false, editingProfile: null } };
    case "sftp/connectionsLoaded":
      return { ...state, sftp: { ...state.sftp, connections: action.connections } };
    case "sftp/connectStart":
      return { ...state, sftp: { ...state.sftp, pendingTrust: null } };
    case "sftp/connected": {
      // Auto-persist-on-first-successful-connect (see Task 5d's Open
      // Question resolution): the profile is added to the connections list
      // the moment a connection actually succeeds, not behind a separate
      // "save profile?" step.
      const existingIndex = state.sftp.connections.findIndex((c) => c.id === action.profile.id);
      const connections =
        existingIndex >= 0
          ? state.sftp.connections.map((c, i) => (i === existingIndex ? action.profile : c))
          : [...state.sftp.connections, action.profile];
      return {
        ...state,
        sftp: {
          ...state.sftp,
          connections,
          activeConnectionId: action.connectionId,
          dialogOpen: false,
          editingProfile: null,
          pendingTrust: null,
        },
      };
    }
    case "sftp/pendingTrust":
      return { ...state, sftp: { ...state.sftp, pendingTrust: action.entry } };
    case "sftp/trustConfirmed":
      return { ...state, sftp: { ...state.sftp, pendingTrust: null } };
    case "sftp/disconnect":
      return { ...state, sftp: { ...state.sftp, activeConnectionId: null } };
    default:
      return state;
  }
}

export { isChecked };
