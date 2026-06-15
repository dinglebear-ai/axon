import { memo } from "react";

import { arrField, boolField, isRecord, numField, shortId, strField, unwrapPayload } from "@/lib/payload";

function jobUrl(job: Record<string, unknown>): string | undefined {
  const direct = strField(job, "url") ?? strField(job, "target");
  if (direct) return direct;
  const config = isRecord(job.config_json) ? job.config_json : {};
  const urls = arrField(config, "urls");
  return typeof urls[0] === "string" ? (urls[0] as string) : undefined;
}

function JobRow({ job }: { job: Record<string, unknown> }) {
  const status = strField(job, "status") ?? "unknown";
  const url = jobUrl(job);
  const attempts = numField(job, "attempt_count");
  const id = strField(job, "id") ?? strField(job, "job_id");
  return (
    <div className="status-job">
      <span className={`status-pill status-pill-${status}`}>{status}</span>
      <span className="status-job-url" title={url}>{url ?? (id ? shortId(id) : "—")}</span>
      <span className="status-job-meta">
        {id ? <code>{shortId(id)}</code> : null}
        {attempts !== undefined ? <span>attempt {attempts}</span> : null}
      </span>
    </div>
  );
}

export const StatusView = memo(function StatusView({ payload }: { payload: unknown }) {
  const data = unwrapPayload(payload);
  const degraded = boolField(data, "degraded") ?? false;
  const errors = arrField(data, "errors").filter((e): e is string => typeof e === "string");

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
                return <JobRow key={key} job={record} />;
              })}
            </div>
          </section>
        ))
      )}
    </div>
  );
});
