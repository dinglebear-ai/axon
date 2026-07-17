import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { ResultRows } from "@/components/palette/OperationResultRows";
import { JobRows, ResultSummary, arrayByKeys } from "@/components/palette/OperationResultViewShared";
import { arrField, strField } from "@/lib/payload";

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
  return (
    <div className="output-body operation-view aurora-scrollbar">
      <ResultSummary metrics={[["Results", rows.length], ["Queued source jobs", jobs.length], ["View", title]]} />
      {includeSummary && summary ? (
        <section className="operation-section">
          <h3 className="stats-heading">Summary</h3>
          <div className="operation-markdown">
            <MarkdownBody>{summary}</MarkdownBody>
          </div>
        </section>
      ) : null}
      <ResultRows rows={rows} />
      {jobs.length > 0 ? <JobRows title="Queued source jobs" rows={jobs} /> : null}
    </div>
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
      <ResultSummary
        metrics={[["Matches", rows.length], ["Collection", strField(payload, "collection") ?? "axon"], ["View", title]]}
      />
      <ResultRows rows={rows} preferSnippet />
    </div>
  );
}
