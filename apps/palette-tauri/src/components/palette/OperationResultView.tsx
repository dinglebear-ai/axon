import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Clock3,
  FileImage,
  FileText,
  Globe2,
  Link2,
  Palette,
  ServerCog,
} from "lucide-react";
import type { ReactNode } from "react";
import { Streamdown, type ThemeInput } from "streamdown";

import { limitedCode } from "@/lib/limitedStreamdownCode";
import { arrField, boolField, isRecord, numField, strField, unwrapPayload } from "@/lib/payload";

const LIST_LIMIT = 18;
const STREAMDOWN_PLUGINS = { code: limitedCode };
const CODE_THEMES: [ThemeInput, ThemeInput] = ["one-dark-pro", "one-dark-pro"];

interface OperationResultViewProps {
  payload: unknown;
  subcommand: string;
}

type Tone = "info" | "success" | "warn" | "error" | "neutral" | "rose" | "violet";

export function hasStructuredOperationView(subcommand: string): boolean {
  return (
    [
      "query",
      "scrape",
      "search",
      "research",
      "crawl",
      "map",
      "suggest",
      "sources",
      "domains",
      "retrieve",
      "doctor",
      "embed",
      "extract",
      "ingest",
      "ingest-sessions-prepared",
      "endpoints",
      "brand",
      "diff",
      "screenshot",
      "dedupe",
      "watch-list",
      "watch-create",
      "watch-run",
    ].includes(subcommand) || isJobLifecycle(subcommand)
  );
}

export function OperationResultView({ payload, subcommand }: OperationResultViewProps) {
  const data = unwrapPayload(payload);

  switch (subcommand) {
    case "scrape":
      return <ReadingView payload={data} mode="scrape" />;
    case "query":
      return <RankedResultView title="Knowledge matches" payload={data} rowsKey="results" />;
    case "retrieve":
      return <ReadingView payload={data} mode="retrieve" />;
    case "search":
      return <SearchResultView payload={data} title="Web search" />;
    case "research":
      return <SearchResultView payload={data} title="Research brief" includeSummary />;
    case "map":
      return <UrlListView title="Discovered URLs" payload={data} keys={["urls"]} />;
    case "suggest":
      return <SuggestionView payload={data} />;
    case "sources":
      return <UrlListView title="Indexed sources" payload={data} keys={["urls", "sources"]} />;
    case "domains":
      return <DomainView payload={data} />;
    case "doctor":
      return <DoctorView payload={data} />;
    case "crawl":
      return <JobStartView payload={data} family="crawl" />;
    case "embed":
    case "extract":
    case "ingest":
    case "ingest-sessions-prepared":
      return <JobStartView payload={data} family={subcommand.replace("-sessions-prepared", "")} />;
    case "endpoints":
      return <EndpointView payload={data} />;
    case "brand":
      return <BrandView payload={data} />;
    case "diff":
      return <DiffView payload={data} />;
    case "screenshot":
      return <ScreenshotView payload={data} />;
    case "dedupe":
      return <DedupeView payload={data} />;
    case "watch-list":
      return <WatchListView payload={data} />;
    case "watch-create":
    case "watch-run":
      return <WatchDetailView payload={data} />;
    default:
      if (isJobLifecycle(subcommand)) return <JobLifecycleView payload={data} subcommand={subcommand} />;
      return <GenericResultView payload={data} />;
  }
}

function SearchResultView({
  payload,
  title,
  includeSummary,
}: {
  payload: Record<string, unknown>;
  title: string;
  includeSummary?: boolean;
}) {
  const summary = strField(payload, "summary");
  const rows = firstArray(payload, ["results", "search_results"]);
  const jobs = firstArray(payload, ["crawl_jobs", "jobs"]);

  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={<Globe2 size={16} />}
        title={title}
        tone={includeSummary ? "rose" : "info"}
        metrics={[
          ["Results", rows.length],
          ["Queued crawls", jobs.length],
        ]}
      />
      {includeSummary && summary ? (
        <section className="operation-section">
          <h3 className="stats-heading">Summary</h3>
          <div className="operation-markdown">
            <Streamdown>{summary}</Streamdown>
          </div>
        </section>
      ) : null}
      <ResultRows rows={rows} />
      {jobs.length > 0 ? <JobRows title="Queued crawl jobs" rows={jobs} /> : null}
    </div>
  );
}

function RankedResultView({
  title,
  payload,
  rowsKey,
}: {
  title: string;
  payload: Record<string, unknown>;
  rowsKey: string;
}) {
  const rows = arrField(payload, rowsKey);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={<ServerCog size={16} />}
        title={title}
        tone="neutral"
        metrics={[
          ["Matches", rows.length],
          ["Collection", strField(payload, "collection") ?? "axon"],
        ]}
      />
      <ResultRows rows={rows} preferSnippet />
    </div>
  );
}

