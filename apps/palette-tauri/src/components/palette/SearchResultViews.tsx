import { Check, ChevronDown, Copy, ExternalLink, Search, SearchX } from "lucide-react";
import { useState } from "react";
import { FilesContextMenu } from "@/components/palette/FilesContextMenu";
import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { ResultRows } from "@/components/palette/OperationResultRows";
import { arrayByKeys, JobRows } from "@/components/palette/OperationResultViewShared";
import {
  WorkspaceEmpty,
  WorkspaceHeader,
  WorkspaceSurface,
} from "@/components/palette/WorkspaceSurface";
import { Button } from "@/components/ui/aurora/button";
import { arrField, isRecord, numField, strField } from "@/lib/payload";
import { hostLabel } from "@/lib/url";

const SEARCH_RESULT_LIMIT = 18;

export function SearchResultView({
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
  const jobs = arrayByKeys(payload, ["source_jobs", "jobs"]);
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set());
  const [copiedUrl, setCopiedUrl] = useState<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    x: number;
    y: number;
    url: string;
    title: string;
  } | null>(null);

  function toggleExpanded(index: number) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  }

  function copyUrl(url: string) {
    void navigator.clipboard.writeText(url).then(() => {
      setCopiedUrl(url);
      window.setTimeout(() => setCopiedUrl(null), 1200);
    });
  }

  return (
    <WorkspaceSurface className="output-body operation-view search-results-view">
      <WorkspaceHeader
        icon={Search}
        eyebrow={title === "Web search" ? "Discovery" : "Research"}
        title={title}
        description="Ranked results with source context and inline actions."
        metrics={[
          {
            label: title === "Web search" ? "Results" : "Sources",
            value: rows.length.toLocaleString(),
          },
          { label: "Queued", value: jobs.length.toLocaleString() },
        ]}
      />

      {includeSummary && summary ? (
        <section className="search-results-brief">
          <span>Brief</span>
          <div className="operation-markdown">
            <MarkdownBody>{summary}</MarkdownBody>
          </div>
        </section>
      ) : null}

      {rows.length === 0 ? (
        <WorkspaceEmpty
          icon={SearchX}
          title="No results found."
          description="Try a broader query or remove a source constraint."
        />
      ) : (
        <section className="search-results-list" aria-label="Search results">
          {rows.slice(0, SEARCH_RESULT_LIMIT).map((row, index) => {
            const record = isRecord(row) ? row : {};
            const url = strField(record, "url") ?? strField(record, "source_url");
            const titleText =
              strField(record, "title") ?? strField(record, "name") ?? url ?? `Result ${index + 1}`;
            const snippet =
              strField(record, "snippet") ??
              strField(record, "content") ??
              strField(record, "text") ??
              strField(record, "reason");
            const score = numField(record, "score");
            const isExpanded = expanded.has(index);
            return (
              <article
                key={url ?? titleText}
                className={`search-result-card${isExpanded ? " is-expanded" : ""}`}
                onContextMenu={(event) => {
                  if (!url) return;
                  event.preventDefault();
                  setContextMenu({ x: event.clientX, y: event.clientY, url, title: titleText });
                }}
              >
                <span className="search-result-rank">{String(index + 1).padStart(2, "0")}</span>
                <div className="search-result-content">
                  <div className="search-result-source">
                    <span className="search-result-favicon" aria-hidden="true">
                      {(url ? hostLabel(url) : titleText).charAt(0).toUpperCase()}
                    </span>
                    <span>{url ? hostLabel(url) : "Result"}</span>
                    {score !== undefined ? <small>{score.toFixed(2)} relevance</small> : null}
                  </div>
                  <h3>
                    {url ? (
                      <a href={url} target="_blank" rel="noopener noreferrer">
                        {titleText}
                      </a>
                    ) : (
                      titleText
                    )}
                  </h3>
                  {snippet ? <p>{snippet}</p> : null}
                  {snippet && snippet.length > 220 ? (
                    <button
                      type="button"
                      className="search-result-expand"
                      onClick={() => toggleExpanded(index)}
                      aria-expanded={isExpanded}
                    >
                      {isExpanded ? "Show less" : "Read more"}
                      <ChevronDown size={12} />
                    </button>
                  ) : null}
                </div>
                {url ? (
                  <div className="search-result-actions">
                    <Button
                      variant="plain"
                      size="unstyled"
                      type="button"
                      onClick={() => copyUrl(url)}
                      aria-label={
                        copiedUrl === url ? `Copied ${titleText} URL` : `Copy ${titleText} URL`
                      }
                      title={copiedUrl === url ? "Copied" : "Copy URL"}
                    >
                      {copiedUrl === url ? <Check size={13} /> : <Copy size={13} />}
                    </Button>
                    <Button
                      variant="plain"
                      size="unstyled"
                      type="button"
                      onClick={() => window.open(url, "_blank", "noopener,noreferrer")}
                      aria-label={`Open ${titleText}`}
                      title="Open result"
                    >
                      <ExternalLink size={13} />
                    </Button>
                  </div>
                ) : null}
              </article>
            );
          })}
        </section>
      )}

      {jobs.length > 0 ? (
        <details className="search-results-jobs">
          <summary>Queued source jobs</summary>
          <JobRows rows={jobs} />
        </details>
      ) : null}
      {contextMenu ? (
        <FilesContextMenu
          x={contextMenu.x}
          y={contextMenu.y}
          label={`${contextMenu.title} actions`}
          onClose={() => setContextMenu(null)}
          items={[
            {
              label: "Copy URL",
              icon: Copy,
              onSelect: () => void navigator.clipboard.writeText(contextMenu.url),
            },
            {
              label: "Open in Browser",
              icon: ExternalLink,
              onSelect: () => window.open(contextMenu.url, "_blank", "noopener,noreferrer"),
            },
          ]}
        />
      ) : null}
    </WorkspaceSurface>
  );
}

export function RankedResultView({
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
      <ResultRows rows={rows} preferSnippet title={title} />
      <span className="sr-only">
        {rows.length} matches in {strField(payload, "collection") ?? "axon"} · {title}
      </span>
    </div>
  );
}
