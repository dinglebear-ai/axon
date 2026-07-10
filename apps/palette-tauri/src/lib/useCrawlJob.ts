import { useEffect, useState, type Dispatch, type SetStateAction } from "react";

import { summarizeCrawl } from "@/lib/crawlJob";
import { formatPayload } from "@/lib/format";
import { invoke } from "@/lib/invoke";
import type { RunState } from "@/lib/runState";

// Bead axon_rust-ruzox.9: `GET /v1/crawl/{id}` and `GET /v1/embed/{id}` were
// removed in favor of the unified `GET /v1/jobs/{id}` route, which returns a
// flat `JobSummary` (`{ status, phase, counts?, last_error?, ... }`), not the
// crawl/embed job's old `result_json` shape (`pages_crawled`, `md_created`,
// `events`, `rate_limited`, `embed_job_id`, ...). None of that per-crawl
// telemetry — nor the crawl→embed handoff via `embed_job_id` — has a unified
// equivalent yet, so this adapter degrades gracefully: it maps what IS
// available (status/counts/last_error) into the legacy shape `summarizeCrawl`
// expects and leaves the rest at `summarizeCrawl`'s zero/empty defaults. The
// two-phase crawl→embed progress fold-in is disabled until the unified job
// contract exposes a parent/child job link on the wire.
const LIFECYCLE_TO_LEGACY_STATUS: Record<string, string> = {
  queued: "pending",
  pending: "pending",
  waiting: "pending",
  blocked: "pending",
  running: "running",
  canceling: "running",
  completed: "completed",
  completed_degraded: "completed",
  failed: "failed",
  expired: "failed",
  canceled: "canceled",
  skipped: "canceled",
};

interface JsonRecord {
  [key: string]: unknown;
}

function asRecord(value: unknown): JsonRecord | null {
  return value && typeof value === "object" && !Array.isArray(value) ? (value as JsonRecord) : null;
}

/** Adapt a unified `GET /v1/jobs/{id}` `JobSummary` payload into the legacy
 * `{ job: { status, result_json, updated_at, started_at, error_text } }`
 * shape `summarizeCrawl` expects. See the module-level TODO above. */
function adaptUnifiedCrawlPayload(payload: unknown): unknown {
  const root = asRecord(payload) ?? {};
  const counts = asRecord(root.counts);
  const lastError = asRecord(root.last_error);
  const status = typeof root.status === "string" ? root.status : undefined;
  return {
    job: {
      status: status ? (LIFECYCLE_TO_LEGACY_STATUS[status] ?? "running") : "running",
      result_json: {
        pages_crawled: counts?.items_done ?? 0,
        md_created: counts?.documents_done ?? 0,
        error_pages: 0,
      },
      updated_at: root.updated_at ?? null,
      // No wire `started_at` on JobSummary (server-side `#[serde(skip)]`) —
      // `created_at` is the closest available proxy.
      started_at: root.created_at ?? null,
      error_text: typeof lastError?.message === "string" ? lastError.message : null,
    },
  };
}

interface UseCrawlJobArgs {
  run: RunState;
  setRun: Dispatch<SetStateAction<RunState>>;
  // A-M2 — three view intents replace the six raw setters (settings/history/
  // browse/query/mode) this hook used to drill into App. Each callback carries
  // its view transition; the rule that minimizing/closing a job clears
  // settings/history/browse/mode lives in App's reducer, not here. `setRun`
  // stays because the live poll snapshot IS run state, owned alongside it.
  onMinimizeJob: () => void;
  onExpandJob: () => void;
  onCloseJob: () => void;
}

