import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { outputKindFor } from "@/lib/format";
import type { RunState } from "@/lib/runState";

export function runStateFromHistory(item: HistoryItem): RunState | null {
  if (item.text == null) return null;
  const result = item.result ?? {
    ok: item.status >= 200 && item.status < 300,
    status: item.status,
    path: item.action.kind === "local" ? `palette://${item.action.subcommand}` : item.action.subcommand,
    method: item.action.kind === "local" ? "GET" as const : "POST" as const,
    payload: null,
  };
  return {
    kind: result.ok ? "success" : "error",
    title: item.title,
    subtitle: item.subtitle,
    text: item.text,
    outputKind: item.outputKind ?? outputKindFor(item.action.subcommand),
    result,
  };
}
