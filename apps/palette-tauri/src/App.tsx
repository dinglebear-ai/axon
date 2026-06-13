import {
  ChevronRight,
  Workflow,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";

import { ActionList } from "@/components/palette/ActionList";
import { CrawlJobView } from "@/components/palette/CrawlJobView";
import { HistoryPanel, type HistoryItem } from "@/components/palette/HistoryPanel";
import { OutputPanel } from "@/components/palette/OutputPanel";
import { PaletteCommandBar } from "@/components/palette/PaletteCommandBar";
import { PaletteFooter } from "@/components/palette/PaletteFooter";
import { SettingsPanel } from "@/components/palette/SettingsPanel";
import {
  ACTIONS,
  type PaletteAction,
  actionMatches,
} from "@/lib/actions";
import { buildHelpRun, helpAction } from "@/lib/actionHelp";
import { currentOutputTarget } from "@/lib/appHelpers";
import { type PaletteConfig, createAxonClient } from "@/lib/axonClient";
import { outputKindFor } from "@/lib/format";
import { runStateFromHistory } from "@/lib/historyRun";
import { appWindow, invoke, isTauriRuntime } from "@/lib/invoke";
import {
  argumentFor,
  focusInput,
  hostLabel,
  looksLikeUrl,
  parseCommand,
  sortActionsByRelevance,
  sortActionsForDisplay,
  validationMessage,
} from "@/lib/paletteView";
import type { RunState } from "@/lib/runState";
import { useActionRunner } from "@/lib/useActionRunner";
import { useCrawlJob } from "@/lib/useCrawlJob";
import { useWindowChrome } from "@/lib/useWindowChrome";

const shortcutOptions = ["Ctrl+Shift+Space", "Alt+Space", "Ctrl+Space", "Cmd+Shift+Space"] as const;
document.documentElement.classList.toggle("tauri-runtime", isTauriRuntime);

export default function App() {
  const [query, setQuery] = useState("");
  const [modeAction, setModeAction] = useState<PaletteAction | null>(null);
  const [selected, setSelected] = useState(0);
  const [config, setConfig] = useState<PaletteConfig | null>(null);
  const [draftConfig, setDraftConfig] = useState<PaletteConfig | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [browseOpen, setBrowseOpen] = useState(false);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [pinnedTargets, setPinnedTargets] = useState<Set<string>>(() => new Set());
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [copied, setCopied] = useState(false);
  const [shownTick, setShownTick] = useState(0);

  useEffect(() => {
    invoke<PaletteConfig>("load_palette_config")
      .then((nextConfig) => {
        setConfig(nextConfig);
        setDraftConfig(nextConfig);
      })
      .catch((err) => {
        setConfigError(String(err));
        void invoke<PaletteConfig>("load_palette_default_config")
          .then((fallbackConfig) => {
            setConfig(fallbackConfig);
            setDraftConfig(fallbackConfig);
          })
          .catch(() => {
            setConfig(null);
            setDraftConfig(null);
          });
      });
  }, []);

  useEffect(() => {
    const unlisteners = [
      appWindow.listen("palette://shown", () => {
        setShownTick((tick) => tick + 1);
        focusInput(true);
      }),
      appWindow.listen("palette://open-settings", () => setSettingsOpen(true)),
    ];
    return () => {
      void Promise.all(unlisteners).then((items) => items.forEach((unlisten) => unlisten()));
    };
  }, []);

  useEffect(() => {
    const onBlur = () => void invoke("hide_palette");
    window.addEventListener("blur", onBlur);
    return () => window.removeEventListener("blur", onBlur);
  }, []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const modifier = event.metaKey || event.ctrlKey;
      if (event.key === "Escape") {
        event.preventDefault();
        if (settingsOpen) {
          setSettingsOpen(false);
        } else if (historyOpen) {
          setHistoryOpen(false);
          setBrowseOpen(true);
        } else if (browseOpen && !query && !modeAction && run.kind === "idle") {
          setBrowseOpen(false);
        } else if (modeAction && !query) {
          setModeAction(null);
        } else if (query) {
          setQuery("");
          setModeAction(null);
        } else {
          void invoke("hide_palette");
        }
      } else if (modifier && event.key.toLowerCase() === "l") {
        event.preventDefault();
        focusInput(true);
      } else if (modifier && event.key.toLowerCase() === "k") {
        event.preventDefault();
        void invoke("show_palette").then(() => focusInput(true));
      } else if (modifier && event.key.toLowerCase() === "c" && "text" in run) {
        const target = event.target as HTMLElement | null;
        if (target?.tagName !== "INPUT" && target?.tagName !== "TEXTAREA") {
          event.preventDefault();
          void copyOutput(run.text);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [browseOpen, historyOpen, modeAction, query, run, settingsOpen]);

  useEffect(() => {
    if (!config) return;
    const root = document.documentElement;
    const media = window.matchMedia("(prefers-color-scheme: light)");
    const applyTheme = () => {
      const useLight = config.theme === "light" || (config.theme === "system" && media.matches);
      root.classList.toggle("light", useLight);
      root.classList.toggle("dark", !useLight);
    };
    applyTheme();
    media.addEventListener("change", applyTheme);
    return () => media.removeEventListener("change", applyTheme);
  }, [config]);

  const parsed = useMemo(() => parseCommand(query), [query]);
  const hasQuery = query.trim().length > 0;
  const filtered = useMemo(() => {
    if (parsed.invoked) return [parsed.invoked];
    if (looksLikeUrl(parsed.search)) {
      return sortActionsForDisplay(ACTIONS).slice(0, 12);
    }
    return sortActionsByRelevance(
      ACTIONS.filter((action) => actionMatches(action, parsed.search)),
      parsed.search,
    ).slice(0, 12);
  }, [parsed.invoked, parsed.search]);
  const suggestedAction = filtered[Math.min(selected, Math.max(filtered.length - 1, 0))];
  const active = modeAction ?? suggestedAction;
  const activeArgument = active ? argumentFor(active, modeAction, parsed, query) : "";
  const validation = active ? validationMessage(active, activeArgument) : "No matching action";
  const canRunLocalAction = active?.kind === "local";
  const jobMinimized = run.kind === "job" && run.minimized;
  const jobExpanded = run.kind === "job" && !run.minimized;
  const showOutput = run.kind !== "idle" && !jobMinimized;
  // Once an action mode is picked, the input collects that action's argument —
  // the palette should NOT keep listing other actions. Stay compact (just the
  // command bar + mode pill) until the run produces output.
  const enteringArgument = Boolean(modeAction) && !showOutput && !settingsOpen && !historyOpen;
  const showContent =
    settingsOpen || historyOpen || showOutput || (!enteringArgument && (hasQuery || browseOpen));
  const compact = !showContent;
  const showResultsLayout = showOutput || settingsOpen || historyOpen;
  const showActionPanel = !showResultsLayout && !settingsOpen && !historyOpen && !enteringArgument;

  useEffect(() => {
    setSelected(0);
  }, [parsed.search, modeAction]);

  useWindowChrome({
    jobExpanded,
    jobMinimized,
    settingsOpen,
    historyOpen,
    showResultsLayout,
    showContent,
    filteredLength: filtered.length,
    shownTick,
  });

  const client = useMemo(() => (config ? createAxonClient(config) : null), [config]);

  const { submit } = useActionRunner({
    client,
    config,
    run,
    setRun,
    setHistory,
    setModeAction,
    setQuery,
    setBrowseOpen,
    modeAction,
    parsed,
    query,
  });

  const { nowMs, canceling, cancelJob, viewPartialJob, minimizeJob, expandJob, closeJob } = useCrawlJob({
    run,
    setRun,
    setSettingsOpen,
    setHistoryOpen,
    setBrowseOpen,
    setQuery,
    setModeAction,
  });

  function enterActionMode(action: PaletteAction) {
    setModeAction(action);
    setQuery(parsed.invoked?.subcommand === action.subcommand ? parsed.arg : "");
    setSelected(0);
    setRun({ kind: "idle" });
    setBrowseOpen(false);
    focusInput(true);
  }

  function switchActionMode(action: PaletteAction) {
    setModeAction(action);
    if (action.argMode === "none") setQuery("");
    setSelected(0);
    setRun({ kind: "idle" });
    setBrowseOpen(false);
    focusInput(true);
  }

  function showHelpFor(action?: PaletteAction, unknownTarget?: string) {
    const cleanUnknownTarget = !action && unknownTarget?.trim() ? unknownTarget.trim() : undefined;
    const helpRun = buildHelpRun(action, cleanUnknownTarget);
    const localHelpAction = helpAction();
    const historyItem: HistoryItem = {
      action: localHelpAction,
      target: action?.subcommand ?? cleanUnknownTarget ?? "catalog",
      status: helpRun.result.status,
      title: helpRun.title,
      subtitle: helpRun.subtitle,
      text: helpRun.text,
      outputKind: "markdown",
      result: helpRun.result,
      when: "just now",
    };
    setModeAction(localHelpAction);
    setQuery(action?.subcommand ?? cleanUnknownTarget ?? "");
    setRun(helpRun);
    setHistory((items) => [historyItem, ...items].slice(0, 18));
    setHistoryOpen(false);
    setSettingsOpen(false);
    setBrowseOpen(false);
  }

  async function saveSettings() {
    if (!draftConfig) return;
    try {
      const nextConfig = await invoke<PaletteConfig>("save_palette_settings", { settings: draftConfig });
      setConfig(nextConfig);
      setDraftConfig(nextConfig);
      setConfigError(null);
      setSettingsOpen(false);
      setBrowseOpen(true);
      focusInput(true);
    } catch (err) {
      setConfigError(String(err));
    }
  }

  function onInputKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      // Arrow-down is the keyboard affordance to browse all actions without
      // typing (focus alone no longer expands the palette).
      if (!modeAction) setBrowseOpen(true);
      setSelected((idx) => Math.min(idx + 1, Math.max(filtered.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((idx) => Math.max(idx - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (!active) return;
      if (!modeAction && !parsed.invoked && active.argMode !== "none" && !looksLikeUrl(parsed.search)) {
        enterActionMode(active);
      } else {
        void submit(active);
      }
    } else if (event.key === "Tab") {
      event.preventDefault();
      if (!active) return;
      // No-input actions run immediately rather than entering an empty arg mode.
      if (active.argMode === "none") void submit(active);
      else enterActionMode(active);
    }
  }

  const outputKind = "outputKind" in run ? run.outputKind : active ? outputKindFor(active.subcommand) : "code";
  const endpointLabel = config ? hostLabel(config.serverUrl) : configError ? "Config error" : "Loading";
  const endpointTone = configError ? "error" : "syncing";
  const showBackButton = settingsOpen || historyOpen || showOutput;
  const currentTarget = currentOutputTarget(run, active, query);
  const commandRunning = run.kind === "running" || run.kind === "streaming";
  const submitDisabled =
    (!client && !canRunLocalAction) || !active || commandRunning || Boolean(validation);

  function goBackToBrowse() {
    setSettingsOpen(false);
    setHistoryOpen(false);
    setRun({ kind: "idle" });
    setModeAction(null);
    setQuery("");
    setBrowseOpen(true);
    focusInput(true);
  }

  return (
    <div className={`aurora-page-shell palette-shell${compact ? " palette-shell-compact" : ""}${showResultsLayout ? " palette-shell-results" : " palette-shell-browse"}${jobExpanded ? " palette-shell-job" : ""}`}>

      <PaletteCommandBar
        active={active}
        config={config}
        endpointLabel={endpointLabel}
        endpointTone={endpointTone}
        hasQuery={hasQuery}
        modeAction={modeAction}
        query={query}
        running={commandRunning}
        settingsOpen={settingsOpen}
        showBackButton={showBackButton}
        submitDisabled={submitDisabled}
        validation={validation}
        onBack={goBackToBrowse}
        onHelp={showHelpFor}
        onInputKeyDown={onInputKeyDown}
        onQueryChange={setQuery}
        onReset={() => {
          setQuery("");
          setModeAction(null);
          setRun({ kind: "idle" });
          setHistoryOpen(false);
          setBrowseOpen(false);
        }}
        onSubmit={(action) => void submit(action)}
        onSwitchAction={switchActionMode}
        onToggleMaximize={() => void invoke("toggle_maximize")}
        onToggleSettings={() => setSettingsOpen((open) => {
          const next = !open;
          setHistoryOpen(false);
          if (!next) setBrowseOpen(true);
          return next;
        })}
      />

      {jobMinimized && run.kind === "job" && (
        <button className="idle-tray" type="button" onClick={expandJob} title="Expand crawl job">
          <span className="idle-tray-dot" />
          <Workflow size={14} strokeWidth={1.9} />
          <span>Crawling {run.snapshot.host}</span>
          <span className="idle-tray-bar">
            <span style={{ width: `${Math.max(2, Math.round(run.snapshot.percent))}%` }} />
          </span>
          <strong>{Math.round(run.snapshot.percent)}%</strong>
          <ChevronRight size={15} />
        </button>
      )}

      {settingsOpen && draftConfig && (
        <SettingsPanel
          configError={configError}
          draftConfig={draftConfig}
          shortcutOptions={shortcutOptions}
          onChange={setDraftConfig}
          onClose={() => {
            setSettingsOpen(false);
            setHistoryOpen(false);
            setBrowseOpen(true);
          }}
          onSave={() => void saveSettings()}
        />
      )}

      {showContent && !settingsOpen && (
      <main className={showResultsLayout ? (showActionPanel ? "palette-grid" : "palette-grid palette-grid-output-only") : "palette-suggestions"}>
        {showActionPanel && (
          <ActionList
            filtered={filtered}
            selected={selected}
            setSelected={setSelected}
            parsed={parsed}
            onSubmit={(action) => void submit(action)}
            onEnterMode={enterActionMode}
            onHelp={showHelpFor}
          />
        )}

        {historyOpen && (
          <HistoryPanel
            items={history}
            onClear={() => setHistory([])}
            onOpen={(item) => {
              setHistoryOpen(false);
              setSettingsOpen(false);
              setModeAction(item.action);
              setQuery(item.target);
              const historyRun = runStateFromHistory(item);
              if (historyRun) {
                setRun(historyRun);
              } else if (item.running) {
                setRun({
                  kind: "running",
                  title: `Running ${item.action.label}`,
                  subtitle: item.target,
                });
              }
            }}
          />
        )}

        {showResultsLayout && !historyOpen && run.kind === "job" && (
          <CrawlJobView
            snapshot={run.snapshot}
            nowMs={nowMs}
            canceling={canceling}
            onCancel={() => void cancelJob()}
            onViewPartial={() => void viewPartialJob()}
            onMinimize={minimizeJob}
            onClose={closeJob}
          />
        )}

        {showResultsLayout && !historyOpen && run.kind !== "job" && (
          <OutputPanel
            active={active}
            copied={copied}
            outputKind={outputKind}
            run={run}
            onCopy={(text) => void copyOutput(text)}
            onRetry={() => active && void submit(active)}
            onFollowUp={(text) => {
              const askAction = ACTIONS.find((action) => action.subcommand === "ask");
              if (!askAction) return;
              setModeAction(askAction);
              setQuery(text);
              void submit(askAction, text);
            }}
            onHistory={() => {
              setRun({ kind: "idle" });
              setSettingsOpen(false);
              setHistoryOpen(true);
              setBrowseOpen(false);
            }}
            onCollapse={() => {
              setRun({ kind: "idle" });
              setModeAction(null);
              setQuery("");
              setHistoryOpen(false);
              setBrowseOpen(false);
            }}
            onTogglePin={() => {
              if (!currentTarget) return;
              setPinnedTargets((items) => {
                const next = new Set(items);
                next.has(currentTarget) ? next.delete(currentTarget) : next.add(currentTarget);
                return next;
              });
              setHistory((items) => items.map((item) => item.target === currentTarget ? { ...item, pinned: !pinnedTargets.has(currentTarget) } : item));
            }}
            pinned={currentTarget ? pinnedTargets.has(currentTarget) : false}
          />
        )}
      </main>
      )}

      {showContent && !settingsOpen && (
        <PaletteFooter
          config={config}
          configError={configError}
          onRecent={() => {
            setRun({ kind: "idle" });
            setModeAction(null);
            setHistoryOpen((open) => !open);
            setBrowseOpen(false);
          }}
          onSettings={() => setSettingsOpen((open) => !open)}
          onHide={() => void invoke("hide_palette")}
        />
      )}
    </div>
  );

  async function copyOutput(text: string) {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }
}
