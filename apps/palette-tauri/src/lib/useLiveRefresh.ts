import { useCallback, useEffect, useState, type Dispatch, type SetStateAction } from "react";

import { invoke } from "@/lib/invoke";
import type { RunState } from "@/lib/runState";

// Zero-input read-only views whose data is genuinely live (collection counts,
// the job queue) and worth re-polling while open. Deliberately small: slow-
// growing lists (sources/domains) are not auto-refreshed — they re-fetch a large
// payload for little benefit and reorder under the reader. The live `stats`
// counts already surface that growth.
const LIVE_REFRESH_PATHS = new Set(["/v1/stats", "/v1/status"]);

export function isLiveRefreshablePath(path: string | undefined): boolean {
  return path != null && LIVE_REFRESH_PATHS.has(path);
}

interface UseLiveRefreshArgs {
  run: RunState;
  setRun: Dispatch<SetStateAction<RunState>>;
  paused: boolean;
  intervalMs?: number;
}

export interface LiveRefreshState {
  /** True when the current view is an auto-refreshing one. */
  active: boolean;
  paused: boolean;
  lastRefreshedAtMs: number | null;
  /** Re-poll once, immediately (manual refresh button). */
  refreshNow: () => void;
}

// Re-polls the current view's endpoint on an interval while it is a live,
// successful zero-input result. Identity is the result `path` (e.g. `/v1/stats`),
// so navigating away cleanly stops the loop. Updates `run.result.payload` in
// place; the structured view (StatsView/StatusView) re-renders with fresh data.
export function useLiveRefresh({ run, setRun, paused, intervalMs = 2000 }: UseLiveRefreshArgs): LiveRefreshState {
  const [lastRefreshedAtMs, setLastRefreshedAtMs] = useState<number | null>(null);

  const path = run.kind === "success" && "result" in run ? run.result.path : "";
  const active = isLiveRefreshablePath(path);

  const refreshNow = useCallback(() => {
    if (!isLiveRefreshablePath(path)) return;
    void (async () => {
      try {
        const res = await invoke<{ ok: boolean; status: number; payload: unknown }>("axon_http_request", {
          request: { method: "GET", path, body: null },
        });
        setRun((current) =>
          current.kind === "success" && "result" in current && current.result.path === path
            ? { ...current, result: { ...current.result, ok: res.ok, status: res.status, payload: res.payload } }
            : current,
        );
        setLastRefreshedAtMs(Date.now());
      } catch {
        /* transient — keep the last good snapshot until the next tick */
      }
    })();
  }, [path, setRun]);

  useEffect(() => {
    if (!active || paused) return;
    const id = window.setInterval(refreshNow, intervalMs);
    return () => window.clearInterval(id);
  }, [active, paused, intervalMs, refreshNow]);

  return { active, paused, lastRefreshedAtMs, refreshNow };
}
