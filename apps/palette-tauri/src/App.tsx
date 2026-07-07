import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";

import { actionOptionId } from "@/components/palette/ActionList";
import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { PaletteShell } from "@/components/palette/PaletteShell";
import { ACTIONS, type PaletteAction, type RemotePaletteAction, actionMatches } from "@/lib/actions";
import {
  actionConfirmationArmed,
  actionConfirmationMessage,
  actionNeedsConfirmation,
  confirmationFor,
  type PendingActionConfirmation,
} from "@/lib/actionGuard";
import { buildHelpRun, helpAction } from "@/lib/actionHelp";
import { currentOutputTarget } from "@/lib/appHelpers";
import { createAxonClient, executeAction } from "@/lib/axonClient";
import { outputKindFor } from "@/lib/format";
import { runStateFromHistory } from "@/lib/historyRun";
import { invoke, isTauriRuntime } from "@/lib/invoke";
import { loadPaletteHistory, normalizeChatSuggestions, persistPaletteHistory } from "@/lib/paletteHistoryStorage";
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
  browserInitialTarget,
  INITIAL_VIEW,
  isBrowseOpen,
  isBrowserOpen,
  isHistoryOpen,
  isSettingsOpen,
  modeOf,
  viewReducer,
} from "@/lib/paletteViewState";
import type { ChatSuggestion, RunState } from "@/lib/runState";
import { strField, unwrapPayload } from "@/lib/payload";
import { useAskHistoryRecorder } from "@/lib/useAskHistoryRecorder";
import { capHistory, useActionRunner } from "@/lib/useActionRunner";
import { useChatToolRunner } from "@/lib/useChatToolRunner";
import { useCrawlJob } from "@/lib/useCrawlJob";
import { useJobPoll } from "@/lib/useJobPoll";
import { useLiveRefresh } from "@/lib/useLiveRefresh";
import { useFocusReturn, usePaletteHotkeys } from "@/lib/useFocusReturn";
import { usePaletteConfig } from "@/lib/usePaletteConfig";
import { usePaletteLifecycle } from "@/lib/usePaletteLifecycle";
import { usePalettePins } from "@/lib/usePalettePins";
import { useOpenJob } from "@/lib/useOpenJob";
import { useSourcesNavigation } from "@/lib/useSourcesNavigation";
import { useWindowChrome } from "@/lib/useWindowChrome";
import { hostLabel } from "@/lib/url";

const shortcutOptions = ["Ctrl+Shift+Space", "Alt+Space", "Ctrl+Space", "Cmd+Shift+Space"] as const;

document.documentElement.classList.toggle("tauri-runtime", isTauriRuntime);

