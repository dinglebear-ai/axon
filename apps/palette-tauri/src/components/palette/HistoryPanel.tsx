import { Activity, Sparkles } from "lucide-react";

import { ActionIcon } from "@/components/palette/ActionIcon";
import { Button } from "@/components/ui/aurora/button";
import type { PaletteAction } from "@/lib/actions";
import type { PaletteResult } from "@/lib/axonClient";

export interface HistoryItem {
  action: PaletteAction;
  target: string;
  status: number;
  title: string;
  subtitle: string;
  when: string;
  pinned?: boolean;
  running?: boolean;
  duration?: string;
  text?: string;
  outputKind?: "markdown" | "code";
  result?: PaletteResult;
}

export function HistoryPanel({
  items,
  onClear,
  onOpen,
}: {
  items: HistoryItem[];
  onClear: () => void;
  onOpen: (item: HistoryItem) => void;
}) {
  return (
    <section className="history-panel">
      <header className="history-head">
        <span>Recent runs</span>
        {/* Scoped by the `.history-head button` element selector — keep the
            rendered native button so that selector stays the styling source. */}
        {items.length > 0 ? <Button variant="plain" size="unstyled" type="button" onClick={onClear}>clear</Button> : null}
      </header>
      {items.length === 0 ? (
        <div className="history-empty">
          <span><Activity size={20} /></span>
          <strong>No runs yet</strong>
          <p>Run an operation and results land here. Start by typing a command above.</p>
        </div>
      ) : (
        <div className="history-list aurora-scrollbar">
          {items.map((item, index) => {
            const ok = item.status >= 200 && item.status < 300;
            return (
              <Button variant="plain" size="unstyled" className="history-row" type="button" key={`${item.action.subcommand}-${item.target}-${index}`} onClick={() => onOpen(item)}>
                <ActionIcon action={item.action} selected={false} />
                <span className="history-main">
                  <span>{item.target}</span>
                  <span>{item.action.label} · {item.when}</span>
                </span>
                {item.pinned ? <Sparkles className="history-pin" size={13} /> : null}
                {item.running ? (
                  <span className="history-live"><span />live</span>
                ) : (
                  <span className="history-duration">{item.duration ?? "—"}</span>
                )}
                <span className={ok ? "history-status history-status-ok" : "history-status history-status-error"}>{item.status || "ERR"}</span>
              </Button>
            );
          })}
        </div>
      )}
    </section>
  );
}
