import { arrField, isRecord, numField, strField, unwrapPayload } from "@/lib/payload";

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

function Metric({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <div className={accent ? "metric-cell metric-cell-accent" : "metric-cell"}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

export function StatsView({ payload }: { payload: unknown }) {
  const stats = unwrapPayload(payload);
  const counts = isRecord(stats.counts) ? stats.counts : {};
  const freshness = isRecord(stats.freshness) ? stats.freshness : {};
  const longest = isRecord(stats.longest_crawl) ? stats.longest_crawl : {};

  const countRows = Object.entries(counts)
    .filter(([, v]) => typeof v === "number" && Number.isFinite(v))
    .map(([k, v]) => [k, v as number] as const)
    .sort((a, b) => b[1] - a[1]);

  return (
    <div className="output-body stats-view aurora-scrollbar">
      <section className="stats-section">
        <h3 className="stats-heading">Collection · {strField(stats, "collection") ?? "axon"}</h3>
        <div className="metric-grid">
          <Metric label="Indexed vectors" value={fmtInt(numField(stats, "indexed_vectors_count"))} accent />
          <Metric label="Docs embedded" value={fmtInt(numField(stats, "docs_embedded_estimate"))} accent />
          <Metric label="Avg chunks/doc" value={fmtNum(numField(stats, "avg_chunks_per_doc"))} />
          <Metric label="Crawls total" value={fmtInt(numField(counts, "crawls"))} />
        </div>
      </section>

      <section className="stats-section">
        <h3 className="stats-heading">Freshness</h3>
        <div className="metric-grid">
          <Metric label="Crawls 24h" value={fmtInt(numField(freshness, "crawls_last_24h"))} />
          <Metric label="Crawls 7d" value={fmtInt(numField(freshness, "crawls_last_7d"))} />
          <Metric label="Last indexed" value={fmtAgo(numField(freshness, "last_indexed_secs_ago"))} />
          <Metric label="Pages/sec" value={fmtNum(numField(stats, "avg_pages_crawled_per_second"))} />
        </div>
      </section>

      <section className="stats-section">
        <h3 className="stats-heading">Timing</h3>
        <div className="metric-grid">
          <Metric label="Avg crawl" value={fmtSecs(numField(stats, "avg_crawl_duration_seconds"))} />
          <Metric label="Avg embed" value={fmtSecs(numField(stats, "avg_embedding_duration_seconds"))} />
          <Metric label="Avg overall" value={fmtSecs(numField(stats, "avg_overall_crawl_duration_seconds"))} />
          <Metric label="Longest crawl" value={fmtSecs(numField(longest, "seconds"))} />
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
          <div className="stats-spark">{arrField(stats, "growth_7d").map((n) => String(n)).join(" · ")}</div>
        </section>
      )}
    </div>
  );
}
