// The AI-edit "Edit with the model" propose/approve orchestration for
// FilesView.tsx, extracted out of the component to keep it under the
// monolith line cap. This is orchestration logic (executeAction calls +
// dispatching filesViewReducer actions), not a pure helper, so it lives here
// rather than in aiEditModel.ts's pure-function style (computeLineDiff stays
// there).

import { ACTIONS, type RemotePaletteAction } from "./actions";
import { computeLineDiff } from "./aiEditModel";
import { type Client, executeAction, type PaletteConfig } from "./axonClient";
import type { FileContents, FilesPane, PaneId } from "./filesModel";
import type { FilesViewAction } from "./filesViewState";
import { invoke } from "./invoke";
import { strField, unwrapPayload } from "./payload";

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

/**
 * Builds the submit/approve callbacks for the AI-edit propose/approve flow.
 * Takes the reducer's `panes` + `dispatch` (rather than owning state itself)
 * since FilesView's reducer already models per-pane proposal state.
 */
export function createAiEditFlow({
  panes,
  dispatch,
  client,
  config,
}: {
  panes: FilesViewPanes;
  dispatch: (action: FilesViewAction) => void;
  client: Client | null;
  config: PaletteConfig | null;
}) {
  async function submitSparkleQuery(id: PaneId) {
    const pane = panes.find((p) => p.id === id);
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
    const pane = panes.find((p) => p.id === id);
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
          message:
            "The file changed on disk since this edit was proposed. Re-open it and try again.",
        });
        return;
      }
      const saved = await invoke<FileContents>("files_write_file", {
        path: pane.selected.path,
        content: pane.proposal.proposedContent,
      });
      dispatch({ type: "pane/proposalApproved", pane: id, file: saved });
    } catch (err) {
      dispatch({
        type: "pane/proposalApproveError",
        pane: id,
        message: err instanceof Error ? err.message : String(err),
      });
    }
  }

  return { submitSparkleQuery, approveProposal };
}

type FilesViewPanes = readonly FilesPane[];
