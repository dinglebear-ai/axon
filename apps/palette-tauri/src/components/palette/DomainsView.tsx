import { memo, useMemo } from "react";
import { ChevronRight } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import { arrField, isRecord, numField, strField, unwrapPayload } from "@/lib/payload";

interface DomainRow {
  domain: string;
  vectors: number;
}

interface DomainsViewProps {
  payload: unknown;
  /** Drill into the sources for a domain (pre-filters the sources view). */
  onDrillDomain?: (domain: string) => void;
}

function parseRows(payload: unknown): DomainRow[] {
  const data = unwrapPayload(payload);
  return arrField(data, "domains").flatMap((entry) => {
    if (!isRecord(entry)) return [];
    const domain = strField(entry, "domain");
    const vectors = numField(entry, "vectors") ?? 0;
    return domain ? [{ domain, vectors }] : [];
  });
}

export const DomainsView = memo(function DomainsView({ payload, onDrillDomain }: DomainsViewProps) {
  const rows = useMemo(() => {
    return [...parseRows(payload)].sort((a, b) => b.vectors - a.vectors);
  }, [payload]);
  const max = useMemo(() => Math.max(1, ...rows.map((r) => r.vectors)), [rows]);
  const total = useMemo(() => rows.reduce((s, r) => s + r.vectors, 0), [rows]);

  return (
    <div className="output-body domains-view aurora-scrollbar">
      <div className="sources-summary">
        <span>
          <strong>{rows.length.toLocaleString()}</strong> domains
        </span>
        <span>
          <strong>{total.toLocaleString()}</strong> vectors
        </span>
      </div>

      {rows.length === 0 ? (
        <div className="status-empty">No indexed domains.</div>
      ) : (
        <div className="domains-list">
          {rows.map((row) => {
            const pct = Math.max(2, Math.round((row.vectors / max) * 100));
            const inner = (
              <>
                <span className="domains-bar" aria-hidden="true">
                  <span style={{ width: `${pct}%` }} />
                </span>
                <span className="domains-name">{row.domain}</span>
                <span className="domains-count">{row.vectors.toLocaleString()}</span>
                {onDrillDomain ? <ChevronRight size={14} className="domains-chevron" /> : null}
              </>
            );
            return onDrillDomain ? (
              <Button
                key={row.domain}
                variant="plain"
                size="unstyled"
                type="button"
                className="domains-row domains-row-clickable"
                onClick={() => onDrillDomain(row.domain)}
                title={`Show sources for ${row.domain}`}
              >
                {inner}
              </Button>
            ) : (
              <div key={row.domain} className="domains-row">
                {inner}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
});