function ReadingView({
  payload,
  mode,
}: {
  payload: Record<string, unknown>;
  mode: "scrape" | "retrieve";
}) {
  const markdown =
    strField(payload, "markdown") ??
    strField(payload, "content") ??
    strField(payload, "output") ??
    strField(payload, "text") ??
    strField(payload, "body");
  const readerMarkdown = sanitizeReaderMarkdown(markdown);
  const chunks = firstArray(payload, ["chunks", "documents", "results"]);

  return (
    <div className="output-body operation-view operation-reader-view aurora-scrollbar">
      {readerMarkdown ? (
        <section className="operation-section operation-reader-section">
          <div className="operation-reader">
            <Streamdown plugins={STREAMDOWN_PLUGINS} shikiTheme={CODE_THEMES}>
              {readerMarkdown}
            </Streamdown>
          </div>
        </section>
      ) : chunks.length > 0 ? (
        <ResultRows rows={chunks} preferSnippet />
      ) : (
        <EmptyResult kind={mode} />
      )}
    </div>
  );
}

function ResultRows({ rows, preferSnippet }: { rows: unknown[]; preferSnippet?: boolean }) {
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

function UrlListView({ title, payload, keys }: { title: string; payload: Record<string, unknown>; keys: string[] }) {
  const urls = firstArray(payload, keys).filter((item): item is string => typeof item === "string");
  const count = numField(payload, "count") ?? numField(payload, "total") ?? urls.length;
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<Link2 size={16} />} title={title} tone="info" metrics={[["Total", count]]} />
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

function SuggestionView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrField(payload, "suggestions");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<ArrowRight size={16} />} title="Suggested URLs" tone="violet" metrics={[["Suggestions", rows.length]]} />
      <ResultRows rows={rows} />
    </div>
  );
}

function DomainView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrField(payload, "domains");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<Globe2 size={16} />} title="Indexed domains" tone="neutral" metrics={[["Domains", rows.length]]} />
      <section className="operation-section">
        <div className="operation-table">
          {rows.slice(0, LIST_LIMIT).map((row, index) => {
            const record = isRecord(row) ? row : {};
            const domain = strField(record, "domain") ?? strField(record, "host") ?? `domain-${index + 1}`;
            const count = numField(record, "count") ?? numField(record, "chunks") ?? numField(record, "urls");
            return (
              <div key={domain} className="operation-table-row">
                <span>{domain}</span>
                <code>{count === undefined ? "indexed" : count.toLocaleString()}</code>
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}

function DoctorView({ payload }: { payload: Record<string, unknown> }) {
  const checks = firstArray(payload, ["checks", "findings", "services"]);
  const degraded = boolField(payload, "degraded") ?? checks.some((item) => isRecord(item) && isBadStatus(strField(item, "status")));
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={degraded ? <AlertTriangle size={16} /> : <CheckCircle2 size={16} />}
        title={degraded ? "Doctor found issues" : "Doctor checks passed"}
        tone={degraded ? "warn" : "success"}
        metrics={[
          ["Checks", checks.length],
          ["Status", degraded ? "degraded" : "healthy"],
        ]}
      />
      {checks.length === 0 ? (
        <GenericResultView payload={payload} embedded />
      ) : (
        <section className="operation-section">
          <div className="operation-list">
            {checks.slice(0, LIST_LIMIT).map((item, index) => {
              const check = isRecord(item) ? item : {};
              const status = strField(check, "status") ?? strField(check, "severity") ?? "unknown";
              const name = strField(check, "name") ?? strField(check, "service") ?? strField(check, "component") ?? `Check ${index + 1}`;
              const message = strField(check, "message") ?? strField(check, "detail") ?? strField(check, "error");
              return (
                <article key={`${name}-${index}`} className="operation-row">
                  <StatusDot status={status} />
                  <div className="operation-row-main">
                    <div className="operation-row-title">{name}</div>
                    {message ? <p className="operation-muted">{message}</p> : null}
                  </div>
                  <span className={`operation-badge operation-badge-${toneForStatus(status)}`}>{status}</span>
                </article>
              );
            })}
          </div>
        </section>
      )}
    </div>
  );
}

function JobStartView({ payload, family }: { payload: Record<string, unknown>; family: string }) {
  const result = isRecord(payload.result) ? payload.result : payload;
  const jobId = strField(result, "job_id") ?? strField(result, "id");
  const status = strField(result, "status") ?? strField(payload, "disposition") ?? "queued";
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={<Clock3 size={16} />}
        title={`${titleCase(family)} job ${status}`}
        tone={toneForStatus(status)}
        metrics={[
          ["Mode", strField(payload, "execution_mode") ?? "async"],
          ["Job", jobId ? (shortId(jobId) ?? jobId) : "pending"],
        ]}
      />
      <section className="operation-section">
        <div className="operation-detail-card">
          {jobId ? <DetailLine label="Job ID" value={jobId} mono /> : null}
          <DetailLine label="Status endpoint" value={strField(payload, "status_url") ?? `/v1/${family}/${jobId ?? "{job_id}"}`} mono />
          <DetailLine label="Next action" value={`${family}-status ${jobId ?? "<job_id>"}`} mono />
        </div>
      </section>
    </div>
  );
}

function JobLifecycleView({ payload, subcommand }: { payload: Record<string, unknown>; subcommand: string }) {
  const rows = firstArray(payload, ["jobs", "items"]);
  const match = subcommand.match(/^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/);
  const family = strField(payload, "family") ?? strField(payload, "kind") ?? match?.[1] ?? "job";
  const action = match?.[2] ?? "updated";
  const status = strField(payload, "status") ?? strField(payload, "state") ?? "updated";
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={<Clock3 size={16} />}
        title={`${titleCase(family)} ${titleCase(action)}`}
        tone={toneForStatus(status)}
        metrics={[
          ["Status", status],
          ["Jobs", rows.length || 1],
        ]}
      />
      {rows.length > 0 ? <JobRows rows={rows} /> : Object.keys(payload).length > 0 ? <JobRows rows={[payload]} /> : <EmptyResult kind="jobs" />}
    </div>
  );
}

