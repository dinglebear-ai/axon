import type { PaletteResult } from "@/lib/axonClient";
import type { OutputKind } from "@/lib/format";
import type { CrawlSnapshot } from "@/lib/crawlJob";
import type { AsyncJobFamily, JobSnapshot } from "@/lib/jobProgress";

export interface AskSource {
  label: string;
  url?: string;
  title?: string;
}

export interface AskActivity {
  id: string;
  label: string;
  detail?: string;
  kind?: "thinking" | "tool" | "done";
}

export interface AskTurn {
  id: string;
  role: "user" | "assistant";
  content: string;
  pending?: boolean;
  sources?: AskSource[];
  activities?: AskActivity[];
}

export type RunState =
  | { kind: "idle" }
  | { kind: "running"; title: string; subtitle: string; prompt?: string; transcript?: AskTurn[] }
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
      transcript?: AskTurn[];
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
      // Live async-job view for embed/extract/ingest. Polled from the real
      // backend the same way as crawl, but with the simpler `JobSnapshot`
      // model. `minimized` drives the compact tray vs. the full job card.
      kind: "asyncJob";
      family: AsyncJobFamily;
      title: string;
      subtitle: string;
      jobId: string;
      statusUrl: string;
      target: string;
      startedAtMs: number;
      snapshot: JobSnapshot;
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
      transcript?: AskTurn[];
    };
