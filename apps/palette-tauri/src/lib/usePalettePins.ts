import { useCallback, useState, type Dispatch, type SetStateAction } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";

/** Tracks pinned output targets and mirrors pin state into history entries. */
export function usePalettePins(
  setHistory: Dispatch<SetStateAction<HistoryItem[]>>,
  currentTarget: string | null,
) {
  const [pinnedTargets, setPinnedTargets] = useState<Set<string>>(() => new Set());

  const togglePin = useCallback(() => {
    if (!currentTarget) return;
    setPinnedTargets((items) => {
      const next = new Set(items);
      if (next.has(currentTarget)) next.delete(currentTarget);
      else next.add(currentTarget);
      return next;
    });
    setHistory((items) =>
      items.map((item) =>
        item.target === currentTarget ? { ...item, pinned: !pinnedTargets.has(currentTarget) } : item,
      ),
    );
  }, [currentTarget, pinnedTargets, setHistory]);

  return { pinnedTargets, togglePin };
}
