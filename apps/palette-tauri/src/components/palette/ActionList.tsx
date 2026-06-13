import { useEffect, type Dispatch, type SetStateAction } from "react";

import { ActionIcon } from "@/components/palette/ActionIcon";
import { ScrollArea } from "@/components/ui/aurora/scroll-area";
import { acceptsDirectUrl, type PaletteAction } from "@/lib/actions";
import { isAsyncAction } from "@/lib/actionHelp";
import { actionDisplayMeta } from "@/lib/actionMeta";
import { looksLikeUrl, type ParsedCommand } from "@/lib/paletteView";

interface ActionListProps {
  filtered: PaletteAction[];
  selected: number;
  setSelected: Dispatch<SetStateAction<number>>;
  parsed: ParsedCommand;
  onSubmit: (action: PaletteAction) => void;
  onEnterMode: (action: PaletteAction) => void;
  onHelp: (action: PaletteAction) => void;
}

// The searchable, keyboard-navigable list of palette actions. A row click runs
// the action directly when a command is invoked or the query is a bare URL the
// action accepts, otherwise it enters argument mode for that action.
export function ActionList({ filtered, selected, setSelected, parsed, onSubmit, onEnterMode, onHelp }: ActionListProps) {
  // Keyboard nav moves the selection; keep the selected row in view by scrolling
  // the list viewport the minimum amount needed (so arrowing past the fold works).
  useEffect(() => {
    const el = document.querySelector(".action-scroll-viewport .action-row-selected");
    if (el instanceof HTMLElement) el.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, [selected]);

  return (
    <section className="action-panel">
      <div className="panel-heading">
        <span>Actions</span>
        <span className="panel-shortcuts">
          <span><kbd>tab</kbd> switch</span>
          <span><kbd>↵</kbd> run</span>
        </span>
      </div>
      <ScrollArea className="action-scroll" viewportClassName="action-scroll-viewport">
        <div className="action-list">
          {filtered.map((action, index) => {
            const meta = actionDisplayMeta(action);
            const previous = index > 0 ? actionDisplayMeta(filtered[index - 1]) : null;
            const selectedRow = index === selected;
            return (
              <div className="action-group-item" key={action.subcommand}>
                {(!previous || previous.category !== meta.category) && (
                  <div className="action-section-heading">
                    <span>{meta.category}</span>
                    <span>{meta.input} → {meta.output}</span>
                  </div>
                )}
                <div
                  className={selectedRow ? "action-row action-row-selected" : "action-row"}
                  onFocusCapture={() => setSelected(index)}
                  onPointerEnter={() => setSelected(index)}
                >
                  <button
                    className="action-row-main"
                    type="button"
                    onClick={() => {
                      setSelected(index);
                      if (parsed.invoked) {
                        onSubmit(action);
                      } else if (action.argMode === "none") {
                        // No-input actions run immediately — no empty argument prompt.
                        onSubmit(action);
                      } else if (acceptsDirectUrl(action) && looksLikeUrl(parsed.search)) {
                        onSubmit(action);
                      } else {
                        onEnterMode(action);
                      }
                    }}
                  >
                    <ActionIcon action={action} selected={selectedRow} />
                    <span className="action-main">
                      <span className="action-title-line">
                        <span className="action-label">{meta.label}</span>
                        <span className="action-method">{meta.method}</span>
                        <span className="action-endpoint">{meta.endpoint}</span>
                        {isAsyncAction(action) ? (
                          <span className="action-async">ASYNC</span>
                        ) : null}
                      </span>
                      <span className="action-description">{action.description}</span>
                    </span>
                  </button>
                  <span className="action-meta">
                    {selectedRow ? (
                      <>
                        <button
                          className="action-help-button"
                          type="button"
                          onClick={() => onHelp(action)}
                          aria-label={`Help for ${action.label}`}
                          title={`Help for ${action.label}`}
                        >
                          ?
                        </button>
                        <span className="action-run-pill">Run <kbd>↵</kbd></span>
                      </>
                    ) : (
                      <kbd>{action.subcommand}</kbd>
                    )}
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      </ScrollArea>
    </section>
  );
}
