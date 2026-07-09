import { Upload } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";

export type BulkIngestState =
  | { kind: "idle" }
  | { kind: "running"; done: number; total: number }
  | { kind: "done"; succeeded: number; failed: number; failedPaths: string[] }
  | { kind: "cancelled"; done: number; total: number };

/**
 * The sticky bulk-select bar (checked count, Ingest all / Cancel / Clear
 * controls, progress and result status) plus the failed-paths list shown
 * after a partial-failure run. Rendered only when at least one file is
 * checked. Pure presentational — `bulkIngestState` and the checked count are
 * owned by the parent's reducer/component state.
 */
export function FilesBulkBar({
  checkedCount,
  bulkIngestState,
  canIngest,
  onClear,
  onIngestAll,
  onCancel,
}: {
  checkedCount: number;
  bulkIngestState: BulkIngestState;
  canIngest: boolean;
  onClear: () => void;
  onIngestAll: () => void;
  onCancel: () => void;
}) {
  if (checkedCount === 0) return null;

  return (
    <>
      <div className="files-bulk-bar">
        <span>{checkedCount} selected</span>
        {bulkIngestState.kind !== "running" && (
          <Button variant="ghost" size="sm" type="button" onClick={onClear}>
            Clear
          </Button>
        )}
        <Button
          variant="aurora"
          size="sm"
          type="button"
          onClick={onIngestAll}
          disabled={bulkIngestState.kind === "running" || !canIngest}
        >
          <Upload size={13} />
          {bulkIngestState.kind === "running"
            ? `Ingesting ${bulkIngestState.done}/${bulkIngestState.total}...`
            : "Ingest all"}
        </Button>
        {bulkIngestState.kind === "running" && (
          <Button variant="ghost" size="sm" type="button" onClick={onCancel}>
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
    </>
  );
}
