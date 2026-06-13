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
  config: PaletteConfig | null;
  endpointLabel: string;
  endpointTone: string;
  hasQuery: boolean;
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

export function PaletteCommandBar({
  active,
  config,
  endpointLabel,
  endpointTone,
  hasQuery,
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
  }, [modeAction?.subcommand]);

  return (
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
        <span className={`axon-status-dot axon-status-${endpointTone}`} />
      </button>
      <span className="axon-divider" aria-hidden="true" />
      <div className="command-input-wrap" onClick={() => focusInput()}>
        {modeAction && ModeIcon ? (
          <div className="command-action-switcher" ref={switcherRef}>
            <button
              className={`command-action-trigger command-mode-icon-${modeAction.tone}`}
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                setSwitcherOpen((open) => !open);
              }}
              aria-haspopup="menu"
              aria-expanded={switcherOpen}
              aria-label={`Switch from ${modeAction.label}`}
              title={`${modeAction.label} mode`}
            >
              <ModeIcon size={15} strokeWidth={1.9} aria-hidden="true" />
              <span>{actionDisplayMeta(modeAction).label}</span>
              <ChevronDown size={13} strokeWidth={1.8} aria-hidden="true" />
            </button>
            {switcherOpen && (
              <div className="command-action-menu" role="menu" aria-label="Switch action">
                {switcherActions.map((action) => {
                  const Icon = actionIcon(action.subcommand);
                  const meta = actionDisplayMeta(action);
                  return (
                    <button
                      key={action.subcommand}
                      className={`command-action-option command-action-option-${action.tone}`}
                      type="button"
                      role="menuitem"
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
          aria-label={modeAction ? `${modeAction.label} argument` : "Axon command"}
        />
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
