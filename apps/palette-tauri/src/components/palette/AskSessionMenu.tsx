import { History } from "lucide-react";
import type { RefObject } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { Button } from "@/components/ui/aurora/button";

export function AskSessionMenu({
  askSessions,
  askSessionsRef,
  selectedAskSession,
  onAskSessionsOpenChange,
  onResumeAskSession,
  onSelect,
}: {
  askSessions: HistoryItem[];
  askSessionsRef: RefObject<HTMLDivElement | null>;
  selectedAskSession: number;
  onAskSessionsOpenChange: (open: boolean) => void;
  onResumeAskSession: (item: HistoryItem) => void;
  onSelect: (index: number) => void;
}) {
  return (
    <div
      id="ask-session-list"
      className="ask-session-menu"
      ref={askSessionsRef}
      role="listbox"
      aria-label="Previous Ask sessions"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.stopPropagation();
          onAskSessionsOpenChange(false);
        }
      }}
    >
      <div className="ask-session-options aurora-scrollbar">
        {askSessions.map((item, index) => (
          <Button
            variant="plain"
            size="unstyled"
            className="ask-session-option"
            type="button"
            role="option"
            id={`ask-session-option-${index}`}
            aria-selected={index === selectedAskSession}
            key={`${item.prompt ?? item.target}-${item.when}`}
            onMouseEnter={() => onSelect(index)}
            onClick={(event) => {
              event.stopPropagation();
              onResumeAskSession(item);
            }}
          >
            <History size={15} strokeWidth={1.8} aria-hidden="true" />
            <span>
              <strong>{item.prompt ?? item.target}</strong>
              <small>{item.text}</small>
            </span>
            <em>{item.when}</em>
          </Button>
        ))}
      </div>
    </div>
  );
}
