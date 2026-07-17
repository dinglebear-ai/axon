import type { PaletteResult } from "@/lib/axonClient";
import type { OutputKind } from "@/lib/format";
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

export interface ChatSuggestion {
  title: string;
  url?: string;
  snippet?: string;
  score?: number;
  rank: number;
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
      // Live async-job view for source/extract jobs. Polled from the real
      // backend with the simpler `JobSnapshot`
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
