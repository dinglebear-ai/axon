import { useCallback } from "react";
import type { Dispatch, SetStateAction } from "react";

import { hostFromUrl, summarizeCrawl } from "@/lib/crawlJob";
import { summarizeJob } from "@/lib/jobProgress";
import type { RunState } from "@/lib/runState";

export function useOpenJob(setRun: Dispatch<SetStateAction<RunState>>) {
  return useCallback(
    (family: string, jobId: string, label: string) => {
      const startedAtMs = Date.now();
      if (family === "crawl") {
        setRun({
          kind: "job",
          family: "crawl",
          title: `Crawling ${hostFromUrl(label)}`,
          subtitle: `job ${jobId}`,
          jobId,
          statusUrl: `/v1/crawl/${jobId}`,
          url: label,
          startedAtMs,
          maxPages: 0,
          maxDepth: 0,
          snapshot: summarizeCrawl({ job: { status: "running" } }, { jobId, url: label }),
          minimized: false,
        });
      } else if (family === "embed" || family === "extract" || family === "ingest") {
        setRun({
          kind: "asyncJob",
          family,
          title: `${family[0].toUpperCase()}${family.slice(1)}`,
          subtitle: `job ${jobId}`,
          jobId,
          statusUrl: `/v1/${family}/${jobId}`,
          target: label,
          startedAtMs,
          snapshot: summarizeJob(family, { job: { status: "running" } }, { jobId, label }),
          minimized: false,
        });
      }
    },
    [setRun],
  );
}
