import type { ReactNode } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";

import { arrField, isRecord, numField, shortId, strField, titleCase } from "@/lib/payload";
import { hostLabel } from "@/lib/url";

const LIST_LIMIT = 18;

type Tone = "info" | "success" | "warn" | "error" | "neutral" | "rose" | "violet";
type EmptyKind = "generic" | "results" | "urls" | "sources" | "jobs" | "watches" | "scrape" | "retrieve";

export function ResultRows({ rows, preferSnippet }: { rows: unknown[]; preferSnippet?: boolean }) {
  if (rows.length === 0) return <EmptyResult kind="results" />;
  return (
    <section className="operation-section">
      <h3 className="stats-heading">Results</h3>
      <div className="operation-list">
        {rows.slice(0, LIST_LIMIT).map((row, index) => {
          const record = isRecord(row) ? row : {};
          const title = strField(record, "title") ?? strField(record, "name") ?? strField(record, "url") ?? `Result ${index + 1}`;
          const url = strField(record, "url") ?? strField(record, "source_url");
          const snippet =
            strField(record, "snippet") ??
            strField(record, "content") ??
            strField(record, "text") ??
            strField(record, "reason");
          const score = numField(record, "score");
          const rank = numField(record, "rank") ?? index + 1;
          return (
            <article key={`${url ?? title}-${index}`} className="operation-row">
              <div className="operation-row-index">{rank}</div>
              <div className="operation-row-main">
                <div className="operation-row-title">
                  {url ? (
                    <a href={url} target="_blank" rel="noopener noreferrer">
                      {title}
                    </a>
                  ) : (
                    title
                  )}
                </div>
                {url ? <div className="operation-url">{url}</div> : null}
                {snippet ? <p className={preferSnippet ? "operation-snippet" : "operation-muted"}>{snippet}</p> : null}
              </div>
              {score !== undefined ? <span className="operation-score">{score.toFixed(3)}</span> : null}
            </article>
          );
        })}
      </div>
    </section>
  );
}

