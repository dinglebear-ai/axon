import { useCallback, useEffect, type Dispatch, type SetStateAction } from "react";

import { ActionIcon } from "@/components/palette/ActionIcon";
import { Button } from "@/components/ui/aurora/button";
import { Kbd } from "@/components/ui/aurora/kbd";
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

  // Delegated, stable click handlers keyed by data-index. Inline per-row arrows
  // would allocate a fresh closure every keystroke (rows re-render on every
  // query change), busting the memoized Button. These callbacks only re-create
  // when their real dependencies change — at which point a re-render is wanted.
  const handleRowClick = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      const index = Number(event.currentTarget.dataset.index);
      const action = filtered[index];
      if (!action) return;
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
    },
    [filtered, parsed.invoked, parsed.search, setSelected, onSubmit, onEnterMode],
  );

  const handleHelpClick = useCallback(
    (event: React.MouseEvent<HTMLButtonElement>) => {
      const index = Number(event.currentTarget.dataset.index);
      const action = filtered[index];
      if (action) onHelp(action);
    },
    [filtered, onHelp],
  );

  return (
    <section className="action-panel">
      <div className="panel-heading">
        <span>Actions</span>
        <span className="panel-shortcuts">
          <span><Kbd unstyled>tab</Kbd> switch</span>
          <span><Kbd unstyled>↵</Kbd> run</span>
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
                  <Button
                    variant="plain"
                    size="unstyled"
                    className="action-row-main"
                    type="button"
                    data-index={index}
                    onClick={handleRowClick}
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
                  </Button>
                  <span className="action-meta">
                    {selectedRow ? (
                      <>
                        <Button
                          variant="plain"
                          size="unstyled"
                          className="action-help-button"
                          type="button"
                          data-index={index}
                          onClick={handleHelpClick}
                          aria-label={`Help for ${action.label}`}
                          title={`Help for ${action.label}`}
                        >
                          ?
                        </Button>
                        <span className="action-run-pill">Run <Kbd unstyled>↵</Kbd></span>
                      </>
                    ) : (
                      <Kbd unstyled>{action.subcommand}</Kbd>
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
