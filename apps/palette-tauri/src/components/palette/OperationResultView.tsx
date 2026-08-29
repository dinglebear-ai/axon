import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  Clock3,
  Copy,
  Download,
  ExternalLink,
  FileImage,
  FileText,
  Map as MapIcon,
  Maximize2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import { memo, type ReactNode, useState } from "react";

import { AuthenticatedArtifactImage } from "@/components/palette/AuthenticatedArtifactImage";
import { FilesView } from "@/components/palette/FilesView";
import { GitHubView } from "@/components/palette/GitHubView";
import { HelpResultView } from "@/components/palette/HelpResultView";
import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { ResultRows } from "@/components/palette/OperationResultRows";
import {
  arrayByKeys,
  ChipSection,
  DetailLine,
  EmptyResult,
  GenericResultView,
  imagePreviewSrc,
  isBadStatus,
  JobRows,
  ResultHero,
  ResultSummary,
  StatusDot,
  Swatch,
  sanitizeReaderMarkdown,
  toneForStatus,
  UrlListView,
} from "@/components/palette/OperationResultViewShared";
import { RankedResultView, SearchResultView } from "@/components/palette/SearchResultViews";
import { WorkspaceHeader, WorkspaceSurface } from "@/components/palette/WorkspaceSurface";
import { Button } from "@/components/ui/aurora/button";
import { actionBehavior, maybeActionBehavior, type StructuredViewKey } from "@/lib/actionRegistry";
import type { Client, PaletteConfig } from "@/lib/axonClient";
import {
  arrField,
  boolField,
  isRecord,
  numField,
  shortId,
  strField,
  titleCase,
  unwrapPayload,
} from "@/lib/payload";

const LIST_LIMIT = 18;

export { sanitizeReaderMarkdown } from "@/components/palette/OperationResultViewShared";

interface OperationResultViewProps {
  payload: unknown;
  subcommand: string;
  fallbackText?: string;
  /** Live Axon client + config — only consumed by the `files` view (source-index
   * needs a real request), every other structured view is payload-only. */
  client?: Client | null;
  config?: PaletteConfig | null;
}

// Renderer dispatch (A-H1): keyed by the registry's `StructuredViewKey` union, so
// `Record<StructuredViewKey, …>` forces an entry for every view the registry can
// reference — a new structured view fails to type-check until it is rendered
// here. The subcommand → view-key mapping lives in `actionRegistry.ts`
// (`ActionBehavior.structuredView`); `hasStructuredOperationView` derives from it.
// Each entry renders the unwrapped `data`; raw `payload`/`fallbackText` are passed
// through for views that need them (help). Job-lifecycle subcommands all share the
// single `"job-lifecycle"` key. `client`/`config` are only populated for `files`
// (a stateful local browser, not a payload render) — every other view ignores them.
type ViewContext = {
  data: Record<string, unknown>;
  payload: unknown;
  fallbackText: string;
  subcommand: string;
  client?: Client | null;
  config?: PaletteConfig | null;
};

const STRUCTURED_VIEWS: Record<StructuredViewKey, (ctx: ViewContext) => ReactNode> = {
  help: ({ payload, fallbackText }) => (
    <HelpResultView payload={payload} fallbackText={fallbackText} />
  ),
  files: ({ client, config }) => <FilesView client={client ?? null} config={config ?? null} />,
  scrape: ({ data }) => <ReadingView payload={data} mode="scrape" />,
  query: ({ data }) => (
    <RankedResultView title="Knowledge matches" payload={data} rowsKey="results" />
  ),
  retrieve: ({ data }) => <ReadingView payload={data} mode="retrieve" />,
  search: ({ data }) => <SearchResultView payload={data} title="Web search" />,
  research: ({ data }) => <SearchResultView payload={data} title="Research brief" includeSummary />,
  map: ({ data }) => <MapResultView payload={data} />,
  suggest: ({ data }) => <SuggestionView payload={data} />,
  sources: ({ data }) => (
    <UrlListView title="Indexed sources" payload={data} keys={["urls", "sources"]} />
  ),
  domains: ({ data }) => <DomainView payload={data} />,
  doctor: ({ data }) => <DoctorView payload={data} />,
  "source-site": ({ data }) => <JobStartView payload={data} family="source" />,
  source: ({ data }) => <JobStartView payload={data} family="source" />,
  extract: ({ data }) => <JobStartView payload={data} family="extract" />,
  // GitHubView needs the WHOLE GitHubBrowseResult (ok/kind/owner/repo/branch/
  // path/rateLimit*), not the inner GitHub JSON `unwrapPayload` would leave
  // after stripping `.payload` — pass the raw payload through instead of `data`.
  github: ({ payload }) => <GitHubView payload={isRecord(payload) ? payload : {}} />,
  endpoints: ({ data }) => <EndpointView payload={data} />,
  brand: ({ data }) => <BrandView payload={data} />,
  diff: ({ data }) => <DiffView payload={data} />,
  screenshot: ({ data }) => <ScreenshotView payload={data} />,
  "watch-list": ({ data }) => <WatchListView payload={data} />,
  "watch-create": ({ data }) => <WatchDetailView payload={data} />,
  "watch-run": ({ data }) => <WatchDetailView payload={data} />,
  "job-lifecycle": ({ data, subcommand }) => (
    <JobLifecycleView payload={data} subcommand={subcommand} />
  ),
};