// Owns the live crawl-job lifecycle: a ~1Hz poll of the real backend while the
// job is non-terminal, plus the tray/cancel/view-partial controls. State that is
// purely job-scoped (nowMs heartbeat, canceling flag) lives here; the shared
// `run` state is owned by App (threaded in as `setRun`) and the view transitions
// are dispatched through the intent callbacks.
export function useCrawlJob({
  run,
  setRun,
  onMinimizeJob,
  onExpandJob,
  onCloseJob,
}: UseCrawlJobArgs) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [canceling, setCanceling] = useState(false);

  // Live job polling: while a crawl job is non-terminal, poll its real status
  // (and the handed-off embed job) ~once/sec and refresh the snapshot. Keyed on
  // jobId + terminal so it stops cleanly when the job (incl. embed phase) ends.
  const jobId = run.kind === "job" ? run.jobId : "";
  const jobPhase = run.kind === "job" ? run.snapshot.phase : "idle";
  const jobTerminal = jobPhase === "done" || jobPhase === "failed" || jobPhase === "canceled";
  useEffect(() => {
    if (run.kind !== "job" || !jobId || jobTerminal) return;
    let active = true;
    // A single transient poll failure is fine on a 1Hz loop, but if the server
    // goes away mid-crawl every tick rejects forever and the spinner freezes with
    // no signal. After STALL_THRESHOLD consecutive failures, surface a visible
    // "lost contact" failed state (which also stops the poll — failed is terminal).
    let consecutiveFailures = 0;
    const STALL_THRESHOLD = 10;
    const getJson = (path: string) =>
      invoke<{ ok: boolean; status: number; payload: unknown }>("axon_http_request", {
        request: { method: "GET", path, body: null },
      });
    const tick = async () => {
      try {
        // Unified route (bead axon_rust-ruzox.9) — see `adaptUnifiedCrawlPayload`
        // above for the shape gap this accepts. The crawl→embed handoff has no
        // unified equivalent yet, so `embedPayload` is always undefined here.
        const crawlRes = await getJson(`/v1/jobs/${jobId}`);
        if (!active) return;
        const crawlPayload = adaptUnifiedCrawlPayload(crawlRes.payload);
        if (!active) return;
        setNowMs(Date.now());
        setRun((current) => {
          if (current.kind !== "job" || current.jobId !== jobId) return current;
          const elapsedSec = Math.max(0, (Date.now() - current.startedAtMs) / 1000);
          const snapshot = summarizeCrawl(crawlPayload, {
            jobId,
            url: current.url,
            elapsedSec,
            maxPages: current.maxPages,
            maxDepth: current.maxDepth,
          });
          return { ...current, snapshot, subtitle: `job ${jobId}` };
        });
        consecutiveFailures = 0;
      } catch {
        if (!active) return;
        if (++consecutiveFailures >= STALL_THRESHOLD) {
          setRun((current) =>
            current.kind === "job" && current.jobId === jobId
              ? {
                  ...current,
                  subtitle: "lost contact with server",
                  snapshot: { ...current.snapshot, phase: "failed" },
                }
              : current,
          );
        }
        /* otherwise transient — keep trying on the next tick */
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 1000);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [run.kind, jobId, jobTerminal, setRun]);

  function minimizeJob() {
    // View transition (clears settings/history/browse/mode) + query reset live in
    // App's onMinimizeJob callback; here we only flip the run snapshot to the tray.
    onMinimizeJob();
    setRun((current) => (current.kind === "job" ? { ...current, minimized: true } : current));
  }

  function expandJob() {
    onExpandJob();
    setRun((current) => (current.kind === "job" ? { ...current, minimized: false } : current));
  }

  function closeJob() {
    onCloseJob();
    setRun({ kind: "idle" });
  }

  async function cancelJob() {
    if (run.kind !== "job" || !run.jobId) return;
    const id = run.jobId;
    setCanceling(true);
    try {
      // Unified `POST /v1/jobs/{id}/cancel` takes a `JobCancelRequest` body
      // (all fields optional) — send `{}` rather than `null`, which the JSON
      // extractor would reject as not-an-object.
      await invoke("axon_http_request", {
        request: { method: "POST", path: `/v1/jobs/${id}/cancel`, body: {} },
      });
    } catch {
      /* the poll will surface the canceled row state */
    } finally {
      setCanceling(false);
    }
  }

  async function viewPartialJob() {
    if (run.kind !== "job" || !run.jobId) return;
    const id = run.jobId;
    const host = run.snapshot.host;
    try {
      const res = await invoke<{ ok: boolean; status: number; payload: unknown }>("axon_http_request", {
        request: { method: "GET", path: `/v1/jobs/${id}`, body: null },
      });
      setRun({
        kind: "success",
        title: `Crawl ${host}`,
        subtitle: `job ${id}`,
        text: formatPayload("crawl", res.payload),
        outputKind: "code",
        result: { ok: res.ok, status: res.status, path: `/v1/jobs/${id}`, method: "GET", payload: res.payload },
      });
    } catch {
      /* keep the live view if the fetch fails */
    }
  }

  return { nowMs, canceling, cancelJob, viewPartialJob, minimizeJob, expandJob, closeJob };
}
