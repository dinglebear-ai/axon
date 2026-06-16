import { ArrowLeft, ChevronDown, HelpCircle, Search, Send, Settings } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { actionIcon } from "@/components/palette/ActionIcon";
import { AxonMark } from "@/components/palette/AxonMark";
import type { PaletteConfig } from "@/lib/axonClient";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import { actionDisplayMeta } from "@/lib/actionMeta";
import { argumentPlaceholder, focusInput, sortActionsForDisplay } from "@/lib/paletteView";

interface PaletteCommandBarProps {
  active?: PaletteAction;
  activeDescendantId?: string;
  config: PaletteConfig | null;
  endpointLabel: string;
  endpointTone: string;
  hasQuery: boolean;
  listboxOpen: boolean;
  modeAction: PaletteAction | null;
  query: string;
  running: boolean;
  settingsOpen: boolean;
  showBackButton: boolean;
  submitDisabled: boolean;
  validation: string;
  onBack: () => void;
  onHelp: (action?: PaletteAction, unknownTarget?: string) => void;
  onInputKeyDown: React.KeyboardEventHandler<HTMLInputElement>;
  onQueryChange: (value: string) => void;
  onReset: () => void;
  onSubmit: (action: PaletteAction) => void;
  onSwitchAction: (action: PaletteAction) => void;
  onToggleMaximize: () => void;
  onToggleSettings: () => void;
}

// Human-readable connection state for the status dot's sr-only label (A11Y-M2).
function endpointStatusLabel(tone: string, endpointLabel: string): string {
  switch (tone) {
    case "error":
      return "Server: connection error";
    case "syncing":
      return `Server: ${endpointLabel}`;
    default:
      return `Server: ${endpointLabel}`;
  }
}

