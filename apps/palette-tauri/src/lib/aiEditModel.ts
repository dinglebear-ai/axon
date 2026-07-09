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
      result.push({ kind: "same", text: beforeLine as string });
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
