import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";

import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { PaletteShell } from "@/components/palette/PaletteShell";
import {
  actionConfirmationArmed,
  actionNeedsConfirmation,
  confirmationFor,
  type PendingActionConfirmation,
} from "@/lib/actionGuard";
import { ACTIONS, actionMatches, type PaletteAction } from "@/lib/actions";
import { currentOutputTarget } from "@/lib/appHelpers";
import { createAxonClient } from "@/lib/axonClient";
import { outputKindFor } from "@/lib/format";
import { runStateFromHistory } from "@/lib/historyRun";
import { invoke, isTauriRuntime } from "@/lib/invoke";
import { loadPaletteHistory, persistPaletteHistory } from "@/lib/paletteHistoryStorage";
import { argumentFor, focusInput, validationMessage } from "@/lib/paletteView";
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
import type { RunState } from "@/lib/runState";
import { hostLabel } from "@/lib/url";
import { useActionRunner } from "@/lib/useActionRunner";
import { useAskHistoryRecorder } from "@/lib/useAskHistoryRecorder";
import { useChatToolRunner } from "@/lib/useChatToolRunner";
import { useFocusReturn, usePaletteHotkeys } from "@/lib/useFocusReturn";
import { useJobPoll } from "@/lib/useJobPoll";
import { useLiveRefresh } from "@/lib/useLiveRefresh";
import { useOpenJob } from "@/lib/useOpenJob";
import { usePaletteConfig } from "@/lib/usePaletteConfig";
import { usePaletteHelp } from "@/lib/usePaletteHelp";
import { usePaletteInputKeyDown } from "@/lib/usePaletteInputKeyDown";
import { usePaletteLifecycle } from "@/lib/usePaletteLifecycle";
import { usePalettePins } from "@/lib/usePalettePins";
import { usePaletteSelection } from "@/lib/usePaletteSelection";
import { useSourcesNavigation } from "@/lib/useSourcesNavigation";
import { useSuggestMessage } from "@/lib/useSuggestMessage";
import { useWindowChrome } from "@/lib/useWindowChrome";

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

  const {
    active,
    activeDescendantId,
    askFallback,
    askSessions,
    canRunLocalAction,
    compact,
    filtered,
    guardMessage,
    hasQuery,
    hideCommandBar,
    jobExpanded,
    jobMinimized,
    listboxOpen,
    parsed,
    showActionPanel,
    showContent,
    showOutput,
    showResultsLayout,
    validation,
  } = usePaletteSelection({
    browseOpen,
    browserOpen,
    history,
    historyOpen,
    modeAction,
    pendingConfirmation,
    query,
    run,
    selected,
    setSelected,
    settingsOpen,
  });

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

  const showHelpFor = usePaletteHelp({ dispatchView, setHistory, setQuery, setRun });
  const onInputKeyDown = usePaletteInputKeyDown({
    active,
    askFallback,
    askSessionsLength: askSessions.length,
    dispatchView,
    enterActionMode,
    filteredLength: filtered.length,
    modeAction,
    parsed,
    requestSubmit,
    setAskSessionsOpen,
    setSelected,
  });

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
  const onSuggestMessage = useSuggestMessage(client, config);
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
      {...{
        active,
        activeDescendantId,
        browserFocusRef,
        browserInitialTarget: browserInitialTargetValue,
        browserOpen,
        cancelAsyncJob,
        client,
        commandRunning,
        compact,
        config,
        configError,
        copied,
        dispatchView,
        draftConfig,
        endpointLabel,
        endpointTone,
        enterActionMode,
        expandAsyncJob,
        filtered,
        guardMessage,
        hasQuery,
        hideCommandBar,
        history,
        historyFocusRef,
        historyOpen,
        askSessions,
        askSessionsOpen,
        jobCanceling,
        jobExpanded,
        jobMinimized,
        jobNowMs,
        listboxOpen,
        liveRefresh,
        modeAction,
        onCloseBrowser,
        onCollapse,
        onCopy,
        onDrillDomain,
        onFollowUp,
        onHistory,
        onInputKeyDown,
        onOpenJob,
        onReset,
        onResumeAskSession,
        onRetry,
        onSubmitAction,
        onSuggestMessage,
        onToggleMaximize,
        onTogglePin,
        onToggleSettings,
        outputFocusRef,
        outputKind,
        parsed,
        query,
        requestSubmit,
        run,
        selected,
        setDraftConfig,
        setHistory,
        setQuery,
        setRun,
        setSelected,
        settingsFocusRef,
        settingsOpen,
        shortcutOptions,
        showActionPanel,
        showBackButton,
        showContent,
        showResultsLayout,
        sourcesGrouped,
        sourcesSort,
        submitDisabled,
        switchActionMode,
        validation,
        showHelpFor,
        minimizeAsyncJob,
        closeAsyncJob,
        setSourcesFilter,
        setSourcesSort,
        setSourcesGrouped,
      }}
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