export function PaletteCommandBar({
  active,
  activeDescendantId,
  config,
  endpointLabel,
  endpointTone,
  hasQuery,
  listboxOpen,
  modeAction,
  query,
  running,
  settingsOpen,
  showBackButton,
  submitDisabled,
  validation,
  onBack,
  onHelp,
  onInputKeyDown,
  onQueryChange,
  onReset,
  onSubmit,
  onSwitchAction,
  onToggleMaximize,
  onToggleSettings,
}: PaletteCommandBarProps) {
  const ModeIcon = modeAction ? actionIcon(modeAction.subcommand) : null;
  const switcherRef = useRef<HTMLDivElement | null>(null);
  const switcherTriggerRef = useRef<HTMLButtonElement | null>(null);
  const [switcherOpen, setSwitcherOpen] = useState(false);
  const switcherActions = useMemo(
    () => sortActionsForDisplay(ACTIONS).filter((action) => action.subcommand !== modeAction?.subcommand),
    [modeAction?.subcommand],
  );

  useEffect(() => {
    if (!switcherOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (switcherRef.current?.contains(event.target as Node)) return;
      setSwitcherOpen(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [switcherOpen]);

  useEffect(() => {
    setSwitcherOpen(false);
  }, []);

  // A11Y-M2 — surface submit validation as text tied to the input via
  // aria-describedby (not just a `title` tooltip). The id is referenced only when
  // there is an active validation message so AT does not announce an empty node.
  const validationId = "command-validation";

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: command-bar is a layout container; double-click toggles window chrome, not an interactive widget
    <section
      className="command-bar"
      onDoubleClick={(event) => {
        if ((event.target as HTMLElement).closest("input, button, a")) return;
        onToggleMaximize();
      }}
    >
      {showBackButton && (
        <button className="command-back" type="button" onClick={onBack} aria-label="Back" title="Back">
          <ArrowLeft size={17} />
        </button>
      )}
      <button
        className="axon-brand"
        type="button"
        onClick={onReset}
        title={`${config?.serverUrl ?? endpointLabel}${config?.collection ? ` · ${config.collection}` : ""}`}
        aria-label="Reset Axon palette"
      >
        <AxonMark size={24} />
        <span className="axon-word">Axon</span>
        <span className={`axon-status-dot axon-status-${endpointTone}`}>
          <span className="sr-only">{endpointStatusLabel(endpointTone, endpointLabel)}</span>
        </span>
      </button>
      <span className="axon-divider" aria-hidden="true" />
      {/* biome-ignore lint/a11y/noStaticElementInteractions: click-to-focus convenience; the real control is the command input within */}
      <div className="command-input-wrap" onClick={() => focusInput()}>
        {modeAction && ModeIcon ? (
          // A11Y-H1 — the action switcher is an `aria-expanded` disclosure of plain
          // Tab-focusable buttons (not a `role="menu"`), so each item is reachable
          // by keyboard with no custom menu key handling. Escape on the trigger
          // closes it and restores focus to the trigger.
          <div className="command-action-switcher" ref={switcherRef}>
            <button
              ref={switcherTriggerRef}
              className={`command-action-trigger command-mode-icon-${modeAction.tone}`}
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                setSwitcherOpen((open) => !open);
              }}
              onKeyDown={(event) => {
                if (event.key === "Escape" && switcherOpen) {
                  event.stopPropagation();
                  setSwitcherOpen(false);
                }
              }}
              aria-haspopup="true"
              aria-expanded={switcherOpen}
              aria-controls="command-action-disclosure"
              aria-label={`Switch from ${modeAction.label}`}
              title={`${modeAction.label} mode`}
            >
              <ModeIcon size={15} strokeWidth={1.9} aria-hidden="true" />
              <span>{actionDisplayMeta(modeAction).label}</span>
              <ChevronDown size={13} strokeWidth={1.8} aria-hidden="true" />
            </button>
            {switcherOpen && (
              // biome-ignore lint/a11y/noStaticElementInteractions: disclosure group; Escape closes it — the switch buttons within are the controls
              <div
                id="command-action-disclosure"
                className="command-action-menu"
                role="group"
                aria-label="Switch action"
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.stopPropagation();
                    setSwitcherOpen(false);
                    switcherTriggerRef.current?.focus();
                  }
                }}
              >
                {switcherActions.map((action) => {
                  const Icon = actionIcon(action.subcommand);
                  const meta = actionDisplayMeta(action);
                  return (
                    <button
                      key={action.subcommand}
                      className={`command-action-option command-action-option-${action.tone}`}
                      type="button"
                      onClick={(event) => {
                        event.stopPropagation();
                        setSwitcherOpen(false);
                        onSwitchAction(action);
                      }}
                    >
                      <Icon size={15} strokeWidth={1.85} aria-hidden="true" />
                      <span>
                        <strong>{meta.label}</strong>
                        <small>{meta.input} to {meta.output}</small>
                      </span>
                      <kbd>{action.subcommand}</kbd>
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        ) : (
          <Search size={16} strokeWidth={1.65} aria-hidden="true" />
        )}
        <input
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          onKeyDown={onInputKeyDown}
          placeholder={modeAction ? argumentPlaceholder(modeAction) : hasQuery ? active?.example ?? "Search commands" : "Search or run an operation — scrape, crawl, map, ask…"}
          className="command-input"
          role="combobox"
          aria-expanded={listboxOpen}
          aria-controls={listboxOpen ? "palette-action-list" : undefined}
          aria-activedescendant={listboxOpen ? activeDescendantId : undefined}
          aria-autocomplete="list"
          aria-describedby={validation ? validationId : undefined}
          aria-label={modeAction ? `${modeAction.label} argument` : "Axon command"}
        />
        {validation && (
          <span id={validationId} className="sr-only" role="status">
            {validation}
          </span>
        )}
      </div>
      <button
        className="command-help"
        type="button"
        onClick={() => onHelp(active, active ? undefined : query.trim())}
        disabled={running}
        aria-label={active ? `Help for ${active.label}` : "Help"}
        title={active ? `Help for ${active.label}` : "Help"}
      >
        <HelpCircle size={15} />
      </button>
      <button
        className={active && !validation ? `command-submit command-submit-${active.tone}` : "command-submit"}
        type="button"
        onClick={() => active && onSubmit(active)}
        disabled={submitDisabled}
        aria-label="Run selected action"
        title={validation || "Run selected action"}
      >
        <Send size={15} />
      </button>
      <button
        className={settingsOpen ? "command-settings command-settings-active" : "command-settings"}
        type="button"
        onClick={onToggleSettings}
        aria-label="Settings"
        title="Settings"
      >
        <Settings size={15} />
      </button>
    </section>
  );
}