export function UrlListView({ title, payload, keys }: { title: string; payload: Record<string, unknown>; keys: string[] }) {
  const urls = arrayByKeys(payload, keys).filter((item): item is string => typeof item === "string");
  const count = numField(payload, "count") ?? numField(payload, "total") ?? urls.length;
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultSummary metrics={[["Total", count], ["View", title]]} />
      {urls.length === 0 ? (
        <EmptyResult kind={title.toLowerCase().includes("indexed") ? "sources" : "urls"} />
      ) : (
        <section className="operation-section">
          <div className="operation-url-grid">
            {urls.slice(0, LIST_LIMIT * 2).map((url) => (
              <a key={url} className="operation-url-card" href={url} target="_blank" rel="noopener noreferrer">
                <span>{hostLabel(url)}</span>
                <code>{url}</code>
              </a>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

export function JobRows({ rows, title = "Jobs" }: { rows: unknown[]; title?: string }) {
  return (
    <section className="operation-section">
      <h3 className="stats-heading">{title}</h3>
      <div className="operation-list">
        {rows.slice(0, LIST_LIMIT).map((row, index) => {
          const job = isRecord(row) ? row : {};
          const id = strField(job, "job_id") ?? strField(job, "id");
          const status = strField(job, "status") ?? strField(job, "state") ?? "unknown";
          const target = strField(job, "target") ?? strField(job, "url");
          return (
            <article key={`${id ?? index}`} className="operation-row">
              <StatusDot status={status} />
              <div className="operation-row-main">
                <div className="operation-row-title">{target ?? (id ? shortId(id) : undefined) ?? `Job ${index + 1}`}</div>
                {id ? <div className="operation-url">{id}</div> : null}
              </div>
              <span className={`operation-badge operation-badge-${toneForStatus(status)}`}>{status}</span>
            </article>
          );
        })}
      </div>
    </section>
  );
}

export function GenericResultView({ payload, embedded }: { payload: Record<string, unknown>; embedded?: boolean }) {
  const fields = Object.entries(payload).filter(([, value]) => typeof value !== "object" || value === null);
  if (fields.length === 0) return embedded ? <EmptyResult /> : null;
  const body = (
    <section className="operation-section">
      <div className="operation-detail-card">
        {fields.slice(0, 14).map(([key, value]) => (
          <DetailLine key={key} label={labelize(key)} value={formatDetailValue(key, value)} mono={isMonoDetail(key)} />
        ))}
      </div>
    </section>
  );
  return embedded ? body : <div className="output-body operation-view aurora-scrollbar">{body}</div>;
}

export function ResultSummary({ metrics }: { metrics: Array<[string, string | number]> }) {
  return (
    <section className="operation-summary-strip" aria-label="Result summary">
      {metrics.map(([label, value]) => (
        <span key={label}>
          <strong>{typeof value === "number" ? value.toLocaleString() : value}</strong>
          {label}
        </span>
      ))}
    </section>
  );
}

export function ResultHero({
  icon,
  title,
  metrics,
  tone,
}: {
  icon: ReactNode;
  title: string;
  metrics: Array<[string, string | number]>;
  tone: Tone;
}) {
  return (
    <section className={`operation-hero operation-hero-${tone}`}>
      <div className="operation-hero-icon">{icon}</div>
      <div className="operation-hero-main">
        <h3>{title}</h3>
        <div className="operation-metrics">
          {metrics.map(([label, value]) => (
            <span key={label}>
              <strong>{typeof value === "number" ? value.toLocaleString() : value}</strong>
              {label}
            </span>
          ))}
        </div>
      </div>
    </section>
  );
}

export function DetailLine({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="operation-detail-line">
      <span>{label}</span>
      <strong className={mono ? "operation-mono" : undefined}>{value}</strong>
    </div>
  );
}

export function ChipSection({ title, values }: { title: string; values: string[] }) {
  return (
    <section className="operation-section">
      <h3 className="stats-heading">{title}</h3>
      <div className="operation-chip-row">
        {values.slice(0, LIST_LIMIT).map((value) => (
          <span key={value} className="operation-chip">
            {value}
          </span>
        ))}
      </div>
    </section>
  );
}

export function Swatch({ color, label }: { color?: string; label: string }) {
  return (
    <div className="operation-swatch">
      <span style={color ? { background: color } : undefined} />
      <strong>{label}</strong>
      {color ? <code>{color}</code> : null}
    </div>
  );
}

export function EmptyResult({ kind = "generic" }: { kind?: EmptyKind }) {
  const copy = emptyCopy(kind);
  return (
    <div className="operation-empty">
      <strong>{copy.title}</strong>
      <span>{copy.body}</span>
    </div>
  );
}

export function StatusDot({ status }: { status: string }) {
  return <span className={`operation-dot operation-dot-${toneForStatus(status)}`} aria-hidden="true" />;
}

// Returns the first non-empty array found among the given payload keys, in order.
// Distinct from payload.ts's `firstArray(v)` (which scans values regardless of
// key) — these views need key-priority lookup (e.g. prefer `results` over
// `search_results`), so this stays a local shared helper rather than the canonical
// one. Renamed from `firstArray` to avoid collision with the canonical export.
export function arrayByKeys(payload: Record<string, unknown>, keys: string[]): unknown[] {
  for (const key of keys) {
    const value = arrField(payload, key);
    if (value.length > 0) return value;
  }
  return [];
}

export function isJobLifecycle(subcommand: string): boolean {
  return /^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/.test(subcommand);
}

function emptyCopy(kind: EmptyKind): { title: string; body: string } {
  switch (kind) {
    case "results":
      return { title: "No matches", body: "The operation completed, but Axon did not return any ranked results." };
    case "urls":
      return { title: "No URLs discovered", body: "Try a higher-level page, sitemap, or a domain with crawlable links." };
    case "sources":
      return { title: "No indexed sources", body: "This collection does not have source URLs yet. Scrape, crawl, or ingest something first." };
    case "jobs":
      return { title: "No jobs in this lane", body: "There are no queued, running, or recent jobs for this operation family." };
    case "watches":
      return { title: "No watches configured", body: "Create a watch from a URL to schedule recurring crawls or diffs." };
    case "scrape":
      return { title: "No page body returned", body: "The page was reachable, but the scrape result did not include markdown content." };
    case "retrieve":
      return { title: "No chunks returned", body: "Axon did not find stored chunks for that source URL in the active collection." };
    default:
      return { title: "No structured fields", body: "The response was successful, but it did not include displayable fields." };
  }
}

export function sanitizeReaderMarkdown(value: string | undefined): string | undefined {
  if (!value) return value;

  let inFence = false;
  let fenceStart = -1;
  const lines = value.split(/\r?\n/);
  const kept: string[] = [];

  for (const rawLine of lines) {
    const cleanedLine = inFence ? rawLine : cleanupScrapeArtifactLine(rawLine);
    if (cleanedLine === undefined) continue;
    const line = cleanedLine;
    if (/^\s*(```|~~~)/.test(line)) {
      if (!inFence) {
        fenceStart = kept.length;
        kept.push(line);
        inFence = true;
        continue;
      }

      kept.push(line);
      const body = kept.slice(fenceStart + 1, -1);
      if (body.length === 0 || body.every(isEmptyBulletLine)) {
        kept.splice(fenceStart);
      }
      inFence = !inFence;
      fenceStart = -1;
      continue;
    }

    if (inFence || !isEmptyBulletLine(line)) {
      kept.push(line);
    }
  }

  return kept.join("\n").replace(/\n{3,}/g, "\n\n").trim();
}

function cleanupScrapeArtifactLine(line: string): string | undefined {
  const cleaned = line
    .replace(/Skip to main content/gi, "")
    .replace(/Debugging\.{0,3}/gi, "")
    .replace(/\s*\(opens in new tab\)/gi, "")
    .replace(/\)(?=[A-Z][A-Za-z])/g, ") ")
    .replace(/([a-z0-9])(?=(?:Developer docs|Onboarding|Triage issues|Refactor code)\b)/g, "$1 ")
    .replace(/[ \t]{2,}/g, " ")
    .trimEnd();

  const normalized = cleaned.trim();
  if (/^(?:debugging|skip to main content)$/i.test(normalized)) return undefined;
  return cleaned;
}

function isEmptyBulletLine(line: string): boolean {
  return /^\s*$/.test(line) || /^\s*(?:[-+*]\s*|[•‣◦]\s*)$/.test(line);
}

// Resolve a screenshot path/URL to something the WebView can render. Remote
// http(s) and inline data:image sources pass through. Local filesystem paths are
// converted via Tauri's `convertFileSrc` (the `asset:` protocol the CSP allows),
// not a raw `file://` URL — `file:` is excluded from `img-src`, so the old branch
// only produced broken images and let payload-controlled paths build arbitrary
// `file://` references (S-L2).
export function imagePreviewSrc(path: string | undefined): string | undefined {
  if (!path) return undefined;
  if (/^https?:\/\//i.test(path) || path.startsWith("data:image/")) return path;
  if (!/\.(png|jpe?g|webp|gif|avif)$/i.test(path)) return undefined;
  if (!path.startsWith("/")) return path;
  return convertFileSrc(path);
}

function labelize(value: string): string {
  const aliases: Record<string, string> = {
    id: "ID",
    job_id: "Job ID",
    watch_id: "Watch ID",
    url: "URL",
    url_a: "Before",
    url_b: "After",
    status_url: "Status endpoint",
    source_url: "Source URL",
    started_at: "Started",
    finished_at: "Finished",
    created_at: "Created",
    updated_at: "Updated",
    next_run_at: "Next run",
    every_seconds: "Interval",
    size_bytes: "Size",
    word_count_delta: "Word delta",
    points_deleted: "Points deleted",
    points_scanned: "Points scanned",
    execution_mode: "Execution mode",
  };
  const normalized = value.toLowerCase();
  if (aliases[normalized]) return aliases[normalized];
  return normalized
    .split("_")
    .map((part) => (part.length <= 2 ? part.toUpperCase() : titleCase(part)))
    .join(" ");
}

export function formatDetailValue(key: string, value: unknown): string {
  if (typeof value === "boolean") return value ? "yes" : "no";
  if (typeof value === "number") {
    if (key.endsWith("_seconds")) return formatDuration(value);
    if (key.endsWith("_bytes")) return formatBytes(value);
    return value.toLocaleString();
  }
  if (value === null || value === undefined) return "-";
  return String(value);
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m`;
  if (seconds < 86400) return `${Math.round(seconds / 3600)}h`;
  return `${Math.round(seconds / 86400)}d`;
}

function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let index = 0;
  while (value >= 1024 && index < units.length - 1) {
    value /= 1024;
    index += 1;
  }
  const rounded = index === 0 ? value.toFixed(0) : value.toFixed(1);
  return `${rounded} ${units[index]}`;
}

function isMonoDetail(key: string): boolean {
  const lower = key.toLowerCase();
  return lower.endsWith("id") || lower.includes("url") || lower.includes("path") || lower.includes("endpoint");
}

export function isBadStatus(status: string | undefined): boolean {
  return status === "error" || status === "failed" || status === "degraded" || status === "warn";
}

export function toneForStatus(status: string | undefined): Tone {
  switch ((status ?? "").toLowerCase()) {
    case "complete":
    case "completed":
    case "success":
    case "healthy":
    case "ok":
      return "success";
    case "queued":
    case "pending":
    case "running":
    case "accepted":
    case "warn":
    case "warning":
      return "warn";
    case "error":
    case "failed":
    case "degraded":
      return "error";
    default:
      return "neutral";
  }
}
