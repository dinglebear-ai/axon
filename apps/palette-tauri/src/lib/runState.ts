import type { PaletteResult } from "@/lib/axonClient";
import type { OutputKind } from "@/lib/format";
import type { CrawlSnapshot } from "@/lib/crawlJob";

export type RunState =
  | { kind: "idle" }
  | { kind: "running"; title: string; subtitle: string; prompt?: string }
  | {
      kind: "streaming";
      title: string;
      subtitle: string;
      text: string;
      outputKind: OutputKind;
      requestId: string;
      path: string;
      actionLabel: string;
      prompt?: string;
    }
  | {
      // Live async-job view (currently: crawl). Polled from the real backend;
      // `snapshot` is refreshed every poll tick. `minimized` drives the compact
      // collapsed tray vs. the full expanded job card.
      kind: "job";
      family: "crawl";
      title: string;
      subtitle: string;
      jobId: string;
      statusUrl: string;
      url: string;
      startedAtMs: number;
      maxPages: number;
      maxDepth: number;
      snapshot: CrawlSnapshot;
      minimized: boolean;
    }
  | {
      kind: "success" | "error";
      title: string;
      subtitle: string;
      text: string;
      outputKind: OutputKind;
      result: PaletteResult;
      prompt?: string;
    };
