import { memo } from "react";

import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { arrField, strField, unwrapPayload } from "@/lib/payload";

// Side-by-side comparison of the same question answered without retrieval
// (baseline) vs. with RAG context injected, plus the judge's analysis.
export const EvaluateView = memo(function EvaluateView({ payload }: { payload: unknown }) {
  const data = unwrapPayload(payload);
  const query = strField(data, "query") ?? "";
  const baseline = strField(data, "baseline_answer") ?? "";
  const rag = strField(data, "rag_answer") ?? "";
  const analysis = strField(data, "analysis_answer") ?? "";
  const sources = arrField(data, "source_urls").filter((u): u is string => typeof u === "string");

  return (
    <div className="output-body evaluate-view aurora-scrollbar">
      {query ? (
        <div className="ask-prompt-strip evaluate-prompt">
          <span>Question</span>
          <p>{query}</p>
        </div>
      ) : null}

      <div className="evaluate-columns">
        <div className="evaluate-column">
          <div className="evaluate-column-head evaluate-head-baseline">Without RAG · baseline</div>
          <div className="ask-answer ask-answer-reader evaluate-answer">
            <MarkdownBody>{baseline}</MarkdownBody>
          </div>
        </div>
        <div className="evaluate-column">
          <div className="evaluate-column-head evaluate-head-rag">With RAG · {sources.length} source{sources.length === 1 ? "" : "s"}</div>
          <div className="ask-answer ask-answer-reader evaluate-answer">
            <MarkdownBody>{rag}</MarkdownBody>
          </div>
        </div>
      </div>

      {analysis && (
        <section className="evaluate-verdict">
          <h3 className="stats-heading">Judge analysis</h3>
          <div className="ask-answer">
            <MarkdownBody>{analysis}</MarkdownBody>
          </div>
        </section>
      )}

      {sources.length > 0 && (
        <section className="evaluate-sources">
          <h3 className="stats-heading">Sources</h3>
          <div className="evaluate-source-list">
            {sources.slice(0, 12).map((url) => (
              <a key={url} href={url} target="_blank" rel="noopener noreferrer">{url}</a>
            ))}
          </div>
        </section>
      )}
    </div>
  );
});
