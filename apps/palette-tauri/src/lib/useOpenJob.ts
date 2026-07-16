import type { Dispatch, SetStateAction } from "react";
import { useCallback } from "react";

import { summarizeJob } from "@/lib/jobProgress";
import type { RunState } from "@/lib/runState";

export function useOpenJob(setRun: Dispatch<SetStateAction<RunState>>) {
  return useCallback(
    (family: string, jobId: string, label: string) => {
      const startedAtMs = Date.now();
      const asyncFamily = family === "extract" ? "extract" : family === "source" ? "source" : null;
      if (!asyncFamily) return;
      setRun({
        kind: "asyncJob",
        family: asyncFamily,
        title: `${family[0].toUpperCase()}${family.slice(1)}`,
        subtitle: `job ${jobId}`,
        jobId,
        statusUrl: `/v1/jobs/${jobId}`,
        target: label,
        startedAtMs,
        snapshot: summarizeJob(asyncFamily, { job: { status: "running" } }, { jobId, label }),
        minimized: false,
      });
    },
    [setRun],
  );
}
