import { Check, ChevronDown, Copy, ExternalLink } from "lucide-react";
import { useState } from "react";

import { EmptyResult } from "@/components/palette/OperationResultViewShared";
import { Button } from "@/components/ui/aurora/button";
import { isRecord, numField, strField } from "@/lib/payload";
import { hostLabel } from "@/lib/url";

const LIST_LIMIT = 18;

export function ResultRows({
  rows,
  preferSnippet,
  title = "Results",
}: {
  rows: unknown[];
  preferSnippet?: boolean;
  title?: string;
}) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  if (rows.length === 0) return <EmptyResult kind="results" />;

  return (
    <section className="operation-section operation-results-section">
      <header className="operation-section-header">
        <div>
          <span className="operation-section-eyebrow">{title}</span>
          <strong>{rows.length.toLocaleString()} items</strong>
        </div>
        {rows.length > LIST_LIMIT ? <span>Showing first {LIST_LIMIT}</span> : null}
      </header>
      <div className="operation-result-grid">
        {rows.slice(0, LIST_LIMIT).map((row, index) => {
          const record = isRecord(row) ? row : {};
          const titleText =
            strField(record, "title") ??
            strField(record, "name") ??
            strField(record, "url") ??
            strField(record, "path") ??
            `Result ${index + 1}`;
          const url = strField(record, "url") ?? strField(record, "source_url");
          const snippet =
            strField(record, "snippet") ??
            strField(record, "content") ??
            strField(record, "text") ??
            strField(record, "reason") ??
            strField(record, "description");
          const score = numField(record, "score");
          const rank = numField(record, "rank") ?? index + 1;
          const key = `${url ?? titleText}-${rank}`;
          const isExpanded = expanded === key;
          const source = url ? hostLabel(url) : (strField(record, "type") ?? "Axon result");

          return (
            <article
              key={key}
              className={`operation-result-card${isExpanded ? " is-expanded" : ""}`}
            >
              <div className="operation-result-card-topline">
                <span className="operation-result-ordinal">{String(rank).padStart(2, "0")}</span>
                <span className="operation-result-source" title={url ?? source}>
                  {source}
                </span>
                {score !== undefined ? (
                  <span className="operation-result-score">{Math.round(score * 100)}%</span>
                ) : null}
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
              {snippet ? (
                <p className={preferSnippet && !isExpanded ? "is-clamped" : undefined}>{snippet}</p>
              ) : null}
              <footer className="operation-result-card-footer">
                {snippet && preferSnippet ? (
                  <Button
                    variant="plain"
                    size="unstyled"
                    type="button"
                    className="operation-result-expand"
                    onClick={() => setExpanded(isExpanded ? null : key)}
                  >
                    {isExpanded ? "Show less" : "Read more"}
                    <ChevronDown size={13} aria-hidden="true" />
                  </Button>
                ) : (
                  <span />
                )}
                {url ? (
                  <span className="operation-result-actions">
                    <Button
                      variant="plain"
                      size="unstyled"
                      type="button"
                      aria-label={`Copy ${titleText} URL`}
                      title="Copy URL"
                      onClick={() => {
                        void navigator.clipboard.writeText(url).then(() => {
                          setCopied(key);
                          window.setTimeout(() => setCopied(null), 1400);
                        });
                      }}
                    >
                      {copied === key ? <Check size={13} /> : <Copy size={13} />}
                    </Button>
                    <a
                      href={url}
                      target="_blank"
                      rel="noopener noreferrer"
                      aria-label={`Open ${titleText}`}
                    >
                      <ExternalLink size={13} aria-hidden="true" />
                    </a>
                  </span>
                ) : null}
              </footer>
            </article>
          );
        })}
      </div>
    </section>
  );
}
