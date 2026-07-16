import { memo, useEffect, useState } from "react";

import { Sparkline } from "@/components/palette/Sparkline";
import { arrField, isRecord, numField, strField, unwrapPayload } from "@/lib/payload";

function fmtDelta(value: number): string {
  const rounded = Math.round(value);
  return `${rounded >= 0 ? "+" : ""}${rounded.toLocaleString()}`;
}

function fmtInt(value: number | undefined): string {
  return value === undefined ? "—" : Math.round(value).toLocaleString();
}

function fmtNum(value: number | undefined, digits = 2): string {
  return value === undefined ? "—" : value.toFixed(digits);
}

function fmtSecs(value: number | undefined): string {
  if (value === undefined) return "—";
  if (value < 60) return `${value.toFixed(1)}s`;
  const mins = Math.floor(value / 60);
  const secs = Math.round(value % 60);
  return `${mins}m ${secs}s`;
}

function fmtAgo(secs: number | undefined): string {
  if (secs === undefined) return "—";
  if (secs < 60) return `${Math.round(secs)}s ago`;
  if (secs < 3600) return `${Math.round(secs / 60)}m ago`;
  return `${Math.round(secs / 3600)}h ago`;
}

function Metric({
  label,
  value,
  accent,
  delta,
}: {
  label: string;
  value: string;
  accent?: boolean;
  delta?: string;
}) {
  return (
    <div className={accent ? "metric-cell metric-cell-accent" : "metric-cell"}>
      <span>{label}</span>
      <strong>
        {value}
        {delta ? <span className="metric-delta">{delta}</span> : null}
      </strong>
    </div>
  );
}

export const StatsView = memo(function StatsView({ payload }: { payload: unknown }) {
  const stats = unwrapPayload(payload);
  const counts = isRecord(stats.counts) ? stats.counts : {};
  const freshness = isRecord(stats.freshness) ? stats.freshness : {};

  // Since-opened delta: capture the first indexed-vector count seen by this
  // mounted view, so live refreshes show how much landed while you watched.
  // Resets on remount (navigating away and back opens a fresh baseline).
  const indexed = numField(stats, "indexed_vectors_count");
  const [baseline, setBaseline] = useState<number | null>(indexed ?? null);
  useEffect(() => {
    if (baseline === null && indexed !== undefined) setBaseline(indexed);
  }, [baseline, indexed]);
  const delta = indexed !== undefined && baseline !== null ? indexed - baseline : 0;

  const countRows = Object.entries(counts)
    .filter(([, v]) => typeof v === "number" && Number.isFinite(v))
    .map(([k, v]) => [k, v as number] as const)
    .sort((a, b) => b[1] - a[1]);

  return (
    <div className="output-body stats-view aurora-scrollbar">
      <section className="stats-section">
        <h3 className="stats-heading">Collection · {strField(stats, "collection") ?? "axon"}</h3>
        <div className="metric-grid">
          <Metric
            label="Indexed vectors"
            value={fmtInt(indexed)}
            accent
            delta={delta > 0 ? fmtDelta(delta) : undefined}
          />
          <Metric
            label="Docs embedded"
            value={fmtInt(numField(stats, "docs_embedded_estimate"))}
            accent
          />
          <Metric label="Avg chunks/doc" value={fmtNum(numField(stats, "avg_chunks_per_doc"))} />
        </div>
      </section>

      <section className="stats-section">
        <h3 className="stats-heading">Freshness</h3>
        <div className="metric-grid">
          <Metric
            label="Last indexed"
            value={fmtAgo(numField(freshness, "last_indexed_secs_ago"))}
          />
        </div>
      </section>

      <section className="stats-section">
        <h3 className="stats-heading">Timing</h3>
        <div className="metric-grid">
          <Metric
            label="Avg embed"
            value={fmtSecs(numField(stats, "avg_embedding_duration_seconds"))}
          />
        </div>
      </section>

      {countRows.length > 0 && (
        <section className="stats-section">
          <h3 className="stats-heading">Operation counts</h3>
          <div className="metric-grid metric-grid-compact">
            {countRows.map(([key, value]) => (
              <Metric key={key} label={key} value={fmtInt(value)} />
            ))}
          </div>
        </section>
      )}

      {arrField(stats, "growth_7d").length > 0 && (
        <section className="stats-section">
          <h3 className="stats-heading">7-day growth</h3>
          <div className="stats-spark">
            <Sparkline
              values={arrField(stats, "growth_7d").map((n) =>
                typeof n === "number" ? n : Number(n) || 0,
              )}
              ariaLabel="Indexed-document growth over the last 7 days"
            />
            <span className="stats-spark-caption">
              {arrField(stats, "growth_7d")
                .reduce<number>((sum, n) => sum + (typeof n === "number" ? n : Number(n) || 0), 0)
                .toLocaleString()}{" "}
              docs / 7d
            </span>
          </div>
        </section>
      )}
    </div>
  );
});