function JobRows({ rows, title = "Jobs" }: { rows: unknown[]; title?: string }) {
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
                <div className="operation-row-title">{target ?? shortId(id) ?? `Job ${index + 1}`}</div>
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

function EndpointView({ payload }: { payload: Record<string, unknown> }) {
  const rows = firstArray(payload, ["endpoints", "candidates", "urls"]);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<ServerCog size={16} />} title="Endpoint discovery" tone="violet" metrics={[["Candidates", numField(payload, "total") ?? rows.length]]} />
      <ResultRows rows={rows.map((item) => (typeof item === "string" ? { url: item, title: item } : item))} />
    </div>
  );
}

function BrandView({ payload }: { payload: Record<string, unknown> }) {
  const colors = arrField(payload, "colors");
  const fonts = arrField(payload, "fonts").filter((item): item is string => typeof item === "string");
  const assets = firstArray(payload, ["logos", "assets"]);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<Palette size={16} />} title={strField(payload, "name") ?? "Brand identity"} tone="rose" metrics={[["Colors", colors.length], ["Fonts", fonts.length]]} />
      {colors.length > 0 ? (
        <section className="operation-section">
          <h3 className="stats-heading">Colors</h3>
          <div className="operation-swatches">
            {colors.slice(0, 12).map((item, index) => {
              const color = isRecord(item) ? strField(item, "hex") : undefined;
              const label = isRecord(item) ? strField(item, "usage") : undefined;
              return <Swatch key={`${color ?? index}`} color={color} label={label ?? color ?? "color"} />;
            })}
          </div>
        </section>
      ) : null}
      {fonts.length > 0 ? <ChipSection title="Fonts" values={fonts} /> : null}
      {assets.length > 0 ? <ResultRows rows={assets} /> : null}
    </div>
  );
}

function DiffView({ payload }: { payload: Record<string, unknown> }) {
  const metadata = arrField(payload, "metadata_changes");
  const added = arrField(payload, "links_added");
  const removed = arrField(payload, "links_removed");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={<FileText size={16} />}
        title={`Diff ${strField(payload, "status") ?? "complete"}`}
        tone={metadata.length || added.length || removed.length ? "warn" : "success"}
        metrics={[
          ["Word delta", numField(payload, "word_count_delta") ?? 0],
          ["Metadata", metadata.length],
          ["Added links", added.length],
          ["Removed links", removed.length],
        ]}
      />
      <section className="operation-section">
        <div className="operation-detail-card">
          <DetailLine label="Before" value={strField(payload, "url_a") ?? "-"} mono />
          <DetailLine label="After" value={strField(payload, "url_b") ?? "-"} mono />
        </div>
      </section>
      {metadata.length > 0 ? <ResultRows rows={metadata} /> : null}
    </div>
  );
}

