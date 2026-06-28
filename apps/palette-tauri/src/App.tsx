import { useCallback, useMemo, useReducer, useRef, useState } from "react";

import { actionOptionId } from "@/components/palette/ActionList";
import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { PaletteShell } from "@/components/palette/PaletteShell";
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
import { createAxonClient } from "@/lib/axonClient";
import { outputKindFor } from "@/lib/format";
import { invoke, isTauriRuntime } from "@/lib/invoke";
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
import { summarizeJob } from "@/lib/jobProgress";
import { useActionRunner } from "@/lib/useActionRunner";
import { useCrawlJob } from "@/lib/useCrawlJob";
import { useJobPoll } from "@/lib/useJobPoll";
import { useLiveRefresh } from "@/lib/useLiveRefresh";
import { useFocusReturn, usePaletteHotkeys } from "@/lib/useFocusReturn";
import { usePaletteConfig } from "@/lib/usePaletteConfig";
import { usePaletteLifecycle } from "@/lib/usePaletteLifecycle";
import { usePalettePins } from "@/lib/usePalettePins";
import { useSourcesNavigation } from "@/lib/useSourcesNavigation";
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
  const { config, draftConfig, setDraftConfig, configError, saveSettings } = usePaletteConfig(dispatchView);
  const [history, setHistory] = useState<HistoryItem[]>([]);
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [copied, setCopied] = useState(false);
  const [shownTick, setShownTick] = useState(0);
  const [pendingConfirmation, setPendingConfirmation] = useState<PendingActionConfirmation | null>(null);

  const modeAction = modeOf(view);
  const settingsOpen = isSettingsOpen(view);
  const historyOpen = isHistoryOpen(view);
  const browseOpen = isBrowseOpen(view);
  usePaletteLifecycle(dispatchView, setShownTick);

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

  const parsed = useMemo(() => parseCommand(query), [query]);
  const hasQuery = query.trim().length > 0;
  const filtered = useMemo(() => {
    if (parsed.invoked) return [parsed.invoked];
    if (looksLikeUrl(parsed.search)) {
      return sortActionsForDisplay(ACTIONS).slice(0, 12);
    }
    const matches = ACTIONS.filter((action) => actionMatches(action, parsed.search));
    if (parsed.search.trim().length > 0 && matches.length === 0) {
      const ask = ACTIONS.find((action) => action.subcommand === "ask");
      return ask ? [ask] : [];
    }
    return sortActionsByRelevance(
      matches,
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
  const askFallback = active?.subcommand === "ask" && !modeAction && !parsed.invoked && parsed.search.trim().length > 0 && !actionMatches(active, parsed.search);
  const activeArgument = active ? (askFallback ? parsed.search : argumentFor(active, modeAction, parsed, query)) : "";
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
  const {
    sourcesDrillFilter,
    sourcesFilter,
    sourcesSort,
    sourcesGrouped,
    setSourcesFilter,
    setSourcesSort,
    setSourcesGrouped,
    clearSourcesFilter,
    clearSourcesForAction,
    onRunAction,
    onDrillDomain,
  } = useSourcesNavigation(requestSubmit);

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
    clearSourcesForAction(action);
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
      clearSourcesForAction(action);
      requestSubmit(action, "");
      return;
    }
    setPendingConfirmation(null);
    clearSourcesForAction(action);
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
        requestSubmit(active, askFallback ? parsed.search : undefined);
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
  const { pinnedTargets, togglePin: onTogglePin } = usePalettePins(setHistory, currentTarget);
  const commandRunning = run.kind === "running" || run.kind === "streaming";
  const submitDisabled =
    (!client && !canRunLocalAction) || !active || commandRunning || Boolean(validation);

  function goBackToBrowse() {
    setPendingConfirmation(null);
    dispatchView({ type: "goToBrowse" });
    setRun({ kind: "idle" });
    setQuery("");
    clearSourcesFilter();
    focusInput(true);
  }

  // P-M2 — stable callbacks for the memoized children (CommandBar/OutputPanel).
  const onSubmitAction = useCallback((action: PaletteAction) => requestSubmit(action), [requestSubmit]);
  const onReset = useCallback(() => {
    setQuery("");
    setRun({ kind: "idle" });
    setPendingConfirmation(null);
    clearSourcesFilter();
    dispatchView({ type: "reset" });
  }, [clearSourcesFilter]);
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
  const shellProps = {
    active, activeDescendantId, cancelAsyncJob, cancelJob, canceling, commandRunning, compact, config,
    configError, copied, dispatchView, draftConfig, endpointLabel, endpointTone, enterActionMode,
    expandAsyncJob, expandJob, filtered, guardMessage, hasQuery, history, historyFocusRef, historyOpen,
    jobCanceling, jobExpanded, jobMinimized, jobNowMs, listboxOpen, liveRefresh, modeAction, nowMs,
    onBack: goBackToBrowse, onCollapse, onCopy, onDrillDomain, onFollowUp, onHistory, onInputKeyDown,
    onOpenJob, onReset, onRetry, onRunAction, onSaveSettings: () => {
      setPendingConfirmation(null);
      void saveSettings();
    }, onSubmitAction,
    onToggleLivePause: () => setLivePaused((paused) => !paused), onToggleMaximize, onTogglePin,
    onToggleSettings, outputFocusRef, outputKind, parsed, pinned: currentTarget ? pinnedTargets.has(currentTarget) : false,
    query, requestSubmit, run, selected, setDraftConfig, setHistory, setQuery, setRun, setSelected,
    settingsFocusRef, settingsOpen, shortcutOptions, showActionPanel, showBackButton, showContent,
    showResultsLayout, sourcesFilter: sourcesFilter || sourcesDrillFilter, sourcesGrouped, sourcesSort,
    submitDisabled, switchActionMode, validation, viewPartialJob, showHelpFor, minimizeJob, closeJob,
    minimizeAsyncJob, closeAsyncJob, setSourcesFilter, setSourcesSort, setSourcesGrouped,
  };

  return <PaletteShell {...shellProps} />;

  async function copyOutput(text: string) {
    await navigator.clipboard.writeText(text);
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1200);
  }
}