export function hasStructuredOperationView(subcommand: string): boolean {
  return actionBehavior(subcommand).structuredView !== null;
}

export const OperationResultView = memo(function OperationResultView({
  payload,
  subcommand,
  fallbackText = "",
  client,
  config,
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
  if (render) return render({ data, payload, fallbackText, subcommand, client, config });
  return <GenericResultView payload={data} />;
});

function MapResultView({ payload }: { payload: Record<string, unknown> }) {
  const [expandedUrl, setExpandedUrl] = useState<string | null>(null);
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);
  const urls = arrayByKeys(payload, ["urls"]).filter(
    (item): item is string => typeof item === "string",
  );
  const count = numField(payload, "count") ?? numField(payload, "total") ?? urls.length;
  const origin = urls[0] ? new URL(urls[0]).host : "Site map";
  return (
    <WorkspaceSurface className="output-body operation-view operation-map-view">
      <WorkspaceHeader
        icon={MapIcon}
        eyebrow="Site Inventory"
        title={origin}
        description="Discovered routes organized into an expandable URL inventory."
        metrics={[{ label: "URLs", value: count.toLocaleString() }]}
      />
      {urls.length === 0 ? (
        <EmptyResult kind="urls" />
      ) : (
        <section className="operation-map-list" aria-label="Discovered URLs">
          {urls.slice(0, LIST_LIMIT * 3).map((url, index) => {
            const parsedUrl = new URL(url);
            const path = `${parsedUrl.pathname}${parsedUrl.search}` || "/";
            const metadata = mapUrlMetadata(parsedUrl);
            const expanded = expandedUrl === url;
            return (
              <div key={url} className={`operation-map-item${expanded ? " is-expanded" : ""}`}>
                <button
                  type="button"
                  className="operation-map-row"
                  aria-expanded={expanded}
                  onClick={() => setExpandedUrl(expanded ? null : url)}
                >
                  <span className="operation-map-index">{String(index + 1).padStart(2, "0")}</span>
                  <span className="operation-map-path">{path}</span>
                  <span className="operation-map-host">{parsedUrl.host}</span>
                  <ChevronDown className="operation-map-chevron" size={14} aria-hidden="true" />
                </button>
                {expanded && (
                  <div className="operation-map-inline">
                    <div className="operation-map-inline-main">
                      <span className="operation-map-full-url">{url}</span>
                      <span className="operation-map-details">
                        {metadata.map((detail) => (
                          <span key={detail}>{detail}</span>
                        ))}
                      </span>
                    </div>
                    <div className="operation-map-actions">
                      <Button
                        variant="ghost"
                        size="sm"
                        type="button"
                        onClick={() => {
                          void navigator.clipboard.writeText(url).then(() => {
                            setCopiedUrl(url);
                            window.setTimeout(() => setCopiedUrl(null), 1400);
                          });
                        }}
                      >
                        <Copy size={13} aria-hidden="true" />
                        {copiedUrl === url ? "Copied" : "Copy URL"}
                      </Button>
                      <Button variant="ghost" size="sm" asChild>
                        <a href={url} target="_blank" rel="noreferrer" aria-label="Open in browser">
                          <ExternalLink size={13} aria-hidden="true" />
                          Open in Browser
                        </a>
                      </Button>
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </section>
      )}
    </WorkspaceSurface>
  );
}

function mapUrlMetadata(url: URL): string[] {
  const segments = url.pathname.split("/").filter(Boolean);
  const filename = segments.at(-1) ?? "";
  const extension = filename.includes(".") ? filename.split(".").at(-1)?.toLowerCase() : undefined;
  const type =
    extension === "md"
      ? "Markdown"
      : extension === "json"
        ? "JSON"
        : extension === "xml"
          ? "XML"
          : extension === "pdf"
            ? "PDF"
            : extension
              ? extension.toUpperCase()
              : "Web page";
  const locale = segments.find((segment) => /^[a-z]{2}(?:-[A-Z]{2})?$/.test(segment));
  const section = segments.length > 1 ? segments.slice(0, -1).join(" / ") : undefined;
  return [
    type,
    `Depth ${segments.length}`,
    locale ? `Locale ${locale}` : undefined,
    section ? `Section ${section}` : undefined,
    url.searchParams.size ? `${url.searchParams.size} query params` : undefined,
  ].filter((value): value is string => Boolean(value));
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
        <ResultRows
          rows={chunks}
          preferSnippet
          title={mode === "retrieve" ? "Stored passages" : "Page content"}
        />
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
      <ResultRows rows={rows} preferSnippet title="Suggested URLs" />
    </div>
  );
}

function DomainView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrField(payload, "domains");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <header className="operation-collection-header">
        <div>
          <span className="operation-section-eyebrow">Collection</span>
          <h3>Indexed domains</h3>
        </div>
        <strong>{rows.length.toLocaleString()} total</strong>
      </header>
      <section className="operation-section">
        <div className="operation-table">
          {rows.slice(0, LIST_LIMIT).map((row, index) => {
            const record = isRecord(row) ? row : {};
            const domain =
              strField(record, "domain") ?? strField(record, "host") ?? `domain-${index + 1}`;
            const count =
              numField(record, "count") ?? numField(record, "chunks") ?? numField(record, "urls");
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
  const degraded =
    boolField(payload, "degraded") ??
    checks.some((item) => isRecord(item) && isBadStatus(strField(item, "status")));
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
              const name =
                strField(check, "name") ??
                strField(check, "service") ??
                strField(check, "component") ??
                `Check ${index + 1}`;
              const message =
                strField(check, "message") ?? strField(check, "detail") ?? strField(check, "error");
              return (
                <article key={`${name}-${status}-${message ?? ""}`} className="operation-row">
                  <StatusDot status={status} />
                  <div className="operation-row-main">
                    <div className="operation-row-title">{name}</div>
                    {message ? <p className="operation-muted">{message}</p> : null}
                  </div>
                  <span className={`operation-badge operation-badge-${toneForStatus(status)}`}>
                    {status}
                  </span>
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
  const statusEndpoint = strField(payload, "status_url") ?? `/v1/jobs/${jobId ?? "{job_id}"}`;
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
          <DetailLine label="Status endpoint" value={statusEndpoint} mono />
          <DetailLine
            label="Next action"
            value={jobId ? `open job ${jobId}` : "open job <job_id>"}
            mono
          />
        </div>
      </section>
    </div>
  );
}

function JobLifecycleView({
  payload,
  subcommand,
}: {
  payload: Record<string, unknown>;
  subcommand: string;
}) {
  const rows = arrayByKeys(payload, ["jobs", "items"]);
  const match = subcommand.match(/^(extract)-(list|status|cancel|cleanup|clear|recover)$/);
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
      {rows.length > 0 ? (
        <JobRows rows={rows} />
      ) : Object.keys(payload).length > 0 ? (
        <JobRows rows={[payload]} />
      ) : (
        <EmptyResult kind="jobs" />
      )}
    </div>
  );
}

function EndpointView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrayByKeys(payload, ["endpoints", "candidates", "urls"]);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultRows
        title="Endpoint discovery"
        rows={rows.map((item) => (typeof item === "string" ? { url: item, title: item } : item))}
      />
    </div>
  );
}

function BrandView({ payload }: { payload: Record<string, unknown> }) {
  const colors = arrField(payload, "colors");
  const fonts = arrField(payload, "fonts").filter(
    (item): item is string => typeof item === "string",
  );
  const assets = arrayByKeys(payload, ["logos", "assets"]);
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultSummary
        metrics={[
          ["Colors", colors.length],
          ["Fonts", fonts.length],
          ["View", strField(payload, "name") ?? "Brand identity"],
        ]}
      />
      {colors.length > 0 ? (
        <section className="operation-section">
          <h3 className="stats-heading">Colors</h3>
          <div className="operation-swatches">
            {colors.slice(0, 12).map((item, index) => {
              const color = isRecord(item) ? strField(item, "hex") : undefined;
              const label = isRecord(item) ? strField(item, "usage") : undefined;
              return (
                <Swatch key={`${color ?? index}`} color={color} label={label ?? color ?? "color"} />
              );
            })}
          </div>
        </section>
      ) : null}
      {fonts.length > 0 ? <ChipSection title="Fonts" values={fonts} /> : null}
      {assets.length > 0 ? <ResultRows rows={assets} title="Brand assets" /> : null}
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
      {metadata.length > 0 ? <ResultRows rows={metadata} title="Metadata changes" /> : null}
    </div>
  );
}

function ScreenshotView({ payload }: { payload: Record<string, unknown> }) {
  const [zoom, setZoom] = useState(100);
  const [fullscreen, setFullscreen] = useState(false);
  const artifact = isRecord(payload.artifact_handle) ? payload.artifact_handle : {};
  const artifactId = strField(payload, "artifact_id") ?? strField(artifact, "artifact_id");
  const previewSrc =
    imagePreviewSrc(strField(payload, "preview_url")) ??
    imagePreviewSrc(strField(payload, "image_url")) ??
    imagePreviewSrc(strField(payload, "data_url")) ??
    imagePreviewSrc(strField(artifact, "url"));
  const alt = "Captured screenshot";
  const width = numField(payload, "width") ?? 0;
  const height = numField(payload, "height") ?? 0;
  return (
    <WorkspaceSurface
      className={`output-body operation-view screenshot-workspace${fullscreen ? " is-fullscreen" : ""}`}
    >
      <WorkspaceHeader
        icon={FileImage}
        eyebrow="Visual Artifact"
        title="Screenshot Captured"
        description="Inspect, zoom, and export the rendered page capture."
        metrics={[
          { label: "Width", value: width.toLocaleString() },
          { label: "Height", value: height.toLocaleString() },
        ]}
        actions={
          previewSrc ? (
            <>
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                onClick={() => setZoom((value) => Math.max(25, value - 25))}
                aria-label="Zoom out"
              >
                <ZoomOut size={14} />
              </Button>
              <span className="screenshot-zoom-value">{zoom}%</span>
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                onClick={() => setZoom((value) => Math.min(200, value + 25))}
                aria-label="Zoom in"
              >
                <ZoomIn size={14} />
              </Button>
              <Button
                variant="plain"
                size="unstyled"
                type="button"
                onClick={() => setFullscreen((value) => !value)}
                aria-label="Toggle screenshot fullscreen"
              >
                <Maximize2 size={14} />
              </Button>
              <Button variant="plain" size="unstyled" asChild>
                <a
                  href={previewSrc}
                  download="axon-screenshot.png"
                  aria-label="Download screenshot"
                >
                  <Download size={14} />
                </a>
              </Button>
            </>
          ) : undefined
        }
      />
      {previewSrc ? (
        <section className="operation-section">
          <figure className="operation-screenshot-preview">
            <img src={previewSrc} alt={alt} style={{ width: `${zoom}%` }} />
          </figure>
        </section>
      ) : artifactId ? (
        <AuthenticatedArtifactImage artifactId={artifactId} alt={alt} />
      ) : null}
      <section className="operation-section">
        <div className="operation-detail-card">
          <DetailLine label="Artifact" value={artifactId ?? "-"} />
          <DetailLine label="Captured" value={strField(payload, "captured_at") ?? "-"} />
        </div>
      </section>
    </WorkspaceSurface>
  );
}

function WatchListView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrField(payload, "watches");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      {rows.length > 0 ? (
        <ResultRows rows={rows} title="Watch schedules" />
      ) : (
        <EmptyResult kind="watches" />
      )}
    </div>
  );
}

function WatchDetailView({ payload }: { payload: Record<string, unknown> }) {
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultHero
        icon={<Clock3 size={16} />}
        title={strField(payload, "name") ?? "Watch updated"}
        tone="success"
        metrics={[["Artifacts", arrField(payload, "artifacts").length]]}
      />
      <GenericResultView payload={payload} embedded />
    </div>
  );
}
