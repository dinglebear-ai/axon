import { Streamdown } from "streamdown";

import { arrField, strField, unwrapPayload } from "@/lib/payload";
import { STREAMDOWN_CODE_THEMES, STREAMDOWN_PLUGINS } from "@/lib/streamdownConfig";

// Side-by-side comparison of the same question answered without retrieval
// (baseline) vs. with RAG context injected, plus the judge's analysis.
export function EvaluateView({ payload }: { payload: unknown }) {
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
            <Streamdown plugins={STREAMDOWN_PLUGINS} shikiTheme={STREAMDOWN_CODE_THEMES}>
              {baseline}
            </Streamdown>
          </div>
        </div>
        <div className="evaluate-column">
          <div className="evaluate-column-head evaluate-head-rag">With RAG · {sources.length} source{sources.length === 1 ? "" : "s"}</div>
          <div className="ask-answer ask-answer-reader evaluate-answer">
            <Streamdown plugins={STREAMDOWN_PLUGINS} shikiTheme={STREAMDOWN_CODE_THEMES}>
              {rag}
            </Streamdown>
          </div>
        </div>
      </div>

      {analysis && (
        <section className="evaluate-verdict">
          <h3 className="stats-heading">Judge analysis</h3>
          <div className="ask-answer">
            <Streamdown plugins={STREAMDOWN_PLUGINS} shikiTheme={STREAMDOWN_CODE_THEMES}>
              {analysis}
            </Streamdown>
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
}
