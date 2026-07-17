import { Upload } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";

export type BulkIndexState =
  | { kind: "idle" }
  | { kind: "running"; done: number; total: number }
  | { kind: "done"; succeeded: number; failed: number; failedPaths: string[] }
  | { kind: "cancelled"; done: number; total: number };

/**
 * The sticky bulk-select bar (checked count, Index all / Cancel / Clear
 * controls, progress and result status) plus the failed-paths list shown
 * after a partial-failure run. Rendered only when at least one file is
 * checked. Pure presentational — `bulkIndexState` and the checked count are
 * owned by the parent's reducer/component state.
 */
export function FilesBulkIndexBar({
  checkedCount,
  bulkIndexState,
  canIndex,
  onClear,
  onIndexAll,
  onCancel,
}: {
  checkedCount: number;
  bulkIndexState: BulkIndexState;
  canIndex: boolean;
  onClear: () => void;
  onIndexAll: () => void;
  onCancel: () => void;
}) {
  if (checkedCount === 0) return null;

  return (
    <>
      <div className="files-bulk-bar">
        <span>{checkedCount} selected</span>
        {bulkIndexState.kind !== "running" && (
          <Button variant="ghost" size="sm" type="button" onClick={onClear}>
            Clear
          </Button>
        )}
        <Button
          variant="aurora"
          size="sm"
          type="button"
          onClick={onIndexAll}
          disabled={bulkIndexState.kind === "running" || !canIndex}
        >
          <Upload size={13} />
          {bulkIndexState.kind === "running"
            ? `Indexing ${bulkIndexState.done}/${bulkIndexState.total}...`
            : "Index all"}
        </Button>
        {bulkIndexState.kind === "running" && (
          <Button variant="ghost" size="sm" type="button" onClick={onCancel}>
            Cancel
          </Button>
        )}
        {bulkIndexState.kind === "cancelled" && (
          <span className="files-bulk-status">
            Cancelled after {bulkIndexState.done}/{bulkIndexState.total}
          </span>
        )}
        {bulkIndexState.kind === "done" && (
          <span className="files-bulk-status">
            {bulkIndexState.succeeded} succeeded
            {bulkIndexState.failed > 0 && `, ${bulkIndexState.failed} failed`}
          </span>
        )}
      </div>
      {bulkIndexState.kind === "done" && bulkIndexState.failedPaths.length > 0 && (
        // Surfacing the actual failed paths (not just a bare count) so a
        // partial-failure run can be diagnosed/retried — see P2 #11.
        <div className="files-bulk-failures operation-muted">
          <span>Failed to index:</span>
          <ul>
            {bulkIndexState.failedPaths.map((path) => (
              <li key={path}>{path}</li>
            ))}
          </ul>
        </div>
      )}
    </>
  );
}
