import { useEffect, useRef } from "react";
import { AlertTriangle, Check, Minus, Workflow, X } from "lucide-react";

import type { CrawlLogEvent, CrawlSnapshot } from "@/lib/crawlJob";
import { isLive } from "@/lib/crawlJob";

interface CrawlJobViewProps {
  snapshot: CrawlSnapshot;
  nowMs: number;
  canceling: boolean;
  onCancel: () => void;
  onViewPartial: () => void;
  onMinimize: () => void;
  onClose: () => void;
}

export function CrawlJobView({
  snapshot,
  nowMs,
  canceling,
  onCancel,
  onViewPartial,
  onMinimize,
  onClose,
}: CrawlJobViewProps) {
  const live = isLive(snapshot, nowMs);
  const status = statusPill(snapshot);
  const third = thirdCell(snapshot);
  const depth = depthText(snapshot);
  const pct = Math.round(snapshot.percent);

  return (
    <section className="output-panel output-job-panel">
      <div className="output-state output-job output-tone-warn">
        <header className="output-header">
          <span className="output-op-tile" aria-hidden="true">
            <Workflow size={19} strokeWidth={1.65} />
          </span>
          <div className="output-meta-info">
            <span className="output-title">Crawling {snapshot.host}</span>
            <span className="output-subtitle">job {snapshot.jobId}</span>
          </div>
          <span className={`output-status output-status-${status.tone}`}>{status.label}</span>
          <span className="output-tools">
            <button type="button" onClick={onMinimize} title="Minimize" aria-label="Minimize to tray">
              <Minus size={13} />
            </button>
            <button type="button" onClick={onClose} title="Close" aria-label="Close job view">
              <X size={13} />
            </button>
          </span>
        </header>

        <div className="running-body">
          <div className="running-stat-strip" aria-label="Crawl progress stats">
            <Stat label="Fetched" value={fmt(snapshot.fetched)} accent />
            <Stat label="Queued" value={fmt(snapshot.queued)} />
            <Stat label={titleCase(third.label)} value={third.value} />
            <Stat label="Depth" value={depth} />
          </div>

          <div>
            <div className="running-progress-head">
              <span>
                {pct}%
                {snapshot.etaText ? ` · ${snapshot.etaText}` : ""}
              </span>
              {live ? (
                <span>
                  <i />
                  TAILING
                </span>
              ) : snapshot.phase === "done" ? (
                <span className="running-progress-done">DONE</span>
              ) : (
                <span className="running-progress-stalled">STALLED</span>
              )}
            </div>
            <div className="running-progress">
              <span style={{ width: `${Math.max(2, pct)}%` }} />
            </div>
          </div>

          {snapshot.rateLimited.length > 0 && (
            <div className="running-warn">
              <AlertTriangle size={15} strokeWidth={1.9} />
              <span>{rateLimitLabel(snapshot)}</span>
              <span>· auto-resume</span>
            </div>
          )}

          <CrawlLog events={snapshot.events} phase={snapshot.phase} live={live} />

          <div className="running-actions">
            <button type="button" onClick={onViewPartial}>
              <Check size={14} strokeWidth={2.2} />
              View {snapshot.phase === "done" ? "result" : "partial result"}
            </button>
            {snapshot.phase === "crawling" || snapshot.phase === "pending" || snapshot.phase === "embedding" ? (
              <button type="button" onClick={onCancel} disabled={canceling}>
                {canceling ? "Canceling…" : "Cancel job"}
              </button>
            ) : null}
          </div>
        </div>
      </div>
    </section>
  );
}

function Stat({ label, value, accent }: { label: string; value: string; accent?: boolean }) {
  return (
    <span className={accent ? "running-stat running-stat-accent" : "running-stat"}>
      <span>{label}</span>
      <strong>{value}</strong>
    </span>
  );
}

function titleCase(value: string): string {
  return value.toLowerCase().replace(/\b\w/g, (char) => char.toUpperCase());
}

function CrawlLog({
  events,
  phase,
  live,
}: {
  events: CrawlLogEvent[];
  phase: CrawlSnapshot["phase"];
  live: boolean;
}) {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const el = ref.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [events.length]);

  return (
    <div className="running-log" ref={ref}>
      {events.length === 0 ? (
        <div className="running-log-line running-log-empty">
          {phase === "pending" ? "queued — waiting for a worker…" : "starting crawl…"}
        </div>
      ) : (
        events.map((event, index) => <LogLine key={`${event.t}-${index}`} event={event} />)
      )}
      {live && events.length > 0 ? <span className="running-log-caret" aria-hidden="true" /> : null}
    </div>
  );
}

function LogLine({ event }: { event: CrawlLogEvent }) {
  const time = <span className="rl-time">{formatT(event.t)}</span>;
  if (event.kind === "embed") {
    return (
      <div className="running-log-line">
        {time} <span className="rl-embed">embed batch {event.batch ?? "?"}</span>
        <span className="rl-dim"> · {event.chunks ?? 0} chunks → </span>
        <span className="rl-coll">axon</span>
      </div>
    );
  }
  const warn = event.kind === "warn" || (event.status != null && event.status >= 400);
  return (
    <div className="running-log-line">
      {time} <span className="rl-op">fetch</span> <span className="rl-url">{event.url ?? ""}</span>
      <span className="rl-arrow"> → </span>
      <span className={warn ? "rl-status rl-status-warn" : "rl-status rl-status-ok"}>{event.status ?? "…"}</span>
      {event.kind === "warn" && event.text ? (
        <span className="rl-dim"> · {event.text}</span>
      ) : event.links != null ? (
        <span className="rl-dim"> · {event.links} links</span>
      ) : null}
    </div>
  );
}

function fmt(value: number): string {
  return value.toLocaleString("en-US");
}

function formatT(t: number): string {
  if (t < 10_000) return `${t}ms`;
  return `${(t / 1000).toFixed(1)}s`;
}

function depthText(snap: CrawlSnapshot): string {
  const max = snap.depthMax;
  if (snap.depthCurrent != null && max != null) return `${snap.depthCurrent} / ${max}`;
  if (max != null) return `· / ${max}`;
  if (snap.depthCurrent != null) return `${snap.depthCurrent}`;
  return "—";
}

function thirdCell(snap: CrawlSnapshot): { label: string; value: string } {
  // Honest two-phase: markdown files saved during the crawl, embedded docs afterwards.
  if (snap.phase === "embedding" || snap.phase === "done") {
    return { label: "EMBEDDED", value: fmt(snap.embedded || snap.docs) };
  }
  return { label: "SAVED", value: fmt(snap.docs) };
}

function statusPill(snap: CrawlSnapshot): { label: string; tone: "ok" | "warn" | "error" | "neutral" | "info" } {
  switch (snap.phase) {
    case "pending":
    case "crawling":
      return { label: "202 Accepted", tone: "info" };
    case "embedding":
      return { label: "embedding", tone: "info" };
    case "done":
      return { label: "complete", tone: "ok" };
    case "failed":
      return { label: "failed", tone: "error" };
    case "canceled":
      return { label: "canceled", tone: "neutral" };
  }
}

function rateLimitLabel(snap: CrawlSnapshot): string {
  const n = snap.rateLimited.length;
  const backoff = Math.max(...snap.rateLimited.map((r) => r.backoffMs), 0);
  const secs = backoff > 0 ? Math.round(backoff / 1000) : 0;
  const hostWord = n === 1 ? "host" : "hosts";
  return secs > 0
    ? `${n} ${hostWord} rate-limited · backing off ${secs}s`
    : `${n} ${hostWord} rate-limited`;
}
