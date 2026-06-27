import { memo } from "react";
import { AlertTriangle, CheckCircle2, XCircle } from "lucide-react";

import { arrField, boolField, isRecord, numField, strField, unwrapPayload } from "@/lib/payload";

interface ServiceRow {
  name: string;
  ok: boolean;
  url?: string;
  model?: string;
  latencyMs?: number;
  detail?: string;
}

interface Capability {
  tier: string;
  available: boolean;
  impact: string[];
  remedies: string[];
}

function serviceRows(services: Record<string, unknown>): ServiceRow[] {
  return Object.entries(services).map(([name, raw]) => {
    const svc = isRecord(raw) ? raw : {};
    return {
      name,
      ok: boolField(svc, "ok") ?? false,
      url: strField(svc, "configured_url"),
      model: strField(svc, "model"),
      latencyMs: numField(svc, "latency_ms"),
      detail: strField(svc, "error") ?? strField(svc, "detail") ?? strField(svc, "vector_mode"),
    };
  });
}

function capabilities(raw: unknown[]): Capability[] {
  return raw.filter(isRecord).map((c) => ({
    tier: strField(c, "tier") ?? "tier",
    available: boolField(c, "available") ?? false,
    impact: arrField(c, "impact").filter((s): s is string => typeof s === "string"),
    remedies: arrField(c, "remedies").filter((s): s is string => typeof s === "string"),
  }));
}

function prettyTier(tier: string): string {
  return tier.replace(/^tier_\d+_/, "").replace(/_/g, " ");
}

export const DoctorView = memo(function DoctorView({ payload }: { payload: unknown }) {
  const report = unwrapPayload(payload);
  const allOk = boolField(report, "all_ok") ?? false;
  const services = isRecord(report.services) ? serviceRows(report.services) : [];
  const caps = capabilities(arrField(report, "capabilities"));
  const recommendations = arrField(report, "recommendations").filter((s): s is string => typeof s === "string");
  const pipelines = isRecord(report.pipelines) ? report.pipelines : {};
  const pendingJobs = numField(report, "pending_jobs");

  return (
    <div className="output-body doctor-view aurora-scrollbar">
      <div className="doctor-summary">
        <span className={allOk ? "status-health status-health-ok" : "status-health status-health-bad"}>
          {allOk ? "All systems healthy" : "Degraded"}
        </span>
        {pendingJobs !== undefined ? (
          <span className="doctor-summary-meta">{pendingJobs} pending job{pendingJobs === 1 ? "" : "s"}</span>
        ) : null}
      </div>

      <section className="doctor-section">
        <h3 className="stats-heading">Services</h3>
        <div className="doctor-service-list">
          {services.map((svc) => (
            <div key={svc.name} className="doctor-service">
              <span className={svc.ok ? "doctor-dot doctor-dot-ok" : "doctor-dot doctor-dot-bad"} aria-hidden="true" />
              <span className="doctor-service-name">{svc.name}</span>
              <span className="doctor-service-detail" title={svc.url}>
                {svc.model ?? svc.detail ?? svc.url ?? ""}
              </span>
              <span className="doctor-service-latency">
                {svc.ok ? (svc.latencyMs !== undefined ? `${svc.latencyMs}ms` : "ok") : "down"}
              </span>
            </div>
          ))}
        </div>
      </section>

      {caps.length > 0 && (
        <section className="doctor-section">
          <h3 className="stats-heading">Capabilities</h3>
          <div className="doctor-cap-list">
            {caps.map((cap) => (
              <div key={cap.tier} className={cap.available ? "doctor-cap doctor-cap-ok" : "doctor-cap doctor-cap-bad"}>
                <span className="doctor-cap-head">
                  {cap.available ? <CheckCircle2 size={14} /> : <XCircle size={14} />}
                  {prettyTier(cap.tier)}
                </span>
                {!cap.available && cap.impact.length > 0 ? (
                  <span className="doctor-cap-impact">{cap.impact.join(" · ")}</span>
                ) : null}
                {!cap.available && cap.remedies.length > 0 ? (
                  <span className="doctor-cap-remedy">→ {cap.remedies.join(" · ")}</span>
                ) : null}
              </div>
            ))}
          </div>
        </section>
      )}

      {Object.keys(pipelines).length > 0 && (
        <section className="doctor-section">
          <h3 className="stats-heading">Pipelines</h3>
          <div className="doctor-pipelines">
            {Object.entries(pipelines).map(([name, ready]) => (
              <span key={name} className={ready === true ? "doctor-pill doctor-pill-ok" : "doctor-pill doctor-pill-bad"}>
                {name.replace(/_/g, " ")}
              </span>
            ))}
          </div>
        </section>
      )}

      {recommendations.length > 0 && (
        <section className="doctor-section">
          <h3 className="stats-heading">Recommendations</h3>
          <ul className="doctor-recs">
            {recommendations.map((rec) => (
              <li key={rec}>
                <AlertTriangle size={13} strokeWidth={1.9} />
                <span>{rec}</span>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
});
