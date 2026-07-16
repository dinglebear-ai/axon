import {
  ArrowLeft,
  ChevronDown,
  CircleHelp,
  Menu,
  Search,
  Send,
  Settings,
  SlidersHorizontal,
  TerminalSquare,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { actionIcon } from "@/components/palette/ActionIcon";
import { AskSessionMenu } from "@/components/palette/AskSessionMenu";
import { AxonMark } from "@/components/palette/AxonMark";
import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import { Kbd } from "@/components/ui/aurora/kbd";
import { actionDisplayMeta } from "@/lib/actionMeta";
import { ACTIONS, type PaletteAction } from "@/lib/actions";
import type { PaletteConfig } from "@/lib/axonClient";
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
  askSessions: HistoryItem[];
  askSessionsOpen: boolean;
  onAskSessionsOpenChange: (open: boolean) => void;
  onBack: () => void;
  onHelp: (action?: PaletteAction, unknownTarget?: string) => void;
  onInputKeyDown: React.KeyboardEventHandler<HTMLInputElement>;
  onQueryChange: (value: string) => void;
  onReset: () => void;
  onResumeAskSession: (item: HistoryItem) => void;
  onSubmit: (action: PaletteAction) => void;
  onSwitchAction: (action: PaletteAction) => void;
  onSwitcherOpenChange: (open: boolean) => void;
  onToggleMaximize: () => void;
  onToggleSettings: () => void;
}

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

function sentenceCase(value: string): string {
  return value ? value[0].toUpperCase() + value.slice(1) : value;
}

