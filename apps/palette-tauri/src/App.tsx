import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react";
import type { HistoryItem } from "@/components/palette/HistoryPanel";
import { PaletteShell } from "@/components/palette/PaletteShell";
import { ProductWorkspaceFrame } from "@/components/palette/ProductWorkspaceFrame";
import {
  actionConfirmationArmed,
  actionNeedsConfirmation,
  confirmationFor,
  type PendingActionConfirmation,
} from "@/lib/actionGuard";
import { actionMatches, type PaletteAction } from "@/lib/actions";
import { currentOutputTarget } from "@/lib/appHelpers";
import { createAxonClient } from "@/lib/axonClient";
import { useProductWorkspace } from "@/lib/backendProfiles/useProductWorkspace";
import {
  androidRuntime,
  initialWorkspace,
  mobileRuntime,
  runtimeActions,
  shortcutOptions,
} from "@/lib/runtimeUi";
import type { BackendProduct } from "@/lib/backendProfiles/model";
import { outputKindFor } from "@/lib/format";
import { runStateFromHistory } from "@/lib/historyRun";
import { invoke } from "@/lib/invoke";
import { loadPaletteHistory, persistPaletteHistory } from "@/lib/paletteHistoryStorage";
import { argumentFor, focusInput, validationMessage } from "@/lib/paletteView";
import {
  browserInitialTarget,
  INITIAL_VIEW,
  isBrowseOpen,
  isBrowserOpen,
  isCodexOpen,
  isHistoryOpen,
  isSettingsOpen,
  modeOf,
  viewReducer,
} from "@/lib/paletteViewState";
import type { RunState } from "@/lib/runState";
import { hostLabel } from "@/lib/url";
import { useActionRunner } from "@/lib/useActionRunner";
import { useAndroidBackButton } from "@/lib/useAndroidBackButton";
import { useAskHistoryRecorder } from "@/lib/useAskHistoryRecorder";
import { useChatToolRunner } from "@/lib/useChatToolRunner";
import { handlePaletteBack, useFocusReturn, usePaletteHotkeys } from "@/lib/useFocusReturn";
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

export default function App() {
  const [workspace, setWorkspace] = useState<BackendProduct>(initialWorkspace);
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
  const codexOpen = isCodexOpen(view);
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
  const paletteBackActions = {
    closeSettings: () => dispatchView({ type: "closeSettings" as const }),
    toBrowseFromHistory: () => dispatchView({ type: "closeHistoryToBrowse" as const }),
    closeBrowse: () => dispatchView({ type: "closeBrowse" as const }),
    clearMode: () => dispatchView({ type: "clearMode" as const }),
    clearQuery: () => {
      setQuery("");
      dispatchView({ type: "clearMode" });
    },
    closeRoot: () => void invoke("hide_palette"),
    copyOutput: (text: string) => void copyOutput(text),
  };
  usePaletteHotkeys(keyStateRef, paletteBackActions);
  useAndroidBackButton(androidRuntime, () => {
    let handled = true;
    handlePaletteBack(keyStateRef.current, {
      ...paletteBackActions,
      closeRoot: () => {
        handled = false;
      },
    });
    return handled;
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
    actions: runtimeActions,
    browseOpen,
    browserOpen,
    history,
    historyOpen,
    modeAction,
    mobileRuntime,
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
    historyOpen,
    browserOpen,
    showResultsLayout: showResultsLayout || workspace !== "axon",
    showContent,
    filteredLength: filtered.length,
    shownTick,
  });

  const client = useMemo(() => (config ? createAxonClient(config) : null), [config]);
  const invalidateProductState = useCallback(() => {
    setPendingConfirmation(null); setRun({ kind: "idle" }); setQuery("");
  }, []);
  const { availableProducts, cortexProfile, labbyProfile, selectBackendProfile, selectWorkspace } =
    useProductWorkspace(config, setDraftConfig, invalidateProductState, setWorkspace);

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
    actions: runtimeActions,
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
      // Keep the argument validated above as the authoritative snapshot. If we
      // ask the runner to derive it again from render state, a fast first Enter
      // can observe the previous query and only the second Enter will execute.
      void submit(action, argument);
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

  const showHelpFor = usePaletteHelp({
    actions: runtimeActions,
    dispatchView,
    setHistory,
    setQuery,
    setRun,
  });
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
  const showBackButton = settingsOpen || historyOpen || browserOpen || codexOpen || showOutput;
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
  const onQueryChange = useCallback(
    (value: string) => {
      // Typing over any completed/running result starts a fresh command search.
      // Keeping an argument-taking action mode alive here made the new text run
      // against stale action state, unlike no-input actions which already reset.
      if (run.kind !== "idle") {
        setRun({ kind: "idle" });
        setPendingConfirmation(null);
        dispatchView({ type: "goToBrowse" });
      }
      setQuery(value);
    },
    [run.kind],
  );
  const onCopy = copyOutput;
  const onRetry = useCallback(() => active && void submit(active), [active, submit]);
  const onFollowUp = useCallback(
    (text: string) => {
      const conversationAction =
        active?.subcommand === "chat"
          ? active
          : runtimeActions.find((action) => action.subcommand === "ask");
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
    <ProductWorkspaceFrame
      workspace={workspace}
      profiles={config?.backendProfiles ?? []}
      activeProfileIds={config?.activeBackendProfiles ?? {}}
      available={availableProducts}
      labbyProfile={labbyProfile}
      cortexProfile={cortexProfile}
      onSelect={selectWorkspace}
      onSelectProfile={(product, id) => void selectBackendProfile(product, id)}
    >
      <PaletteShell
        {...{
          active,
          actions: runtimeActions,
          activeDescendantId,
          browserFocusRef,
          browserInitialTarget: browserInitialTargetValue,
          browserOpen,
          codexOpen,
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
          mobileRuntime,
          onCloseBrowser,
          onCloseCodex: () => dispatchView({ type: "closeCodex" }),
          onCollapse,
          onCopy,
          onDrillDomain,
          onFollowUp,
          onHistory,
          onInputKeyDown,
          onOpenJob,
          onQueryChange,
          onReset,
          onResumeAskSession,
          onRetry,
          onSubmitAction,
          onSuggestMessage,
          onToggleMaximize,
          onTogglePin,
          onToggleSettings,
          onToggleCodex: () => dispatchView({ type: "openCodex" }),
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
    </ProductWorkspaceFrame>
  );
}
