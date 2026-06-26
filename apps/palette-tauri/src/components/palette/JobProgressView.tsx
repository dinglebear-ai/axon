import { memo } from "react";
import { AlertTriangle, Minus, Workflow, X } from "lucide-react";

import { Button } from "@/components/ui/aurora/button";
import { MIN_PROGRESS_PCT } from "@/lib/format";
import { jobFamilyVerb, type JobSnapshot } from "@/lib/jobProgress";

interface JobProgressViewProps {
  snapshot: JobSnapshot;
  nowMs: number;
  canceling: boolean;
  onCancel: () => void;
  onMinimize: () => void;
  onClose: () => void;
}

// Live status card for the generic async-job families (embed/extract/ingest).
// Visual sibling of `CrawlJobView` — same Aurora output-job-panel chrome — but
// driven by the simpler `JobSnapshot` model (no page frontier / depth / log).
export const JobProgressView = memo(function JobProgressView({
  snapshot,
  nowMs,
  canceling,
  onCancel,
  onMinimize,
  onClose,
}: JobProgressViewProps) {
  const status = statusPill(snapshot);
  const live = isLive(snapshot, nowMs);
  const active = snapshot.phase === "running" || snapshot.phase === "pending";
  // Determinate when the backend exposed a percent; otherwise a low-width
  // "working" bar while running, full when done.
  const pct =
    snapshot.percent != null
      ? Math.round(snapshot.percent)
      : snapshot.phase === "done"
        ? 100
        : 0;

  return (
    <section className="output-panel output-job-panel">
      <div className="output-state output-job output-tone-warn">
        <header className="output-header">
          <span className="output-op-tile" aria-hidden="true">
            <Workflow size={19} strokeWidth={1.65} />
          </span>
          <div className="output-meta-info">
            <span className="output-title">
              {jobFamilyVerb(snapshot.family)} {snapshot.label}
            </span>
            <span className="output-subtitle">job {snapshot.jobId || "submitting…"}</span>
          </div>
          <span className={`output-status output-status-${status.tone}`}>{status.label}</span>
          <span className="output-tools">
            <Button variant="plain" size="unstyled" type="button" onClick={onMinimize} title="Minimize" aria-label="Minimize to tray">
              <Minus size={13} />
            </Button>
            <Button variant="plain" size="unstyled" type="button" onClick={onClose} title="Close" aria-label="Close job view">
              <X size={13} />
            </Button>
          </span>
        </header>

        <div className="running-body">
          {snapshot.metrics.length > 0 && (
            <div className="running-stat-strip" role="group" aria-label="Job progress stats">
              {snapshot.metrics.map((metric, index) => (
                <Stat key={metric.label} label={metric.label} value={metric.value} accent={index === 0} />
              ))}
            </div>
          )}

          <div>
            <div className="running-progress-head">
              <span>{snapshot.percent != null ? `${pct}%` : snapshot.status}</span>
              {live ? (
                <span>
                  <i />
                  WORKING
                </span>
              ) : snapshot.phase === "done" ? (
                <span className="running-progress-done">DONE</span>
              ) : active ? (
                <span className="running-progress-stalled">STALLED</span>
              ) : (
                <span className="running-progress-stalled">{snapshot.phase.toUpperCase()}</span>
              )}
            </div>
            <div className="running-progress">
              <span style={{ width: `${Math.max(MIN_PROGRESS_PCT, pct)}%` }} />
            </div>
          </div>

          {snapshot.errorText && (snapshot.phase === "failed" || snapshot.phase === "canceled") && (
            <div className="running-warn">
              <AlertTriangle size={15} strokeWidth={1.9} />
              <span>{snapshot.errorText}</span>
            </div>
          )}

          <div className="running-actions">
            {active ? (
              <Button variant="plain" size="unstyled" type="button" className="disabled:opacity-100" onClick={onCancel} disabled={canceling}>
                {canceling ? "Canceling…" : "Cancel job"}
              </Button>
            ) : null}
          </div>
        </div>
      </div>
    </section>
  );
});

function Stat({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <span className={accent ? "running-stat running-stat-accent" : "running-stat"}>
      <span>{label}</span>
      <strong>{value}</strong>
    </span>
  );
}

/** Heartbeat fires every 30s; allow a margin before calling a job stalled. */
function isLive(snap: JobSnapshot, nowMs: number): boolean {
  if (snap.phase !== "running" && snap.phase !== "pending") return false;
  if (snap.updatedAtMs == null) return true; // no timestamp yet — assume live
  return nowMs - snap.updatedAtMs < 45_000;
}

function statusPill(snap: JobSnapshot): {
  label: string;
  tone: "ok" | "warn" | "error" | "neutral" | "info";
} {
  switch (snap.phase) {
    case "pending":
    case "running":
      return { label: "202 Accepted", tone: "info" };
    case "done":
      return { label: "complete", tone: "ok" };
    case "failed":
      return { label: "failed", tone: "error" };
    case "canceled":
      return { label: "canceled", tone: "neutral" };
  }
}
