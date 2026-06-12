import {
  AlertTriangle,
  ArrowRight,
  CheckCircle2,
  Clock3,
  FileImage,
  FileText,
  Globe2,
  Palette,
  ServerCog,
} from "lucide-react";
import { Streamdown } from "streamdown";

import {
  ChipSection,
  DetailLine,
  EmptyResult,
  GenericResultView,
  JobRows,
  ResultRows,
  ResultHero,
  StatusDot,
  Swatch,
  UrlListView,
  firstArray,
  formatDetailValue,
  imagePreviewSrc,
  isBadStatus,
  isJobLifecycle,
  sanitizeReaderMarkdown,
  shortId,
  titleCase,
  toneForStatus,
} from "@/components/palette/OperationResultViewShared";
import { arrField, boolField, isRecord, numField, strField, unwrapPayload } from "@/lib/payload";
import { STREAMDOWN_CODE_THEMES, STREAMDOWN_PLUGINS } from "@/lib/streamdownConfig";

const LIST_LIMIT = 18;
export { sanitizeReaderMarkdown } from "@/components/palette/OperationResultViewShared";

interface OperationResultViewProps {
  payload: unknown;
  subcommand: string;
}

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
            <Streamdown plugins={STREAMDOWN_PLUGINS} shikiTheme={STREAMDOWN_CODE_THEMES}>
              {summary}
            </Streamdown>
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
            <Streamdown plugins={STREAMDOWN_PLUGINS} shikiTheme={STREAMDOWN_CODE_THEMES}>
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