function ScreenshotView({ payload }: { payload: Record<string, unknown> }) {
  const artifact = isRecord(payload.artifact_handle) ? payload.artifact_handle : {};
  const path = strField(payload, "path") ?? strField(artifact, "display_path");
  const previewSrc =
    imagePreviewSrc(strField(payload, "preview_url")) ??
    imagePreviewSrc(strField(payload, "image_url")) ??
    imagePreviewSrc(strField(payload, "data_url")) ??
    imagePreviewSrc(strField(artifact, "url")) ??
    imagePreviewSrc(path);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<FileImage size={16} />} title="Screenshot captured" tone="violet" metrics={[["Size", formatDetailValue("size_bytes", numField(payload, "size_bytes"))]]} />
      {previewSrc ? (
        <section className="operation-section">
          <figure className="operation-screenshot-preview">
            <img src={previewSrc} alt={`Screenshot of ${strField(payload, "url") ?? "captured page"}`} />
          </figure>
        </section>
      ) : null}
      <section className="operation-section">
        <div className="operation-detail-card">
          <DetailLine label="URL" value={strField(payload, "url") ?? "-"} mono />
          <DetailLine label="Path" value={path ?? "-"} mono />
        </div>
      </section>
    </div>
  );
}

function DedupeView({ payload }: { payload: Record<string, unknown> }) {
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={<ServerCog size={16} />}
        title="Dedupe complete"
        tone="success"
        metrics={[
          ["Removed", numField(payload, "removed") ?? numField(payload, "points_deleted") ?? 0],
          ["Scanned", numField(payload, "scanned") ?? numField(payload, "points_scanned") ?? "-"],
          ["Collection", strField(payload, "collection") ?? "axon"],
        ]}
      />
    </div>
  );
}

function WatchListView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrField(payload, "watches");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<Clock3 size={16} />} title="Watch schedules" tone="neutral" metrics={[["Watches", rows.length]]} />
      {rows.length > 0 ? <ResultRows rows={rows} /> : <EmptyResult kind="watches" />}
    </div>
  );
}

function WatchDetailView({ payload }: { payload: Record<string, unknown> }) {
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<Clock3 size={16} />} title={strField(payload, "name") ?? "Watch updated"} tone="success" metrics={[["Artifacts", arrField(payload, "artifacts").length]]} />
      <GenericResultView payload={payload} embedded />
    </div>
  );
}

function GenericResultView({ payload, embedded }: { payload: Record<string, unknown>; embedded?: boolean }) {
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

function ResultHero({
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

function DetailLine({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="operation-detail-line">
      <span>{label}</span>
      <strong className={mono ? "operation-mono" : undefined}>{value}</strong>
    </div>
  );
}

function ChipSection({ title, values }: { title: string; values: string[] }) {
  return (
    <section className="operation-section">
      <h3 className="stats-heading">{title}</h3>
      <div className="operation-chip-row">
        {values.slice(0, LIST_LIMIT).map((value) => (
          <span key={value} className="operation-chip">{value}</span>
        ))}
      </div>
    </section>
  );
}

function Swatch({ color, label }: { color?: string; label: string }) {
  return (
    <div className="operation-swatch">
      <span style={color ? { background: color } : undefined} />
      <strong>{label}</strong>
      {color ? <code>{color}</code> : null}
    </div>
  );
}

function EmptyResult({ kind = "generic" }: { kind?: EmptyKind }) {
  const copy = emptyCopy(kind);
  return (
    <div className="operation-empty">
      <strong>{copy.title}</strong>
      <span>{copy.body}</span>
    </div>
  );
}

function StatusDot({ status }: { status: string }) {
  return <span className={`operation-dot operation-dot-${toneForStatus(status)}`} aria-hidden="true" />;
}

function firstArray(payload: Record<string, unknown>, keys: string[]): unknown[] {
  for (const key of keys) {
    const value = arrField(payload, key);
    if (value.length > 0) return value;
  }
  return [];
}

function isJobLifecycle(subcommand: string): boolean {
  return /^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/.test(subcommand);
}

function hostLabel(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url.split("/")[0] || url;
  }
}

type EmptyKind = "generic" | "results" | "urls" | "sources" | "jobs" | "watches" | "scrape" | "retrieve";

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

function imagePreviewSrc(path: string | undefined): string | undefined {
  if (!path) return undefined;
  if (/^https?:\/\//i.test(path) || path.startsWith("data:image/")) return path;
  if (!/\.(png|jpe?g|webp|gif|avif)$/i.test(path)) return undefined;
  return path.startsWith("/") ? `file://${path}` : path;
}

function shortId(id: string | undefined): string | undefined {
  if (!id) return undefined;
  return id.length > 14 ? `${id.slice(0, 8)}...${id.slice(-4)}` : id;
}

function titleCase(value: string): string {
  return value.charAt(0).toUpperCase() + value.slice(1);
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

function formatDetailValue(key: string, value: unknown): string {
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

function isBadStatus(status: string | undefined): boolean {
  return status === "error" || status === "failed" || status === "degraded" || status === "warn";
}

function toneForStatus(status: string | undefined): Tone {
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
