import { EmptyResult } from "@/components/palette/OperationResultViewShared";
import { isRecord, numField, strField } from "@/lib/payload";

const LIST_LIMIT = 18;

export function ResultRows({ rows, preferSnippet }: { rows: unknown[]; preferSnippet?: boolean }) {
  if (rows.length === 0) return <EmptyResult kind="results" />;
  return (
    <section className="operation-section">
      <h3 className="stats-heading">Results</h3>
      <div className="operation-list">
        {rows.slice(0, LIST_LIMIT).map((row, index) => {
          const record = isRecord(row) ? row : {};
          const title =
            strField(record, "title") ??
            strField(record, "name") ??
            strField(record, "url") ??
            `Result ${index + 1}`;
          const url = strField(record, "url") ?? strField(record, "source_url");
          const snippet =
            strField(record, "snippet") ??
            strField(record, "content") ??
            strField(record, "text") ??
            strField(record, "reason");
          const score = numField(record, "score");
          const rank = numField(record, "rank") ?? index + 1;
          return (
            <article key={`${url ?? title}-${rank}`} className="operation-row">
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
                {snippet ? (
                  <p className={preferSnippet ? "operation-snippet" : "operation-muted"}>
                    {snippet}
                  </p>
                ) : null}
              </div>
              {score !== undefined ? (
                <span className="operation-score">{score.toFixed(3)}</span>
              ) : null}
            </article>
          );
        })}
      </div>
    </section>
  );
}
