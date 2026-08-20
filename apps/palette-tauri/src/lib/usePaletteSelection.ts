import { type Dispatch, type SetStateAction, useMemo, useRef } from "react";

import { actionOptionId } from "@/components/palette/ActionList";
import type { HistoryItem } from "@/components/palette/HistoryPanel";
import {
  actionConfirmationArmed,
  actionConfirmationMessage,
  actionNeedsConfirmation,
  type PendingActionConfirmation,
} from "@/lib/actionGuard";
import { ACTIONS, actionMatches, type PaletteAction } from "@/lib/actions";
import {
  argumentFor,
  looksLikeUrl,
  parseCommand,
  sortActionsByRelevance,
  sortActionsForDisplay,
  validationMessage,
} from "@/lib/paletteView";
import type { RunState } from "@/lib/runState";

interface PaletteSelectionInput {
  actions?: PaletteAction[];
  browseOpen: boolean;
  browserOpen: boolean;
  history: HistoryItem[];
  historyOpen: boolean;
  modeAction: PaletteAction | null;
  mobileRuntime?: boolean;
  pendingConfirmation: PendingActionConfirmation | null;
  query: string;
  run: RunState;
  selected: number;
  setSelected: Dispatch<SetStateAction<number>>;
  settingsOpen: boolean;
}

export function usePaletteSelection(input: PaletteSelectionInput) {
  const {
    actions = ACTIONS,
    browseOpen,
    browserOpen,
    history,
    historyOpen,
    modeAction,
    mobileRuntime = false,
    pendingConfirmation,
    query,
    run,
    selected,
    setSelected,
    settingsOpen,
  } = input;
  const parsed = useMemo(() => parseCommand(query), [query]);
  const invoked = useMemo(
    () =>
      parsed.invoked
        ? actions.find((action) => action.subcommand === parsed.invoked?.subcommand)
        : undefined,
    [actions, parsed.invoked],
  );
  const unavailableInvocation = Boolean(parsed.invoked && !invoked);
  const hasQuery = query.trim().length > 0;
  const filtered = useMemo(() => {
    if (unavailableInvocation) return [];
    if (invoked) return [invoked];
    if (mobileRuntime && parsed.search.trim().length === 0) return sortActionsForDisplay(actions);
    if (looksLikeUrl(parsed.search)) return sortActionsForDisplay(actions).slice(0, 12);
    const matches = actions.filter((action) => actionMatches(action, parsed.search));
    if (parsed.search.trim().length > 0 && matches.length === 0) {
      const ask = actions.find((action) => action.subcommand === "ask");
      return ask ? [ask] : [];
    }
    return sortActionsByRelevance(matches, parsed.search).slice(0, 12);
  }, [actions, invoked, mobileRuntime, parsed.search, unavailableInvocation]);
  const selectionKey = `${parsed.search} ${modeAction?.subcommand ?? ""}`;
  const previousSelectionKey = useRef(selectionKey);
  let selectedIndex = selected;
  if (previousSelectionKey.current !== selectionKey) {
    previousSelectionKey.current = selectionKey;
    selectedIndex = 0;
    if (selected !== 0) setSelected(0);
  }
  selectedIndex = Math.min(selectedIndex, Math.max(filtered.length - 1, 0));
  const suggestedAction = filtered[selectedIndex];
  const slashInvokedAction = query.trimStart().startsWith("/") ? invoked : undefined;
  const active = unavailableInvocation
    ? undefined
    : (slashInvokedAction ?? modeAction ?? suggestedAction);
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
  const validation =
    mobileRuntime && !hasQuery && !modeAction
      ? ""
      : active
        ? validationMessage(active, activeArgument)
        : "No matching action";
  const confirmationArmed =
    active && !validation
      ? actionConfirmationArmed(pendingConfirmation, active, activeArgument)
      : false;
  const guardMessage =
    active && !validation && actionNeedsConfirmation(active)
      ? actionConfirmationMessage(active, Boolean(confirmationArmed))
      : "";
  const canRunLocalAction = active?.kind === "local";
  const jobMinimized = run.kind === "asyncJob" && run.minimized;
  const jobExpanded = run.kind === "asyncJob" && !run.minimized;
  const showOutput = run.kind !== "idle" && !jobMinimized;
  const enteringArgument =
    Boolean(modeAction) && !showOutput && !settingsOpen && !historyOpen && !browserOpen;
  const showContent =
    mobileRuntime ||
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

  return {
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
  };
}
