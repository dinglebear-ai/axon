import { memo } from "react";

import { Button } from "@/components/ui/aurora/button";
import { arrField, boolField, isRecord, numField, shortId, strField, unwrapPayload } from "@/lib/payload";

export interface OpenJobHandler {
  /** Open the live job card for a job shown in the status queue. */
  (family: string, jobId: string, label: string): void;
}

const TOTAL_FAMILIES = ["crawl", "extract", "embed", "ingest"] as const;

function jobUrl(job: Record<string, unknown>): string | undefined {
  const direct = strField(job, "url") ?? strField(job, "target");
  if (direct) return direct;
  const config = isRecord(job.config_json) ? job.config_json : {};
  const urls = arrField(config, "urls");
  return typeof urls[0] === "string" ? (urls[0] as string) : undefined;
}

function JobRow({
  job,
  family,
  onOpenJob,
}: {
  job: Record<string, unknown>;
  family: string;
  onOpenJob?: OpenJobHandler;
}) {
  const status = strField(job, "status") ?? "unknown";
  const url = jobUrl(job);
  const attempts = numField(job, "attempt_count");
  const id = strField(job, "id") ?? strField(job, "job_id");
  // Only live (pending/running) jobs have a live card worth tailing.
  const openable = Boolean(onOpenJob && id && (status === "running" || status === "pending"));

  const body = (
    <>
      <span className={`status-pill status-pill-${status}`}>{status}</span>
      <span className="status-job-url" title={url}>{url ?? (id ? shortId(id) : "—")}</span>
      <span className="status-job-meta">
        {id ? <code>{shortId(id)}</code> : null}
        {attempts !== undefined ? <span>attempt {attempts}</span> : null}
      </span>
    </>
  );

  return openable && onOpenJob && id ? (
    <Button
      variant="plain"
      size="unstyled"
      type="button"
      className="status-job status-job-clickable"
      onClick={() => onOpenJob(family, id, url ?? id)}
      title={`Open live ${family} job`}
    >
      {body}
    </Button>
  ) : (
    <div className="status-job">{body}</div>
  );
}

export const StatusView = memo(function StatusView({
  payload,
  onOpenJob,
}: {
  payload: unknown;
  onOpenJob?: OpenJobHandler;
}) {
  const data = unwrapPayload(payload);
  const degraded = boolField(data, "degraded") ?? false;
  const errors = arrField(data, "errors").filter((e): e is string => typeof e === "string");
  const totals = isRecord(data.totals) ? data.totals : null;

  const families = Object.entries(data)
    .filter(([k, v]) => k.endsWith("_jobs") && Array.isArray(v))
    .map(([k, v]) => [k.replace(/^local_/, "").replace(/_jobs$/, ""), v as unknown[]] as const)
    .filter(([, jobs]) => jobs.length > 0);

  const totalJobs = families.reduce((sum, [, jobs]) => sum + jobs.length, 0);

  return (
    <div className="output-body status-view aurora-scrollbar">
      <div className="status-summary">
        <span className={degraded ? "status-health status-health-bad" : "status-health status-health-ok"}>
          {degraded ? "Degraded" : "Healthy"}
        </span>
        <span className="status-summary-meta">{totalJobs} active job{totalJobs === 1 ? "" : "s"}</span>
      </div>

      {totals && (
        <div className="status-totals" role="group" aria-label="Total jobs by family">
          {TOTAL_FAMILIES.map((family) => (
            <div key={family} className="status-total-cell">
              <span>{family}</span>
              <strong>{(numField(totals, family) ?? 0).toLocaleString()}</strong>
            </div>
          ))}
        </div>
      )}

      {errors.length > 0 && (
        <div className="status-errors">
          {errors.map((err, i) => (
            <div key={i} className="status-error-row">{err}</div>
          ))}
        </div>
      )}

      {families.length === 0 ? (
        <div className="status-empty">No active jobs in the queue.</div>
      ) : (
        families.map(([family, jobs]) => (
          <section key={family} className="status-section">
            <h3 className="stats-heading">{family} · {jobs.length}</h3>
            <div className="status-job-list">
              {jobs.map((job, i) => {
                const record = isRecord(job) ? job : {};
                // M4: key on the stable job id so React reconciles rows by identity,
                // not position — prevents row state/DOM reuse glitches when the poll
                // reorders or drops jobs. Falls back to family+index only when no id.
                const key = strField(record, "job_id") ?? strField(record, "id") ?? `${family}-${i}`;
                return <JobRow key={key} job={record} family={family} onOpenJob={onOpenJob} />;
              })}
            </div>
          </section>
        ))
      )}
    </div>
  );
});