const SWITCHER_GROUPS = [
  "Fetch & read",
  "Sources",
  "Search & discover",
  "Reason",
  "System",
  "Jobs",
] as const;

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
  askSessions,
  askSessionsOpen,
  onAskSessionsOpenChange,
  onBack,
  onHelp,
  onInputKeyDown,
  onQueryChange,
  onReset,
  onResumeAskSession,
  onSubmit,
  onSwitchAction,
  onSwitcherOpenChange,
  onToggleMaximize,
  onToggleSettings,
}: PaletteCommandBarProps) {
  const ModeIcon = modeAction ? actionIcon(modeAction.subcommand) : null;
  const switcherRef = useRef<HTMLDivElement | null>(null);
  const switcherTriggerRef = useRef<HTMLButtonElement | null>(null);
  const menuRef = useRef<HTMLDivElement | null>(null);
  const menuTriggerRef = useRef<HTMLButtonElement | null>(null);
  const askSessionsRef = useRef<HTMLDivElement | null>(null);
  const [switcherOpen, setSwitcherOpen] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [selectedAskSession, setSelectedAskSession] = useState(0);
  const switcherActions = useMemo(
    () =>
      sortActionsForDisplay(ACTIONS).filter(
        (action) => action.subcommand !== modeAction?.subcommand,
      ),
    [modeAction?.subcommand],
  );
  const groupedSwitcherActions = useMemo(() => {
    const groups = new Map<string, PaletteAction[]>();
    for (const action of switcherActions) {
      const category = actionDisplayMeta(action).category;
      groups.set(category, [...(groups.get(category) ?? []), action]);
    }
    return [
      ...SWITCHER_GROUPS.map((category) => ({ category, actions: groups.get(category) ?? [] })),
      ...[...groups.entries()]
        .filter(
          ([category]) => !SWITCHER_GROUPS.includes(category as (typeof SWITCHER_GROUPS)[number]),
        )
        .map(([category, actions]) => ({ category, actions })),
    ].filter((group) => group.actions.length > 0);
  }, [switcherActions]);

  useEffect(() => {
    if (!switcherOpen && !menuOpen && !askSessionsOpen) return;
    const onPointerDown = (event: PointerEvent) => {
      if (switcherRef.current?.contains(event.target as Node)) return;
      if (menuRef.current?.contains(event.target as Node)) return;
      if (askSessionsRef.current?.contains(event.target as Node)) return;
      setSwitcherOpen(false);
      setMenuOpen(false);
      onAskSessionsOpenChange(false);
    };
    window.addEventListener("pointerdown", onPointerDown);
    return () => window.removeEventListener("pointerdown", onPointerDown);
  }, [askSessionsOpen, menuOpen, onAskSessionsOpenChange, switcherOpen]);

  useEffect(() => {
    setSwitcherOpen(false);
    setMenuOpen(false);
    onAskSessionsOpenChange(false);
  }, [onAskSessionsOpenChange]);

  useEffect(() => {
    onSwitcherOpenChange(switcherOpen || menuOpen || askSessionsOpen);
  }, [askSessionsOpen, menuOpen, onSwitcherOpenChange, switcherOpen]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reset highlighted session when the visible session set changes.
  useEffect(() => {
    setSelectedAskSession(0);
  }, [askSessions.length, askSessionsOpen]);

  function onCommandInputKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (askSessionsOpen && askSessions.length > 0) {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        setSelectedAskSession((index) => Math.min(index + 1, askSessions.length - 1));
        return;
      }
      if (event.key === "ArrowUp") {
        event.preventDefault();
        setSelectedAskSession((index) => Math.max(index - 1, 0));
        return;
      }
      if (event.key === "Enter") {
        event.preventDefault();
        onResumeAskSession(askSessions[selectedAskSession]);
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        onAskSessionsOpenChange(false);
        return;
      }
    }
    onInputKeyDown(event);
  }

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
        <Button
          variant="plain"
          size="unstyled"
          className="command-back"
          type="button"
          onClick={onBack}
          aria-label="Back"
          title="Back"
        >
          <ArrowLeft size={17} />
        </Button>
      )}
      <Button
        variant="plain"
        size="unstyled"
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
      </Button>
      <span className="axon-divider" aria-hidden="true" />
      {/* biome-ignore lint/a11y/noStaticElementInteractions: click-to-focus convenience; the real control is the command input within */}
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: keyboard users focus the input directly; this wrapper only expands the pointer target */}
      <div
        className="command-input-wrap"
        onClick={() => {
          focusInput();
          if (modeAction?.subcommand === "ask" && askSessions.length > 0)
            onAskSessionsOpenChange(true);
        }}
      >
        {modeAction && ModeIcon ? (
          <div className="command-action-switcher" ref={switcherRef}>
            <Button
              variant="plain"
              size="unstyled"
              ref={switcherTriggerRef}
              className={`command-action-trigger command-mode-icon-${modeAction.tone}`}
              type="button"
              onClick={(event) => {
                event.stopPropagation();
                setMenuOpen(false);
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
            </Button>
            {switcherOpen && (
              // biome-ignore lint/a11y/noStaticElementInteractions: disclosure group; Escape closes it — the switch buttons within are the controls
              <div
                id="command-action-disclosure"
                className="command-action-menu"
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.stopPropagation();
                    setSwitcherOpen(false);
                    switcherTriggerRef.current?.focus();
                  }
                }}
              >
                <div className="command-action-options">
                  {groupedSwitcherActions.map((group) => (
                    <div className="command-action-group" key={group.category}>
                      <div className="command-action-group-heading">{group.category}</div>
                      {group.actions.map((action) => {
                        const Icon = actionIcon(action.subcommand);
                        const meta = actionDisplayMeta(action);
                        const descriptor = sentenceCase(`${meta.input} to ${meta.output}`);
                        return (
                          <Button
                            variant="plain"
                            size="unstyled"
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
                              <small>{descriptor}</small>
                            </span>
                            <Kbd unstyled>{actionDisplayMeta(action).method}</Kbd>
                          </Button>
                        );
                      })}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        ) : (
          <Search size={16} strokeWidth={1.65} aria-hidden="true" />
        )}
        <Input
          unstyled
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          onFocus={() => {
            if (modeAction?.subcommand === "ask" && askSessions.length > 0)
              onAskSessionsOpenChange(true);
          }}
          onKeyDown={onCommandInputKeyDown}
          placeholder={
            modeAction
              ? argumentPlaceholder(modeAction)
              : hasQuery
                ? (active?.example ?? "Search commands")
                : "Search or run an operation — scrape, index site, map, ask…"
          }
          className="command-input"
          role="combobox"
          aria-expanded={listboxOpen || askSessionsOpen}
          aria-controls={
            listboxOpen ? "palette-action-list" : askSessionsOpen ? "ask-session-list" : undefined
          }
          aria-activedescendant={
            listboxOpen
              ? activeDescendantId
              : askSessionsOpen
                ? `ask-session-option-${selectedAskSession}`
                : undefined
          }
          aria-autocomplete="list"
          aria-describedby={validation ? validationId : undefined}
          aria-label={modeAction ? `${modeAction.label} argument` : "Axon command"}
        />
        {validation && (
          <span id={validationId} className="sr-only" role="status">
            {validation}
          </span>
        )}
        {askSessionsOpen && askSessions.length > 0 && (
          <AskSessionMenu
            askSessions={askSessions}
            askSessionsRef={askSessionsRef}
            selectedAskSession={selectedAskSession}
            onAskSessionsOpenChange={onAskSessionsOpenChange}
            onResumeAskSession={onResumeAskSession}
            onSelect={setSelectedAskSession}
          />
        )}
      </div>
      <Button
        variant="plain"
        size="unstyled"
        // `.command-submit:disabled` owns the disabled look (no opacity change);
        // neutralize the primitive base's disabled:opacity-45 to stay pixel-identical.
        className={`${active && !validation ? "command-submit command-submit-armed" : "command-submit"} disabled:opacity-100`}
        type="button"
        onClick={() => active && onSubmit(active)}
        disabled={submitDisabled}
        aria-label="Run selected action"
        title={validation || "Run selected action"}
      >
        <Send size={15} />
      </Button>
      <div className="command-menu-wrap" ref={menuRef}>
        <Button
          variant="plain"
          size="unstyled"
          ref={menuTriggerRef}
          className={
            settingsOpen || menuOpen
              ? "command-settings command-settings-active"
              : "command-settings"
          }
          type="button"
          onClick={(event) => {
            event.stopPropagation();
            setSwitcherOpen(false);
            setMenuOpen((open) => !open);
          }}
          onKeyDown={(event) => {
            if (event.key === "Escape" && menuOpen) {
              event.stopPropagation();
              setMenuOpen(false);
              menuTriggerRef.current?.focus();
            }
          }}
          aria-haspopup="true"
          aria-expanded={menuOpen}
          aria-controls="command-menu"
          aria-label="Menu"
          title="Menu"
        >
          <Menu size={15} />
        </Button>
        {menuOpen && (
          <div id="command-menu" className="command-menu">
            <Button
              variant="plain"
              size="unstyled"
              className="command-menu-item"
              type="button"
              onClick={() => {
                setMenuOpen(false);
                onToggleSettings();
              }}
            >
              <Settings size={15} strokeWidth={1.7} aria-hidden="true" />
              <span>
                <strong>Settings</strong>
                <small>Palette preferences</small>
              </span>
            </Button>
            <Button
              variant="plain"
              size="unstyled"
              className="command-menu-item"
              type="button"
              onClick={() => {
                setMenuOpen(false);
                onToggleSettings();
              }}
            >
              <SlidersHorizontal size={15} strokeWidth={1.7} aria-hidden="true" />
              <span>
                <strong>Config</strong>
                <small>config.toml tuning</small>
              </span>
            </Button>
            <Button
              variant="plain"
              size="unstyled"
              className="command-menu-item"
              type="button"
              onClick={() => {
                setMenuOpen(false);
                onToggleSettings();
              }}
            >
              <TerminalSquare size={15} strokeWidth={1.7} aria-hidden="true" />
              <span>
                <strong>Environment</strong>
                <small>.env secrets & URLs</small>
              </span>
            </Button>
            <Button
              variant="plain"
              size="unstyled"
              className="command-menu-item command-menu-item-separated"
              type="button"
              onClick={() => {
                setMenuOpen(false);
                onHelp(active, active ? undefined : query.trim());
              }}
              disabled={running}
            >
              <CircleHelp size={15} strokeWidth={1.7} aria-hidden="true" />
              <span>
                <strong>Help</strong>
                <small>Shortcuts & action docs</small>
              </span>
            </Button>
          </div>
        )}
      </div>
    </section>
  );
}
