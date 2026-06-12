import { Streamdown } from "streamdown";

import { ConversationThread } from "@/components/palette/AskConversation";
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
      <div className="evaluate-columns">
        <div className="evaluate-column">
          <div className="evaluate-column-head evaluate-head-baseline">Without RAG · baseline</div>
          <ConversationThread prompt={query} answer={baseline} assistantLabel="LLM" />
        </div>
        <div className="evaluate-column">
          <div className="evaluate-column-head evaluate-head-rag">With RAG · {sources.length} source{sources.length === 1 ? "" : "s"}</div>
          <ConversationThread prompt={query} answer={rag} assistantLabel="Axon" />
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
          <ul>
            {sources.slice(0, 12).map((url) => (
              <li key={url}>
                <a href={url} target="_blank" rel="noopener noreferrer">{url}</a>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
