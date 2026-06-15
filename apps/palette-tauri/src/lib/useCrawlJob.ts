import { useEffect, useState, type Dispatch, type SetStateAction } from "react";

import type { PaletteAction } from "@/lib/actions";
import { extractEmbedJobId } from "@/lib/appHelpers";
import { summarizeCrawl } from "@/lib/crawlJob";
import { formatPayload } from "@/lib/format";
import { invoke } from "@/lib/invoke";
import type { RunState } from "@/lib/runState";

interface UseCrawlJobArgs {
  run: RunState;
  setRun: Dispatch<SetStateAction<RunState>>;
  setSettingsOpen: Dispatch<SetStateAction<boolean>>;
  setHistoryOpen: Dispatch<SetStateAction<boolean>>;
  setBrowseOpen: Dispatch<SetStateAction<boolean>>;
  setQuery: Dispatch<SetStateAction<string>>;
  setModeAction: Dispatch<SetStateAction<PaletteAction | null>>;
}

// Owns the live crawl-job lifecycle: a ~1Hz poll of the real backend while the
// job is non-terminal, plus the tray/cancel/view-partial controls. State that is
// purely job-scoped (nowMs heartbeat, canceling flag) lives here; the shared
// `run`/panel state is owned by App and threaded in as setters.
export function useCrawlJob({
  run,
  setRun,
  setSettingsOpen,
  setHistoryOpen,
  setBrowseOpen,
  setQuery,
  setModeAction,
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
    const getJson = (path: string) =>
      invoke<{ ok: boolean; status: number; payload: unknown }>("axon_http_request", {
        request: { method: "GET", path, body: null },
      });
    const tick = async () => {
      try {
        const crawlRes = await getJson(`/v1/crawl/${jobId}`);
        if (!active) return;
        const crawlPayload = crawlRes.payload;
        const embedId = extractEmbedJobId(crawlPayload);
        let embedPayload: unknown;
        if (embedId) {
          try {
            embedPayload = (await getJson(`/v1/embed/${embedId}`)).payload;
          } catch {
            /* embed row not visible yet — keep crawl in the embedding phase */
          }
        }
        if (!active) return;
        setNowMs(Date.now());
        setRun((current) => {
          if (current.kind !== "job" || current.jobId !== jobId) return current;
          const elapsedSec = Math.max(0, (Date.now() - current.startedAtMs) / 1000);
          const snapshot = summarizeCrawl(
            crawlPayload,
            { jobId, url: current.url, elapsedSec, maxPages: current.maxPages, maxDepth: current.maxDepth },
            embedPayload,
          );
          return { ...current, snapshot, subtitle: `job ${jobId}` };
        });
      } catch {
        /* transient poll failure — keep trying on the next tick */
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
    setSettingsOpen(false);
    setHistoryOpen(false);
    setBrowseOpen(false);
    setQuery("");
    setModeAction(null); // clean command bar (no mode pill / default placeholder) in the tray
    setRun((current) => (current.kind === "job" ? { ...current, minimized: true } : current));
  }

  function expandJob() {
    setBrowseOpen(false);
    setRun((current) => (current.kind === "job" ? { ...current, minimized: false } : current));
  }

  function closeJob() {
    setRun({ kind: "idle" });
    setModeAction(null);
    setQuery("");
    setBrowseOpen(false);
  }

  async function cancelJob() {
    if (run.kind !== "job" || !run.jobId) return;
    const id = run.jobId;
    setCanceling(true);
    try {
      await invoke("axon_http_request", {
        request: { method: "POST", path: `/v1/crawl/${id}/cancel`, body: null },
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
        request: { method: "GET", path: `/v1/crawl/${id}`, body: null },
      });
      setRun({
        kind: "success",
        title: `Crawl ${host}`,
        subtitle: `job ${id}`,
        text: formatPayload("crawl", res.payload),
        outputKind: "code",
        result: { ok: res.ok, status: res.status, path: `/v1/crawl/${id}`, method: "GET", payload: res.payload },
      });
    } catch {
      /* keep the live view if the fetch fails */
    }
  }

  return { nowMs, canceling, cancelJob, viewPartialJob, minimizeJob, expandJob, closeJob };
}
