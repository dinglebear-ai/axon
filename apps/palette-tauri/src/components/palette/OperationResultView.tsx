import { memo, type ReactNode } from "react";
import {
  AlertTriangle,
  CheckCircle2,
  Clock3,
  FileImage,
  FileText,
  ServerCog,
} from "lucide-react";

import { AuthenticatedArtifactImage } from "@/components/palette/AuthenticatedArtifactImage";
import { HelpResultView } from "@/components/palette/HelpResultView";
import { MarkdownBody } from "@/components/palette/MarkdownBody";
import {
  ChipSection,
  DetailLine,
  EmptyResult,
  GenericResultView,
  JobRows,
  ResultRows,
  ResultHero,
  ResultSummary,
  StatusDot,
  Swatch,
  UrlListView,
  arrayByKeys,
  formatDetailValue,
  imagePreviewSrc,
  isBadStatus,
  sanitizeReaderMarkdown,
  toneForStatus,
} from "@/components/palette/OperationResultViewShared";
import { actionBehavior, maybeActionBehavior, type StructuredViewKey } from "@/lib/actionRegistry";
import { arrField, boolField, isRecord, numField, shortId, strField, titleCase, unwrapPayload } from "@/lib/payload";

const LIST_LIMIT = 18;
export { sanitizeReaderMarkdown } from "@/components/palette/OperationResultViewShared";

interface OperationResultViewProps {
  payload: unknown;
  subcommand: string;
  fallbackText?: string;
}

// Renderer dispatch (A-H1): keyed by the registry's `StructuredViewKey` union, so
// `Record<StructuredViewKey, …>` forces an entry for every view the registry can
// reference — a new structured view fails to type-check until it is rendered
// here. The subcommand → view-key mapping lives in `actionRegistry.ts`
// (`ActionBehavior.structuredView`); `hasStructuredOperationView` derives from it.
// Each entry renders the unwrapped `data`; raw `payload`/`fallbackText` are passed
// through for views that need them (help). Job-lifecycle subcommands all share the
// single `"job-lifecycle"` key.
type ViewContext = { data: Record<string, unknown>; payload: unknown; fallbackText: string; subcommand: string };

const STRUCTURED_VIEWS: Record<StructuredViewKey, (ctx: ViewContext) => ReactNode> = {
  help: ({ payload, fallbackText }) => <HelpResultView payload={payload} fallbackText={fallbackText} />,
  scrape: ({ data }) => <ReadingView payload={data} mode="scrape" />,
  query: ({ data }) => <RankedResultView title="Knowledge matches" payload={data} rowsKey="results" />,
  retrieve: ({ data }) => <ReadingView payload={data} mode="retrieve" />,
  search: ({ data }) => <SearchResultView payload={data} title="Web search" />,
  research: ({ data }) => <SearchResultView payload={data} title="Research brief" includeSummary />,
  map: ({ data }) => <UrlListView title="Discovered URLs" payload={data} keys={["urls"]} />,
  suggest: ({ data }) => <SuggestionView payload={data} />,
  sources: ({ data }) => <UrlListView title="Indexed sources" payload={data} keys={["urls", "sources"]} />,
  domains: ({ data }) => <DomainView payload={data} />,
  doctor: ({ data }) => <DoctorView payload={data} />,
  crawl: ({ data }) => <JobStartView payload={data} family="crawl" />,
  embed: ({ data }) => <JobStartView payload={data} family="embed" />,
  extract: ({ data }) => <JobStartView payload={data} family="extract" />,
  ingest: ({ data }) => <JobStartView payload={data} family="ingest" />,
  "ingest-sessions-prepared": ({ data }) => <JobStartView payload={data} family="ingest" />,
  endpoints: ({ data }) => <EndpointView payload={data} />,
  brand: ({ data }) => <BrandView payload={data} />,
  diff: ({ data }) => <DiffView payload={data} />,
  screenshot: ({ data }) => <ScreenshotView payload={data} />,
  dedupe: ({ data }) => <DedupeView payload={data} />,
  "watch-list": ({ data }) => <WatchListView payload={data} />,
  "watch-create": ({ data }) => <WatchDetailView payload={data} />,
  "watch-run": ({ data }) => <WatchDetailView payload={data} />,
  "job-lifecycle": ({ data, subcommand }) => <JobLifecycleView payload={data} subcommand={subcommand} />,
};

