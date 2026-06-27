import { useEffect, useState, type Dispatch, type SetStateAction } from "react";

import { invoke } from "@/lib/invoke";
import { isJobPhaseTerminal, summarizeJob } from "@/lib/jobProgress";
import type { RunState } from "@/lib/runState";

interface UseJobPollArgs {
  run: RunState;
  setRun: Dispatch<SetStateAction<RunState>>;
  // Mirror of useCrawlJob's intent callbacks — the view transitions (which
  // overlays/mode they clear) live in App's reducer, not here.
  onMinimizeJob: () => void;
  onExpandJob: () => void;
  onCloseJob: () => void;
}

// Live lifecycle for the generic async-job families (embed/extract/ingest).
// Sibling of `useCrawlJob`: a ~1Hz poll of `GET /v1/{family}/{id}` while the
// job is non-terminal, plus the tray/cancel controls. Crawl keeps its own,
// richer hook; this one drives the simpler `JobSnapshot` model.
export function useJobPoll({ run, setRun, onMinimizeJob, onExpandJob, onCloseJob }: UseJobPollArgs) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [canceling, setCanceling] = useState(false);

  const isAsync = run.kind === "asyncJob";
  const jobId = isAsync ? run.jobId : "";
  const family = isAsync ? run.family : "embed";
  const phase = isAsync ? run.snapshot.phase : "done";
  const terminal = isJobPhaseTerminal(phase);

  useEffect(() => {
    if (run.kind !== "asyncJob" || !jobId || terminal) return;
    let active = true;
    // A single transient poll failure is fine on a 1Hz loop, but if the server
    // goes away mid-job the spinner would freeze silently. After STALL_THRESHOLD
    // consecutive failures, surface a visible failed state (also stops the poll).
    let consecutiveFailures = 0;
    const STALL_THRESHOLD = 10;
    // A transient failure (thrown error OR a non-ok HTTP status) is fine on a
    // 1Hz loop, but if the server stays unreachable the spinner would freeze
    // silently. After STALL_THRESHOLD consecutive failures, surface a *visible*
    // failed state — phase AND errorText, so JobProgressView renders its error
    // banner instead of leaving the user staring at a dead spinner.
    let inFlight = false;
    const recordFailure = () => {
      if (++consecutiveFailures < STALL_THRESHOLD) return; // transient — retry next tick
      setRun((current) =>
        current.kind === "asyncJob" && current.jobId === jobId
          ? {
              ...current,
              subtitle: "lost contact with server",
              snapshot: {
                ...current.snapshot,
                phase: "failed",
                errorText: `Lost contact with the server after ${STALL_THRESHOLD} failed status checks. The job may still be running — reopen it once the server is reachable.`,
              },
            }
          : current,
      );
    };
    const tick = async () => {
      if (inFlight) return;
      inFlight = true;
      try {
        const res = await invoke<{ ok: boolean; status: number; payload: unknown }>(
          "axon_http_request",
          { request: { method: "GET", path: `/v1/${family}/${jobId}`, body: null } },
        );
        if (!active) return;
        setNowMs(Date.now());
        // A non-ok status (5xx/404) does not throw — count it toward the stall
        // so a server returning errors trips the same visible failure path.
        if (!res.ok) {
          recordFailure();
          return;
        }
        setRun((current) => {
          if (current.kind !== "asyncJob" || current.jobId !== jobId) return current;
          const snapshot = summarizeJob(current.family, res.payload, {
            jobId,
            label: current.snapshot.label,
          });
          return { ...current, snapshot, subtitle: `job ${jobId}` };
        });
        consecutiveFailures = 0;
      } catch {
        if (!active) return;
        recordFailure();
      } finally {
        inFlight = false;
      }
    };
    void tick();
    const id = window.setInterval(() => void tick(), 1000);
    return () => {
      active = false;
      window.clearInterval(id);
    };
  }, [run.kind, jobId, family, terminal, setRun]);

  function minimizeJob() {
    onMinimizeJob();
    setRun((current) => (current.kind === "asyncJob" ? { ...current, minimized: true } : current));
  }

  function expandJob() {
    onExpandJob();
    setRun((current) => (current.kind === "asyncJob" ? { ...current, minimized: false } : current));
  }

  function closeJob() {
    onCloseJob();
    setRun({ kind: "idle" });
  }

  async function cancelJob() {
    if (run.kind !== "asyncJob" || !run.jobId) return;
    const id = run.jobId;
    const fam = run.family;
    setCanceling(true);
    try {
      const res = await invoke<{ ok: boolean; status: number; payload: unknown }>("axon_http_request", {
        request: { method: "POST", path: `/v1/${fam}/${id}/cancel`, body: null },
      });
      if (!res.ok) {
        setRun((current) =>
          current.kind === "asyncJob" && current.jobId === id
            ? {
                ...current,
                subtitle: `cancel failed (${res.status})`,
                snapshot: {
                  ...current.snapshot,
                  errorText: `Cancel request failed with HTTP ${res.status}.`,
                },
              }
            : current,
        );
      }
    } catch {
      /* the poll will surface the canceled row state */
    } finally {
      setCanceling(false);
    }
  }

  return { nowMs, canceling, cancelJob, minimizeJob, expandJob, closeJob };
}
