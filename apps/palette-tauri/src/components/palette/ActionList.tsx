import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";

import { ActionIcon } from "@/components/palette/ActionIcon";
import { Button } from "@/components/ui/aurora/button";
import { Kbd } from "@/components/ui/aurora/kbd";
import { ScrollArea } from "@/components/ui/aurora/scroll-area";
import { acceptsDirectUrl, actionMatches, type PaletteAction } from "@/lib/actions";
import { isAsyncAction } from "@/lib/actionHelp";
import { actionDisplayMeta } from "@/lib/actionMeta";
import { looksLikeUrl, type ParsedCommand } from "@/lib/paletteView";

interface ActionListProps {
  filtered: PaletteAction[];
  selected: number;
  setSelected: Dispatch<SetStateAction<number>>;
  parsed: ParsedCommand;
  onSubmit: (action: PaletteAction, argumentOverride?: string) => void;
  onEnterMode: (action: PaletteAction) => void;
  onHelp: (action: PaletteAction) => void;
}

// Stable per-option id shared with the command-bar input's aria-activedescendant
// so AT announces the highlighted option as the listbox's active descendant.
export function actionOptionId(action: PaletteAction): string {
  return `action-${action.subcommand}`;
}

// The searchable, keyboard-navigable list of palette actions. A row click runs
// the action directly when a command is invoked or the query is a bare URL the
// action accepts, otherwise it enters argument mode for that action.
//
// A11Y-C1 — the container is a `role="listbox"` (labelled "Actions"); each row is
// a `role="option"` with `aria-selected` and a stable `id`; each category is a
// `role="group"` with an `aria-label`, so the section headings live inside a
// labelled group rather than as bare listbox children. The command-bar input
// owns the `aria-activedescendant` pointer at the selected option.
export function ActionList({ filtered, selected, setSelected, parsed, onSubmit, onEnterMode, onHelp }: ActionListProps) {
  // Keyboard nav moves the selection; keep the selected row in view by scrolling
  // the list viewport the minimum amount needed (so arrowing past the fold works).
  // L3 — track the selected row with a ref instead of a `.action-row-selected`
  // DOM class query.
  const selectedRowRef = useRef<HTMLButtonElement | null>(null);
  useEffect(() => {
    selectedRowRef.current?.scrollIntoView({ block: "nearest", inline: "nearest" });
  }, []);

  // Group consecutive actions by category (ACTIONS arrive category-sorted) while
  // preserving each action's absolute index for selection/keys.
  const groups: { category: string; items: { action: PaletteAction; index: number }[] }[] = [];
  filtered.forEach((action, index) => {
    const category = actionDisplayMeta(action).category;
    const last = groups[groups.length - 1];
    if (last && last.category === category) {
      last.items.push({ action, index });
    } else {
      groups.push({ category, items: [{ action, index }] });
    }
  });

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
        <div id="palette-action-list" role="listbox" aria-label="Actions" className="action-list">
          {groups.map((group) => {
            const headingMeta = actionDisplayMeta(group.items[0].action);
            // Key on the group's first index — a category can recur in
            // non-consecutive runs under relevance sort, so its name is not unique.
            return (
              <div
                className="action-group"
                role="presentation"
                key={`group-${group.items[0].index}`}
              >
                <div className="action-section-heading" aria-hidden="true">
                  <span>{group.category}</span>
                  <span>{headingMeta.input} → {headingMeta.output}</span>
                </div>
                {group.items.map(({ action, index }) => {
                  const meta = actionDisplayMeta(action);
                  const selectedRow = index === selected;
                  // A11Y-C1 — the `.action-row-main` button IS the listbox option
                  // (role="option"); the `.action-row` is a presentation container so
                  // the secondary help button sits as a SIBLING of the option, never
                  // nested inside it (which would be invalid nested-interactive ARIA).
                  // Options are activated via the combobox/aria-activedescendant, so
                  // they stay out of the Tab order (tabIndex=-1).
                  return (
                    <div className="action-group-item" role="presentation" key={action.subcommand}>
                      <div
                        role="presentation"
                        className={selectedRow ? "action-row action-row-selected" : "action-row"}
                        onPointerEnter={() => setSelected(index)}
                      >
                        <Button
                          variant="plain"
                          size="unstyled"
                          id={actionOptionId(action)}
                          role="option"
                          aria-selected={selectedRow}
                          tabIndex={-1}
                          ref={selectedRow ? selectedRowRef : undefined}
                          className="action-row-main"
                          type="button"
                          onFocusCapture={() => setSelected(index)}
                          onClick={() => {
                            setSelected(index);
                            if (parsed.invoked) {
                              onSubmit(action);
                            } else if (action.argMode === "none") {
                              // No-input actions run immediately — no empty argument prompt.
                              onSubmit(action);
                            } else if (
                              action.subcommand === "ask" &&
                              parsed.search.trim().length > 0 &&
                              !actionMatches(action, parsed.search)
                            ) {
                              // Free text in the empty command field is an Ask prompt.
                              onSubmit(action, parsed.search);
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
                        </Button>
                        {/* Secondary row affordances are a pointer convenience that
                            duplicate the command-bar Help control and run-on-Enter;
                            they are hidden from the listbox a11y tree (and kept out
                            of the Tab order) so the listbox contains only options. */}
                        <span className="action-meta" aria-hidden="true">
                          {selectedRow ? (
                            <>
                              <Button
                                variant="plain"
                                size="unstyled"
                                className="action-help-button"
                                type="button"
                                tabIndex={-1}
                                onClick={() => onHelp(action)}
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
            );
          })}
        </div>
      </ScrollArea>
    </section>
  );
}
