import { type Dispatch, type SetStateAction, useCallback } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import type { PendingActionConfirmation } from "@/lib/actionGuard";
import { runStateFromHistory } from "@/lib/historyRun";
import type { ViewIntent } from "@/lib/paletteViewState";
import type { RunState } from "@/lib/runState";

export function useAppHistoryCallbacks(
  dispatchView: Dispatch<ViewIntent>,
  setRun: Dispatch<SetStateAction<RunState>>,
  setPendingConfirmation: Dispatch<SetStateAction<PendingActionConfirmation | null>>,
  setAskSessionsOpen: Dispatch<SetStateAction<boolean>>,
  setQuery: Dispatch<SetStateAction<string>>,
) {
  const onHistory = useCallback(() => {
    setRun({ kind: "idle" });
    dispatchView({ type: "openHistory" });
  }, [dispatchView, setRun]);
  const onResumeAskSession = useCallback(
    (item: HistoryItem) => {
      setPendingConfirmation(null);
      setAskSessionsOpen(false);
      dispatchView({ type: "openHistoryItem", action: item.action });
      setQuery(item.prompt ?? item.target);
      const historyRun = runStateFromHistory(item);
      setRun(historyRun ?? { kind: "idle" });
    },
    [dispatchView, setAskSessionsOpen, setPendingConfirmation, setQuery, setRun],
  );
  const onCollapse = useCallback(() => {
    setRun({ kind: "idle" });
    setQuery("");
    dispatchView({ type: "collapse" });
  }, [dispatchView, setQuery, setRun]);
  return { onCollapse, onHistory, onResumeAskSession };
}