export default function App() {
  const [view, dispatchView] = useReducer(viewReducer, INITIAL_VIEW);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const { config, draftConfig, setDraftConfig, configError, saveSettings } =
    usePaletteConfig(dispatchView);
  const [history, setHistory] = useState<HistoryItem[]>(() => loadPaletteHistory());
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const [copied, setCopied] = useState(false);
  const [shownTick, setShownTick] = useState(0);
  const [pendingConfirmation, setPendingConfirmation] = useState<PendingActionConfirmation | null>(
    null,
  );
  const [actionSwitcherOpen, setActionSwitcherOpen] = useState(false);
  const [askSessionsOpen, setAskSessionsOpen] = useState(false);

  const modeAction = modeOf(view);
  const settingsOpen = isSettingsOpen(view);
  const historyOpen = isHistoryOpen(view);
  const browseOpen = isBrowseOpen(view);
  const browserOpen = isBrowserOpen(view);
  const browserInitialTargetValue = browserInitialTarget(view);
  usePaletteLifecycle(dispatchView, setShownTick);

  useEffect(() => {
    persistPaletteHistory(history);
  }, [history]);

  const keyStateRef = useRef({ settingsOpen, historyOpen, browseOpen, query, modeAction, run });
  keyStateRef.current = { settingsOpen, historyOpen, browseOpen, query, modeAction, run };
  const copyOutput = useCallback((text: string) => {
    void navigator.clipboard.writeText(text).then(() => {
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    });
  }, []);
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
    return sortActionsByRelevance(matches, parsed.search).slice(0, 12);
  }, [parsed.invoked, parsed.search]);
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
  const slashInvokedAction = query.trimStart().startsWith("/") ? parsed.invoked : undefined;
  const active = slashInvokedAction ?? modeAction ?? suggestedAction;
  const askSessions = useMemo(
    () =>
      history.filter(
        (item) => item.action.subcommand === "ask" && item.text && (item.prompt || item.target),
      ),
    [history],
  );
  const askFallback =
    active?.subcommand === "ask" &&
    !modeAction &&
    !parsed.invoked &&
    parsed.search.trim().length > 0 &&
    !actionMatches(active, parsed.search);
  const activeArgument = active
    ? askFallback
      ? parsed.search
      : argumentFor(active, slashInvokedAction ? null : modeAction, parsed, query)
    : "";
  const validation = active ? validationMessage(active, activeArgument) : "No matching action";
  const confirmationArmed =
    active && !validation
      ? actionConfirmationArmed(pendingConfirmation, active, activeArgument)
      : false;
  const guardMessage =
    active && !validation && actionNeedsConfirmation(active)
      ? actionConfirmationMessage(active, Boolean(confirmationArmed))
      : "";
  const canRunLocalAction = active?.kind === "local";
  const jobMinimized = (run.kind === "job" || run.kind === "asyncJob") && run.minimized;
  const jobExpanded = (run.kind === "job" || run.kind === "asyncJob") && !run.minimized;
  const showOutput = run.kind !== "idle" && !jobMinimized;
  const enteringArgument =
    Boolean(modeAction) && !showOutput && !settingsOpen && !historyOpen && !browserOpen;
  const showContent =
    settingsOpen ||
    historyOpen ||
    browserOpen ||
    showOutput ||
    (!enteringArgument && (hasQuery || browseOpen));
  const compact = !showContent;
  const showResultsLayout = showOutput || settingsOpen || historyOpen || browserOpen;
  const hideCommandBar =
    browserOpen || (showOutput && (active?.subcommand === "ask" || active?.subcommand === "chat"));
  const showActionPanel =
    !showResultsLayout && !settingsOpen && !historyOpen && !browserOpen && !enteringArgument;
  const listboxOpen = showContent && showActionPanel;
  const activeDescendantId =
    listboxOpen && suggestedAction ? actionOptionId(suggestedAction) : undefined;

  const settingsFocusRef = useFocusReturn<HTMLDivElement>(settingsOpen);
  const historyFocusRef = useFocusReturn<HTMLDivElement>(historyOpen && !settingsOpen);
  const browserFocusRef = useFocusReturn<HTMLDivElement>(browserOpen);
  const outputFocusRef = useFocusReturn<HTMLDivElement>(
    showOutput && !settingsOpen && !historyOpen,
  );

  useWindowChrome({
    actionSwitcherOpen,
    jobExpanded,
    jobMinimized,
    settingsOpen,
    historyOpen: historyOpen || browserOpen,
    showResultsLayout,
    showContent,
    filteredLength: filtered.length,
    shownTick,
  });

  const client = useMemo(() => (config ? createAxonClient(config) : null), [config]);

  useEffect(() => {
    if (modeAction?.subcommand !== "ask") setAskSessionsOpen(false);
  }, [modeAction?.subcommand]);

  useAskHistoryRecorder({ active, run, setHistory });

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
      // Browser is a local, window-driven action: it never issues an HTTP
      // request (unlike other `kind: "local"` actions such as `help`, which
      // `useActionRunner.submit` special-cases into a synthetic RunState), so
      // it is intercepted here and routed straight to its own overlay
      // instead of falling into `submit()`'s generic `kind === "local"`
      // no-op path.
      if (action.subcommand === "browser") {
        setPendingConfirmation(null);
        dispatchView({ type: "openBrowser", initialTarget: argument.trim() || null });
        return;
      }
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

  const onMinimizeJob = useCallback(() => {
    dispatchView({ type: "minimizeJob" });
    setQuery("");
  }, []);
  const onExpandJob = useCallback(() => dispatchView({ type: "expandJob" }), []);
  const onCloseJob = useCallback(() => {
    dispatchView({ type: "closeJob" });
    setQuery("");
  }, []);

  const { nowMs, canceling, cancelJob, viewPartialJob, minimizeJob, expandJob, closeJob } =
    useCrawlJob({
      run,
      setRun,
      onMinimizeJob,
      onExpandJob,
      onCloseJob,
    });

  const {
    nowMs: jobNowMs,
    canceling: jobCanceling,
    cancelJob: cancelAsyncJob,
    minimizeJob: minimizeAsyncJob,
    expandJob: expandAsyncJob,
    closeJob: closeAsyncJob,
  } = useJobPoll({ run, setRun, onMinimizeJob, onExpandJob, onCloseJob });

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

  const onOpenJob = useOpenJob(setRun);

  function enterActionMode(action: PaletteAction) {
    setPendingConfirmation(null);
    clearSourcesForAction(action);
    dispatchView({ type: "enterMode", action });
    setQuery(
      parsed.invoked?.subcommand === action.subcommand
        ? parsed.arg
        : action.subcommand === "ask" &&
            parsed.search.trim().length > 0 &&
            !actionMatches(action, parsed.search)
          ? parsed.search
          : "",
    );
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
    setHistory((items) => capHistory([historyItem, ...items]));
  }

  function onInputKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (modeAction?.subcommand === "ask" && askSessions.length > 0) {
        setAskSessionsOpen(true);
        return;
      }
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
      if (
        !modeAction &&
        !parsed.invoked &&
        active.argMode !== "none" &&
        !looksLikeUrl(parsed.search)
      ) {
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

  const outputKind =
    "outputKind" in run ? run.outputKind : active ? outputKindFor(active.subcommand) : "code";
  const endpointLabel = config
    ? hostLabel(config.serverUrl)
    : configError
      ? "Config error"
      : "Loading";
  const endpointTone = configError ? "error" : "syncing";
  const showBackButton = settingsOpen || historyOpen || browserOpen || showOutput;
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

  const onCloseBrowser = useCallback(() => {
    dispatchView({ type: "closeBrowser" });
    setQuery("");
    focusInput(true);
  }, []);

  // P-M2 — stable callbacks for the memoized children (CommandBar/OutputPanel).
  const onSubmitAction = useCallback(
    (action: PaletteAction) => requestSubmit(action),
    [requestSubmit],
  );
  const onReset = useCallback(() => {
    setQuery("");
    setRun({ kind: "idle" });
    setPendingConfirmation(null);
    clearSourcesFilter();
    dispatchView({ type: "reset" });
  }, [clearSourcesFilter]);
  const onToggleSettings = useCallback(() => dispatchView({ type: "toggleSettings" }), []);
  const onToggleMaximize = useCallback(() => void invoke("toggle_maximize"), []);
  const onCopy = copyOutput;
  const onRetry = useCallback(() => active && void submit(active), [active, submit]);
  const onFollowUp = useCallback(
    (text: string) => {
      const conversationAction =
        active?.subcommand === "chat"
          ? active
          : ACTIONS.find((action) => action.subcommand === "ask");
      if (!conversationAction) return;
      dispatchView({ type: "enterModeForRun", action: conversationAction });
      setQuery(text);
      void submit(conversationAction, text);
    },
    [active, submit],
  );
  const onConversationRunAction = useChatToolRunner({
    active,
    client,
    config,
    run,
    setRun,
    onFallbackRunAction: onRunAction,
  });
  const onSuggestMessage = useCallback(
    async (message: string): Promise<ChatSuggestion[]> => {
      if (!client || !config) throw new Error("Axon is not connected.");
      const queryAction = ACTIONS.find(
        (action): action is RemotePaletteAction =>
          action.subcommand === "query" && action.kind !== "local",
      );
      if (!queryAction) throw new Error("Query action is unavailable.");
      const result = await executeAction(client, queryAction, message, config);
      if (!result.ok) {
        const payload = unwrapPayload(result.payload);
        throw new Error(
          strField(payload, "message") ??
            strField(payload, "error") ??
            strField(payload, "detail") ??
            `Query failed with HTTP ${result.status}`,
        );
      }
      return normalizeChatSuggestions(result.payload);
    },
    [client, config],
  );
  const onHistory = useCallback(() => {
    setRun({ kind: "idle" });
    dispatchView({ type: "openHistory" });
  }, []);
  const onResumeAskSession = useCallback((item: HistoryItem) => {
    setPendingConfirmation(null);
    setAskSessionsOpen(false);
    dispatchView({ type: "openHistoryItem", action: item.action });
    setQuery(item.prompt ?? item.target);
    const historyRun = runStateFromHistory(item);
    setRun(historyRun ?? { kind: "idle" });
  }, []);
  const onCollapse = useCallback(() => {
    setRun({ kind: "idle" });
    setQuery("");
    dispatchView({ type: "collapse" });
  }, []);
  return (
    <PaletteShell
      {...{ active, activeDescendantId, browserFocusRef, browserInitialTarget: browserInitialTargetValue, browserOpen, cancelAsyncJob, cancelJob, canceling, client, commandRunning, compact, config, configError, copied, dispatchView, draftConfig, endpointLabel, endpointTone, enterActionMode, expandAsyncJob, expandJob, filtered, guardMessage, hasQuery, hideCommandBar, history, historyFocusRef, historyOpen, askSessions, askSessionsOpen, jobCanceling, jobExpanded, jobMinimized, jobNowMs, listboxOpen, liveRefresh, modeAction, nowMs, onCloseBrowser, onCollapse, onCopy, onDrillDomain, onFollowUp, onHistory, onInputKeyDown, onOpenJob, onReset, onResumeAskSession, onRetry, onSubmitAction, onSuggestMessage, onToggleMaximize, onTogglePin, onToggleSettings, outputFocusRef, outputKind, parsed, query, requestSubmit, run, selected, setDraftConfig, setHistory, setQuery, setRun, setSelected, settingsFocusRef, settingsOpen, shortcutOptions, showActionPanel, showBackButton, showContent, showResultsLayout, sourcesGrouped, sourcesSort, submitDisabled, switchActionMode, validation, viewPartialJob, showHelpFor, minimizeJob, closeJob, minimizeAsyncJob, closeAsyncJob, setSourcesFilter, setSourcesSort, setSourcesGrouped }}
      onBack={goBackToBrowse}
      onAskSessionsOpenChange={setAskSessionsOpen}
      onRunAction={onConversationRunAction}
      onSaveSettings={() => {
        setPendingConfirmation(null);
        void saveSettings();
      }}
      onSwitcherOpenChange={setActionSwitcherOpen}
      onToggleLivePause={() => setLivePaused((paused) => !paused)}
      pinned={currentTarget ? pinnedTargets.has(currentTarget) : false}
      sourcesFilter={sourcesFilter || sourcesDrillFilter}
    />
  );
}
