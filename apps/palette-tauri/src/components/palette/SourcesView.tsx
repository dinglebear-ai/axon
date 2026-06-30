import { memo } from "react";
import { ArrowDownUp, Download, Search, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import type { SourceRow, SourceSortMode, SourcesModel } from "@/lib/sourcesModel";

/** Run a palette action against a single source URL — `retrieve` (read the
 * stored chunks) or `purge` (delete from the index). Purge is guarded by the
 * destructive-action confirmation, then routes through `POST /v1/purge`. */
export type SourceRowAction = (subcommand: string, argument: string) => void;

interface SourcesViewProps {
  model: SourcesModel;
  onRunAction?: SourceRowAction;
  filter: string;
  sort: SourceSortMode;
  grouped: boolean;
  onFilterChange: (filter: string) => void;
  onSortChange: (sort: SourceSortMode) => void;
  onGroupedChange: (grouped: boolean) => void;
}

export const SourcesView = memo(function SourcesView({
  model,
  onRunAction,
  filter,
  sort,
  grouped,
  onFilterChange,
  onSortChange,
  onGroupedChange,
}: SourcesViewProps) {
  const { rows, filtered, groups, totalChunks } = model;
  return (
    <div className="output-body sources-view aurora-scrollbar">
      <div className="sources-toolbar">
        <Input
          className="sources-search"
          size="sm"
          startAdornment={<Search size={13} aria-hidden="true" />}
          value={filter}
          placeholder="Filter URLs…"
          onChange={(event) => onFilterChange(event.target.value)}
          aria-label="Filter sources by URL"
        />
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          className="sources-toggle"
          onClick={() => onSortChange(sort === "chunks" ? "url" : "chunks")}
          title="Toggle sort"
        >
          <ArrowDownUp size={13} />
          {sort === "chunks" ? "chunks" : "url"}
        </Button>
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          className={grouped ? "sources-toggle sources-toggle-on" : "sources-toggle"}
          onClick={() => onGroupedChange(!grouped)}
          title="Group by domain"
        >
          group
        </Button>
      </div>

      <div className="sources-summary">
        <span>
          <strong>{filtered.length.toLocaleString()}</strong>
          {filtered.length === rows.length ? " URLs" : ` / ${rows.length.toLocaleString()} URLs`}
        </span>
        <span>
          <strong>{totalChunks.toLocaleString()}</strong> chunks
        </span>
      </div>

      {filtered.length === 0 ? (
        <div className="status-empty">No sources match.</div>
      ) : groups ? (
        groups.map((group) => (
          <section key={group.domain} className="sources-group">
            <h3 className="stats-heading">
              {group.domain} · {group.list.length} · {group.chunks.toLocaleString()} chunks
            </h3>
            <div className="sources-list">
              {group.list.map((row) => (
                <SourceRowView key={row.url} row={row} onRunAction={onRunAction} />
              ))}
            </div>
          </section>
        ))
      ) : (
        <div className="sources-list">
          {filtered.map((row) => (
            <SourceRowView key={row.url} row={row} onRunAction={onRunAction} />
          ))}
        </div>
      )}
    </div>
  );
});

function SourceRowView({ row, onRunAction }: { row: SourceRow; onRunAction?: SourceRowAction }) {
  return (
    <div className="sources-row">
      <span className="sources-url" title={row.url}>
        {row.url}
      </span>
      <span className="sources-chunks">{row.chunks.toLocaleString()}</span>
      {onRunAction ? (
        <span className="sources-row-actions">
          <Button
            variant="plain"
            size="unstyled"
            type="button"
            onClick={() => onRunAction("retrieve", row.url)}
            title="Retrieve stored chunks"
            aria-label={`Retrieve ${row.url}`}
          >
            <Download size={13} />
          </Button>
          <Button
            variant="plain"
            size="unstyled"
            type="button"
            className="sources-action-danger"
            onClick={() => onRunAction("purge", row.url)}
            title="Purge from index"
            aria-label={`Purge ${row.url}`}
          >
            <Trash2 size={13} />
          </Button>
        </span>
      ) : null}
    </div>
  );
}