export function hasStructuredOperationView(subcommand: string): boolean {
  return actionBehavior(subcommand).structuredView !== null;
}

export const OperationResultView = memo(function OperationResultView({
  payload,
  subcommand,
  fallbackText = "",
}: OperationResultViewProps) {
  const data = unwrapPayload(payload);
  const behavior = maybeActionBehavior(subcommand);
  if (!behavior) {
    return (
      <div className="operation-empty" role="alert">
        <strong>Unknown palette action</strong>
        <span>{subcommand}</span>
      </div>
    );
  }
  const viewKey = behavior.structuredView;
  const render = viewKey ? STRUCTURED_VIEWS[viewKey] : undefined;
  if (render) return render({ data, payload, fallbackText, subcommand });
  return <GenericResultView payload={data} />;
});

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
  const rows = arrayByKeys(payload, ["results", "search_results"]);
  const jobs = arrayByKeys(payload, ["crawl_jobs", "jobs"]);

  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultSummary metrics={[["Results", rows.length], ["Queued crawls", jobs.length], ["View", title]]} />
      {includeSummary && summary ? (
        <section className="operation-section">
          <h3 className="stats-heading">Summary</h3>
          <div className="operation-markdown">
            <MarkdownBody>{summary}</MarkdownBody>
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
      <ResultSummary metrics={[["Matches", rows.length], ["Collection", strField(payload, "collection") ?? "axon"], ["View", title]]} />
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
  const chunks = arrayByKeys(payload, ["chunks", "documents", "results"]);

  return (
    <div className="output-body operation-view operation-reader-view aurora-scrollbar">
      {readerMarkdown ? (
        <section className="operation-section operation-reader-section">
          <div className="operation-reader">
            <MarkdownBody>{readerMarkdown}</MarkdownBody>
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
      <ResultSummary metrics={[["Suggestions", rows.length], ["View", "Suggested URLs"]]} />
      <ResultRows rows={rows} />
    </div>
  );
}

function DomainView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrField(payload, "domains");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultSummary metrics={[["Domains", rows.length], ["View", "Indexed domains"]]} />
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
  const checks = arrayByKeys(payload, ["checks", "findings", "services"]);
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
          ["Job", jobId ? shortId(jobId) : "pending"],
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
  const rows = arrayByKeys(payload, ["jobs", "items"]);
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
  const rows = arrayByKeys(payload, ["endpoints", "candidates", "urls"]);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultSummary metrics={[["Candidates", numField(payload, "total") ?? rows.length], ["View", "Endpoint discovery"]]} />
      <ResultRows rows={rows.map((item) => (typeof item === "string" ? { url: item, title: item } : item))} />
    </div>
  );
}

function BrandView({ payload }: { payload: Record<string, unknown> }) {
  const colors = arrField(payload, "colors");
  const fonts = arrField(payload, "fonts").filter((item): item is string => typeof item === "string");
  const assets = arrayByKeys(payload, ["logos", "assets"]);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultSummary metrics={[["Colors", colors.length], ["Fonts", fonts.length], ["View", strField(payload, "name") ?? "Brand identity"]]} />
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
  const relativePath = strField(artifact, "relative_path");
  const artifactDisplay = strField(artifact, "display_path") ?? relativePath;
  const previewSrc =
    imagePreviewSrc(strField(payload, "preview_url")) ??
    imagePreviewSrc(strField(payload, "image_url")) ??
    imagePreviewSrc(strField(payload, "data_url")) ??
    imagePreviewSrc(strField(artifact, "url"));
  const alt = `Screenshot of ${strField(payload, "url") ?? "captured page"}`;
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero icon={<FileImage size={16} />} title="Screenshot captured" tone="violet" metrics={[["Size", formatDetailValue("size_bytes", numField(payload, "size_bytes"))]]} />
      {previewSrc ? (
        <section className="operation-section">
          <figure className="operation-screenshot-preview">
            <img src={previewSrc} alt={alt} />
          </figure>
        </section>
      ) : relativePath ? (
        <AuthenticatedArtifactImage relativePath={relativePath} alt={alt} />
      ) : null}
      <section className="operation-section">
        <div className="operation-detail-card">
          <DetailLine label="URL" value={strField(payload, "url") ?? "-"} mono />
          <DetailLine label="Artifact" value={artifactDisplay ?? "-"} mono />
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
      <ResultSummary metrics={[["Watches", rows.length], ["View", "Watch schedules"]]} />
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
