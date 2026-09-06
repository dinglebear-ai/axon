import { ChevronDown, Copy, ExternalLink, Map as MapIcon } from "lucide-react";
import { useState } from "react";

import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { ResultRows } from "@/components/palette/OperationResultRows";
import {
  arrayByKeys,
  EmptyResult,
  sanitizeReaderMarkdown,
} from "@/components/palette/OperationResultViewShared";
import { WorkspaceHeader, WorkspaceSurface } from "@/components/palette/WorkspaceSurface";
import { Button } from "@/components/ui/aurora/button";
import { arrField, numField, strField } from "@/lib/payload";

const LIST_LIMIT = 18;

export function MapResultView({ payload }: { payload: Record<string, unknown> }) {
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

export function ReadingView({
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

export function SuggestionView({ payload }: { payload: Record<string, unknown> }) {
  const rows = arrField(payload, "suggestions");
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultRows rows={rows} preferSnippet title="Suggested URLs" />
    </div>
  );
}
