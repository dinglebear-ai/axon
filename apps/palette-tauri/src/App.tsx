import {
  ChevronRight,
  Workflow,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";

import { ActionList, actionOptionId } from "@/components/palette/ActionList";
import { AuthNotice } from "@/components/palette/AuthNotice";
import { CrawlJobView } from "@/components/palette/CrawlJobView";
import { JobProgressView } from "@/components/palette/JobProgressView";
import { HistoryPanel, type HistoryItem } from "@/components/palette/HistoryPanel";
import { OutputPanel } from "@/components/palette/OutputPanel";
import { PaletteCommandBar } from "@/components/palette/PaletteCommandBar";
import { PaletteFooter } from "@/components/palette/PaletteFooter";
import { SettingsPanel } from "@/components/palette/SettingsPanel";
import { Button } from "@/components/ui/aurora/button";
import {
  ACTIONS,
  type PaletteAction,
  actionMatches,
} from "@/lib/actions";
import {
  actionConfirmationArmed,
  actionConfirmationMessage,
  actionNeedsConfirmation,
  confirmationFor,
  type PendingActionConfirmation,
} from "@/lib/actionGuard";
import { buildHelpRun, helpAction } from "@/lib/actionHelp";
import { currentOutputTarget } from "@/lib/appHelpers";
import { type PaletteConfig, createAxonClient } from "@/lib/axonClient";
import { MIN_PROGRESS_PCT, outputKindFor } from "@/lib/format";
import { runStateFromHistory } from "@/lib/historyRun";
import { appWindow, invoke, isTauriRuntime } from "@/lib/invoke";
import {
  argumentFor,
  focusInput,
  looksLikeUrl,
  parseCommand,
  sortActionsByRelevance,
  sortActionsForDisplay,
  validationMessage,
} from "@/lib/paletteView";
import {
  INITIAL_VIEW,
  isBrowseOpen,
  isHistoryOpen,
  isSettingsOpen,
  modeOf,
  viewReducer,
} from "@/lib/paletteViewState";
import type { RunState } from "@/lib/runState";
import { hostFromUrl, summarizeCrawl } from "@/lib/crawlJob";
import { jobFamilyVerb, summarizeJob } from "@/lib/jobProgress";
import { useActionRunner } from "@/lib/useActionRunner";
import { useCrawlJob } from "@/lib/useCrawlJob";
import { useJobPoll } from "@/lib/useJobPoll";
import { useLiveRefresh } from "@/lib/useLiveRefresh";
import { useFocusReturn, usePaletteHotkeys } from "@/lib/useFocusReturn";
import { useWindowChrome } from "@/lib/useWindowChrome";
import { hostLabel } from "@/lib/url";

const shortcutOptions = ["Ctrl+Shift+Space", "Alt+Space", "Ctrl+Space", "Cmd+Shift+Space"] as const;
document.documentElement.classList.toggle("tauri-runtime", isTauriRuntime);

export default function App() {
  // A-M1 — the top-level view is a single discriminated union driven by a reducer;
  // the legacy settingsOpen/browseOpen/historyOpen/modeAction flags derive from it below.
  const [view, dispatchView] = useReducer(viewReducer, INITIAL_VIEW);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [config, setConfig] = useState<PaletteConfig | null>(null);
  const [draftConfig, setDraftConfig] = useState<PaletteConfig | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [pinnedTargets, setPinnedTargets] = useState<Set<string>>(() => new Set());
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [copied, setCopied] = useState(false);
  const [shownTick, setShownTick] = useState(0);
  const [pendingConfirmation, setPendingConfirmation] = useState<PendingActionConfirmation | null>(null);

  const modeAction = modeOf(view);
  const settingsOpen = isSettingsOpen(view);
  const historyOpen = isHistoryOpen(view);
  const browseOpen = isBrowseOpen(view);

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
      appWindow.listen("palette://open-settings", () => dispatchView({ type: "openSettings" })),
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

  // R-M1/H3/P-H2 — ref-for-latest-value; the keydown listener binds once.
  const keyStateRef = useRef({ settingsOpen, historyOpen, browseOpen, query, modeAction, run });
  keyStateRef.current = { settingsOpen, historyOpen, browseOpen, query, modeAction, run };
  usePaletteHotkeys(keyStateRef, {
    closeSettings: () => dispatchView({ type: "closeSettings" }),
    toBrowseFromHistory: () => dispatchView({ type: "closeHistoryToBrowse" }),
    closeBrowse: () => dispatchView({ type: "closeBrowse" }),
    clearMode: () => dispatchView({ type: "clearMode" }),
    clearQuery: () => {
      setQuery("");
      dispatchView({ type: "clearMode" });
    },
    copyOutput: (text) => void copyOutput(text),
  });

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
  // R-M3 — reset + clamp selection during render (no reset-on-prop-change effect).
  const selectionKey = `${parsed.search} ${modeAction?.subcommand ?? ""}`;
  const prevSelectionKeyRef = useRef(selectionKey);
  let selectedIndex = selected;
  if (prevSelectionKeyRef.current !== selectionKey) {
    prevSelectionKeyRef.current = selectionKey;
    selectedIndex = 0;
    if (selected !== 0) setSelected(0);
  }
  selectedIndex = Math.min(selectedIndex, Math.max(filtered.length - 1, 0));
  const suggestedAction = filtered[selectedIndex];
  const active = modeAction ?? suggestedAction;
  const activeArgument = active ? argumentFor(active, modeAction, parsed, query) : "";
  const validation = active ? validationMessage(active, activeArgument) : "No matching action";
  const confirmationArmed =
    active && !validation ? actionConfirmationArmed(pendingConfirmation, active, activeArgument) : false;
  const guardMessage =
    active && !validation && actionNeedsConfirmation(active)
      ? actionConfirmationMessage(active, Boolean(confirmationArmed))
      : "";
  const canRunLocalAction = active?.kind === "local";
  const jobMinimized = (run.kind === "job" || run.kind === "asyncJob") && run.minimized;
  const jobExpanded = (run.kind === "job" || run.kind === "asyncJob") && !run.minimized;
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
  // A11Y-C1 — the listbox is only mounted when the action panel is shown AND there
  // is content to show. The combobox must only reference ids that exist in the DOM,
  // so gate aria-controls/aria-activedescendant on the listbox actually rendering.
  const listboxOpen = showContent && showActionPanel;
  const activeDescendantId =
    listboxOpen && suggestedAction ? actionOptionId(suggestedAction) : undefined;

  // A11Y-H2 — focus into overlays on open, restore on close. Wrappers use
  // `display: contents` so they stay transparent to the grid layout.
  const settingsFocusRef = useFocusReturn<HTMLDivElement>(settingsOpen);
  const historyFocusRef = useFocusReturn<HTMLDivElement>(historyOpen && !settingsOpen);
  const outputFocusRef = useFocusReturn<HTMLDivElement>(showOutput && !settingsOpen && !historyOpen);

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

  // A-M2 — the runner dispatches view intents + resets orthogonal query/run via
  // these stable callbacks instead of 5 raw setters; the view rules live in the reducer.
  const enterModeForRun = useCallback((action: PaletteAction, argument: string) => {
    dispatchView({ type: "enterModeForRun", action });
    setQuery(argument);
  }, []);
  const showHelpRun = useCallback((action: PaletteAction, target: string) => {
    dispatchView({ type: "showHelp", action });
    setQuery(target);
  }, []);

  const { submit } = useActionRunner({
    client,
    config,
    run,
    setRun,
    setHistory,
    enterModeForRun,
    showHelpRun,
    modeAction,
    parsed,
    query,
  });

  const requestSubmit = useCallback(
    (action: PaletteAction, argumentOverride?: string) => {
      const argument = argumentOverride ?? argumentFor(action, modeAction, parsed, query);
      const validationMessageText = validationMessage(action, argument);
      if (!validationMessageText && actionNeedsConfirmation(action)) {
        if (!actionConfirmationArmed(pendingConfirmation, action, argument)) {
          setPendingConfirmation(confirmationFor(action, argument));
          focusInput(true);
          return;
        }
        setPendingConfirmation(null);
      } else if (pendingConfirmation) {
        setPendingConfirmation(null);
      }
      void submit(action, argumentOverride);
    },
    [modeAction, parsed, pendingConfirmation, query, submit],
  );

  // A-M2 — useCrawlJob's 6 setters collapse to 3 view intents + the query reset
  // each implies. setRun stays (it owns the live poll snapshot, which is run state).
  const onMinimizeJob = useCallback(() => {
    dispatchView({ type: "minimizeJob" });
    setQuery("");
  }, []);
  const onExpandJob = useCallback(() => dispatchView({ type: "expandJob" }), []);
  const onCloseJob = useCallback(() => {
    dispatchView({ type: "closeJob" });
    setQuery("");
  }, []);

  const { nowMs, canceling, cancelJob, viewPartialJob, minimizeJob, expandJob, closeJob } = useCrawlJob({
    run,
    setRun,
    onMinimizeJob,
    onExpandJob,
    onCloseJob,
  });

  // Sibling lifecycle for embed/extract/ingest. The two hooks are mutually
  // exclusive at runtime — each only acts when `run.kind` matches its variant —
  // and reuse the same view-intent callbacks (family-agnostic).
  const {
    nowMs: jobNowMs,
    canceling: jobCanceling,
    cancelJob: cancelAsyncJob,
    minimizeJob: minimizeAsyncJob,
    expandJob: expandAsyncJob,
    closeJob: closeAsyncJob,
  } = useJobPoll({ run, setRun, onMinimizeJob, onExpandJob, onCloseJob });

  // Live-refresh for the zero-input dynamic views (stats/status): re-poll the
  // result endpoint on an interval while it is open, with a pause toggle.
  const [livePaused, setLivePaused] = useState(false);
  const liveRefresh = useLiveRefresh({ run, setRun, paused: livePaused });

  // Run a palette action against a single argument (e.g. SourcesView "retrieve"
  // row action). Routes through the same confirmation/validation path as Enter.
  const onRunAction = useCallback(
    (subcommand: string, argument: string) => {
      const action = ACTIONS.find((a) => a.subcommand === subcommand);
      if (action) requestSubmit(action, argument);
    },
    [requestSubmit],
  );

  // Drill from a domain (DomainsView) into the sources list pre-filtered to that
  // domain. `sourcesDrillFilter` seeds SourcesView's filter; cleared on reset.
  const [sourcesDrillFilter, setSourcesDrillFilter] = useState("");
  const onDrillDomain = useCallback(
    (domain: string) => {
      setSourcesDrillFilter(domain);
      const action = ACTIONS.find((a) => a.subcommand === "sources");
      if (action) requestSubmit(action, "");
    },
    [requestSubmit],
  );

  // Open the live job card for a job listed in StatusView. Crawl gets the rich
  // crawl view; embed/extract/ingest get the generic async-job card. The poll
  // hooks (useCrawlJob/useJobPoll) take over once the run state is set.
  const onOpenJob = useCallback((family: string, jobId: string, label: string) => {
    const startedAtMs = Date.now();
    if (family === "crawl") {
      setRun({
        kind: "job",
        family: "crawl",
        title: `Crawling ${hostFromUrl(label)}`,
        subtitle: `job ${jobId}`,
        jobId,
        statusUrl: `/v1/crawl/${jobId}`,
        url: label,
        startedAtMs,
        maxPages: 0,
        maxDepth: 0,
        snapshot: summarizeCrawl({ job: { status: "running" } }, { jobId, url: label }),
        minimized: false,
      });
    } else if (family === "embed" || family === "extract" || family === "ingest") {
      setRun({
        kind: "asyncJob",
        family,
        title: `${family[0].toUpperCase()}${family.slice(1)}`,
        subtitle: `job ${jobId}`,
        jobId,
        statusUrl: `/v1/${family}/${jobId}`,
        target: label,
        startedAtMs,
        snapshot: summarizeJob(family, { job: { status: "running" } }, { jobId, label }),
        minimized: false,
      });
    }
  }, []);

  function enterActionMode(action: PaletteAction) {
    setPendingConfirmation(null);
    dispatchView({ type: "enterMode", action });
    setQuery(parsed.invoked?.subcommand === action.subcommand ? parsed.arg : "");
    setSelected(0);
    setRun({ kind: "idle" });
    focusInput(true);
  }

  function shouldAutoRunOnSwitch(action: PaletteAction) {
    return action.argMode === "none" && action.autoRunOnSwitch === true;
  }

  function switchActionMode(action: PaletteAction) {
    if (shouldAutoRunOnSwitch(action)) {
      setQuery("");
      setSelected(0);
      setRun({ kind: "idle" });
      requestSubmit(action, "");
      return;
    }
    setPendingConfirmation(null);
    dispatchView({ type: "switchMode", action });
    setSelected(0);
    setRun({ kind: "idle" });
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
    dispatchView({ type: "showHelp", action: localHelpAction });
    setQuery(action?.subcommand ?? cleanUnknownTarget ?? "");
    setRun(helpRun);
    setHistory((items) => [historyItem, ...items].slice(0, 18));
  }

  async function saveSettings() {
    if (!draftConfig) return;
    try {
      const nextConfig = await invoke<PaletteConfig>("save_palette_settings", { settings: draftConfig });
      setConfig(nextConfig);
      setDraftConfig(nextConfig);
      setConfigError(null);
      setPendingConfirmation(null);
      dispatchView({ type: "closeSettings" });
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
      if (!modeAction) dispatchView({ type: "openBrowse" });
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
        requestSubmit(active);
      }
    } else if (event.key === "Tab") {
      event.preventDefault();
      if (!active) return;
      // No-input actions run immediately rather than entering an empty arg mode.
      if (active.argMode === "none") requestSubmit(active);
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
    setPendingConfirmation(null);
    dispatchView({ type: "goToBrowse" });
    setRun({ kind: "idle" });
    setQuery("");
    setSourcesDrillFilter("");
    focusInput(true);
  }

  // P-M2 — stable callbacks for the memoized children (CommandBar/OutputPanel).
  const onSubmitAction = useCallback((action: PaletteAction) => requestSubmit(action), [requestSubmit]);
  const onReset = useCallback(() => {
    setQuery("");
    setRun({ kind: "idle" });
    setPendingConfirmation(null);
    setSourcesDrillFilter("");
    dispatchView({ type: "reset" });
  }, []);
  const onToggleSettings = useCallback(() => dispatchView({ type: "toggleSettings" }), []);
  const onToggleMaximize = useCallback(() => void invoke("toggle_maximize"), []);
  const onCopy = useCallback((text: string) => void copyOutput(text), [copyOutput]);
  const onRetry = useCallback(() => active && void submit(active), [active, submit]);
  const onFollowUp = useCallback((text: string) => {
    const askAction = ACTIONS.find((action) => action.subcommand === "ask");
    if (!askAction) return;
    dispatchView({ type: "enterModeForRun", action: askAction });
    setQuery(text);
    void submit(askAction, text);
  }, [submit]);
  const onHistory = useCallback(() => {
    setRun({ kind: "idle" });
    dispatchView({ type: "openHistory" });
  }, []);
  const onCollapse = useCallback(() => {
    setRun({ kind: "idle" });
    setQuery("");
    dispatchView({ type: "collapse" });
  }, []);
  const onTogglePin = useCallback(() => {
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
  }, [currentTarget, pinnedTargets]);

  return (
    <div className={`aurora-page-shell palette-shell${compact ? " palette-shell-compact" : ""}${showResultsLayout ? " palette-shell-results" : " palette-shell-browse"}${jobExpanded ? " palette-shell-job" : ""}`}>
      <AuthNotice />
      <PaletteCommandBar
        active={active}
        activeDescendantId={activeDescendantId}
        config={config}
        endpointLabel={endpointLabel}
        endpointTone={endpointTone}
        hasQuery={hasQuery}
        listboxOpen={listboxOpen}
        modeAction={modeAction}
        query={query}
        running={commandRunning}
        settingsOpen={settingsOpen}
        showBackButton={showBackButton}
        submitDisabled={submitDisabled}
        validation={validation || guardMessage}
        onBack={goBackToBrowse}
        onHelp={showHelpFor}
        onInputKeyDown={onInputKeyDown}
        onQueryChange={setQuery}
        onReset={onReset}
        onSubmit={onSubmitAction}
        onSwitchAction={switchActionMode}
        onToggleMaximize={onToggleMaximize}
        onToggleSettings={onToggleSettings}
      />

      {jobMinimized && run.kind === "job" && (
        <Button variant="plain" size="unstyled" className="idle-tray" type="button" onClick={expandJob} title="Expand crawl job">
          <span className="idle-tray-dot" />
          <Workflow size={14} strokeWidth={1.9} />
          <span>Crawling {run.snapshot.host}</span>
          <span className="idle-tray-bar">
            <span style={{ width: `${Math.max(MIN_PROGRESS_PCT, Math.round(run.snapshot.percent))}%` }} />
          </span>
          <strong>{Math.round(run.snapshot.percent)}%</strong>
          <ChevronRight size={15} />
        </Button>
      )}

      {jobMinimized && run.kind === "asyncJob" && (
        <Button variant="plain" size="unstyled" className="idle-tray" type="button" onClick={expandAsyncJob} title="Expand job">
          <span className="idle-tray-dot" />
          <Workflow size={14} strokeWidth={1.9} />
          <span>
            {jobFamilyVerb(run.family)} {run.snapshot.label}
          </span>
          {run.snapshot.percent != null && (
            <>
              <span className="idle-tray-bar">
                <span style={{ width: `${Math.max(MIN_PROGRESS_PCT, Math.round(run.snapshot.percent))}%` }} />
              </span>
              <strong>{Math.round(run.snapshot.percent)}%</strong>
            </>
          )}
          <ChevronRight size={15} />
        </Button>
      )}

      {settingsOpen && draftConfig && (
        <div ref={settingsFocusRef} style={{ display: "contents" }}>
          <SettingsPanel
            configError={configError}
            draftConfig={draftConfig}
            shortcutOptions={shortcutOptions}
            onChange={setDraftConfig}
            onClose={() => dispatchView({ type: "closeSettings" })}
            onSave={() => void saveSettings()}
          />
        </div>
      )}

      {showContent && !settingsOpen && (
      <main className={showResultsLayout ? (showActionPanel ? "palette-grid" : "palette-grid palette-grid-output-only") : "palette-suggestions"}>
        {showActionPanel && (
          <ActionList
            filtered={filtered}
            selected={selected}
            setSelected={setSelected}
            parsed={parsed}
            onSubmit={requestSubmit}
            onEnterMode={enterActionMode}
            onHelp={showHelpFor}
          />
        )}

        {historyOpen && (
          <div ref={historyFocusRef} style={{ display: "contents" }}>
          <HistoryPanel
            items={history}
            onClear={() => setHistory([])}
            onOpen={(item) => {
              dispatchView({ type: "openHistoryItem", action: item.action });
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
          </div>
        )}

        {showResultsLayout && !historyOpen && run.kind === "job" && (
          <div ref={outputFocusRef} style={{ display: "contents" }}>
          <CrawlJobView
            snapshot={run.snapshot}
            nowMs={nowMs}
            canceling={canceling}
            onCancel={() => void cancelJob()}
            onViewPartial={() => void viewPartialJob()}
            onMinimize={minimizeJob}
            onClose={closeJob}
          />
          </div>
        )}

        {showResultsLayout && !historyOpen && run.kind === "asyncJob" && (
          <div ref={outputFocusRef} style={{ display: "contents" }}>
          <JobProgressView
            snapshot={run.snapshot}
            nowMs={jobNowMs}
            canceling={jobCanceling}
            onCancel={() => void cancelAsyncJob()}
            onMinimize={minimizeAsyncJob}
            onClose={closeAsyncJob}
          />
          </div>
        )}

        {showResultsLayout && !historyOpen && run.kind !== "job" && run.kind !== "asyncJob" && (
          <div ref={outputFocusRef} style={{ display: "contents" }}>
          <OutputPanel
            active={active}
            copied={copied}
            outputKind={outputKind}
            run={run}
            onCopy={onCopy}
            onRetry={onRetry}
            onFollowUp={onFollowUp}
            onHistory={onHistory}
            onCollapse={onCollapse}
            onTogglePin={onTogglePin}
            pinned={currentTarget ? pinnedTargets.has(currentTarget) : false}
            liveRefresh={liveRefresh}
            onToggleLivePause={() => setLivePaused((paused) => !paused)}
            onOpenJob={onOpenJob}
            onRunAction={onRunAction}
            onDrillDomain={onDrillDomain}
            sourcesInitialFilter={sourcesDrillFilter}
          />
          </div>
        )}
      </main>
      )}

      {showContent && !settingsOpen && (
        <PaletteFooter
          config={config}
          configError={configError}
          onRecent={() => {
            setRun({ kind: "idle" });
            dispatchView({ type: "toggleHistory" });
          }}
          onSettings={() => dispatchView({ type: "toggleSettings" })}
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
