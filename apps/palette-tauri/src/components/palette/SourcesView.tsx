import { memo, useMemo, useState } from "react";
import { ArrowDownUp, Download, Search, Trash2 } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import { arrField, unwrapPayload } from "@/lib/payload";

export interface SourceRowAction {
  /** Run a palette action against a single source URL — `retrieve` (read the
   * stored chunks) or `purge` (delete from the index). Purge is guarded by the
   * destructive-action confirmation, then routes through `POST /v1/purge`. */
  (subcommand: string, argument: string): void;
}

interface SourcesViewProps {
  payload: unknown;
  onRunAction?: SourceRowAction;
  /** Pre-seeded filter, e.g. when drilling in from a domain. */
  initialFilter?: string;
}

interface SourceRow {
  url: string;
  chunks: number;
}

type SortMode = "chunks" | "url";

function parseRows(payload: unknown): SourceRow[] {
  const data = unwrapPayload(payload);
  return arrField(data, "urls").flatMap((entry) => {
    if (!Array.isArray(entry)) return [];
    const url = typeof entry[0] === "string" ? entry[0] : "";
    const chunks = typeof entry[1] === "number" ? entry[1] : 0;
    return url ? [{ url, chunks }] : [];
  });
}

function domainOf(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url.replace(/^https?:\/\//, "").split("/")[0] || url;
  }
}

export const SourcesView = memo(function SourcesView({ payload, onRunAction, initialFilter }: SourcesViewProps) {
  const [filter, setFilter] = useState(initialFilter ?? "");
  const [sort, setSort] = useState<SortMode>("chunks");
  const [grouped, setGrouped] = useState(false);

  const rows = useMemo(() => parseRows(payload), [payload]);
  const totalChunks = useMemo(() => rows.reduce((sum, r) => sum + r.chunks, 0), [rows]);

  const filtered = useMemo(() => {
    const needle = filter.trim().toLowerCase();
    const matched = needle ? rows.filter((r) => r.url.toLowerCase().includes(needle)) : rows;
    return [...matched].sort((a, b) => (sort === "chunks" ? b.chunks - a.chunks : a.url.localeCompare(b.url)));
  }, [rows, filter, sort]);

  const groups = useMemo(() => {
    if (!grouped) return null;
    const byDomain = new Map<string, SourceRow[]>();
    for (const row of filtered) {
      const key = domainOf(row.url);
      const list = byDomain.get(key);
      if (list) list.push(row);
      else byDomain.set(key, [row]);
    }
    return [...byDomain.entries()]
      .map(([domain, list]) => ({ domain, list, chunks: list.reduce((s, r) => s + r.chunks, 0) }))
      .sort((a, b) => b.chunks - a.chunks);
  }, [grouped, filtered]);

  return (
    <div className="output-body sources-view aurora-scrollbar">
      <div className="sources-toolbar">
        <Input
          className="sources-search"
          size="sm"
          startAdornment={<Search size={13} aria-hidden="true" />}
          value={filter}
          placeholder="Filter URLs…"
          onChange={(e) => setFilter(e.target.value)}
          aria-label="Filter sources by URL"
        />
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          className="sources-toggle"
          onClick={() => setSort((s) => (s === "chunks" ? "url" : "chunks"))}
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
          onClick={() => setGrouped((g) => !g)}
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
