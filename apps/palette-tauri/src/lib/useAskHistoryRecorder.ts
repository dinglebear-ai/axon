import { useEffect, useRef } from "react";
import type { Dispatch, SetStateAction } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import type { PaletteAction } from "@/lib/actions";
import { capHistory } from "@/lib/useActionRunner";
import type { RunState } from "@/lib/runState";

export function useAskHistoryRecorder({
  active,
  run,
  setHistory,
}: {
  active?: PaletteAction;
  run: RunState;
  setHistory: Dispatch<SetStateAction<HistoryItem[]>>;
}) {
  const lastSignatureRef = useRef<string | null>(null);

  useEffect(() => {
    if (active?.subcommand !== "ask") return;
    if (run.kind !== "success" && run.kind !== "error") return;
    const prompt = run.prompt?.trim();
    if (!prompt) return;
    const signature = `${run.kind}\0${prompt}\0${run.text}\0${run.result.status}`;
    if (lastSignatureRef.current === signature) return;
    lastSignatureRef.current = signature;
    const status = run.result.status || (run.result.ok ? 200 : 0);
    const item: HistoryItem = {
      action: active,
      target: prompt,
      status,
      title: run.title,
      subtitle: run.subtitle,
      text: run.text,
      outputKind: run.outputKind,
      result: run.result,
      prompt,
      transcript: run.transcript,
      when: "just now",
      duration: status >= 200 && status < 300 ? undefined : "fail",
    };
    setHistory((items) =>
      capHistory([
        item,
        ...items.filter((existing) => {
          if (existing.action.subcommand !== "ask") return true;
          return (existing.prompt ?? existing.target) !== prompt || existing.text !== run.text;
        }),
      ]),
    );
  }, [active, run, setHistory]);
}
