import { ChevronRight, Workflow } from "lucide-react";
import type { Dispatch, RefObject, SetStateAction } from "react";

import { ActionList } from "@/components/palette/ActionList";
import { AuthNotice } from "@/components/palette/AuthNotice";
import { BrowserView } from "@/components/palette/BrowserView";
import { type HistoryItem, HistoryPanel } from "@/components/palette/HistoryPanel";
import { JobProgressView } from "@/components/palette/JobProgressView";
import { OutputPanel } from "@/components/palette/OutputPanel";
import { PaletteCommandBar } from "@/components/palette/PaletteCommandBar";
import { PaletteFooter } from "@/components/palette/PaletteFooter";
import { SettingsPanel } from "@/components/palette/SettingsPanel";
import { Button } from "@/components/ui/aurora/button";
import { acceptsDirectUrl, actionMatches, type PaletteAction } from "@/lib/actions";
import type { Client, PaletteConfig } from "@/lib/axonClient";
import { MIN_PROGRESS_PCT } from "@/lib/format";
import { runStateFromHistory } from "@/lib/historyRun";
import { invoke } from "@/lib/invoke";
import { jobFamilyVerb } from "@/lib/jobProgress";
import { looksLikeUrl, type ParsedCommand } from "@/lib/paletteView";
import type { ViewIntent } from "@/lib/paletteViewState";
import type { ChatSuggestion, RunState } from "@/lib/runState";
import type { SourceSortMode } from "@/lib/sourcesModel";
import type { LiveRefreshState } from "@/lib/useLiveRefresh";

interface PaletteShellProps {
  active?: PaletteAction;
  activeDescendantId?: string;
  browserFocusRef: RefObject<HTMLDivElement | null>;
  browserInitialTarget: string | null;
  browserOpen: boolean;
  onCloseBrowser: () => void;
  cancelAsyncJob: () => Promise<void>;
  client: Client | null;
  commandRunning: boolean;
  compact: boolean;
  config: PaletteConfig | null;
  configError: string | null;
  copied: boolean;
  dispatchView: Dispatch<ViewIntent>;
  draftConfig: PaletteConfig | null;
  endpointLabel: string;
  endpointTone: string;
  enterActionMode: (action: PaletteAction) => void;
  expandAsyncJob: () => void;
  filtered: PaletteAction[];
  guardMessage: string;
  hasQuery: boolean;
  askSessions: HistoryItem[];
  askSessionsOpen: boolean;
  hideCommandBar: boolean;
  history: HistoryItem[];
  historyFocusRef: RefObject<HTMLDivElement | null>;
  historyOpen: boolean;
  jobCanceling: boolean;
  jobExpanded: boolean;
  jobMinimized: boolean;
  jobNowMs: number;
  listboxOpen: boolean;
  liveRefresh: LiveRefreshState;
  modeAction: PaletteAction | null;
  onBack: () => void;
  onCollapse: () => void;
  onCopy: (text: string) => void;
  onDrillDomain: (domain: string) => void;
  onFollowUp: (text: string) => void;
  onHistory: () => void;
  onAskSessionsOpenChange: (open: boolean) => void;
  onInputKeyDown: (event: React.KeyboardEvent<HTMLInputElement>) => void;
  onOpenJob: (family: string, jobId: string, label: string) => void;
  onReset: () => void;
  onResumeAskSession: (item: HistoryItem) => void;
  onRetry: () => void;
  onRunAction: (subcommand: string, argument: string) => void;
  onSuggestMessage: (message: string) => Promise<ChatSuggestion[]>;
  onSaveSettings: () => void;
  onSubmitAction: (action: PaletteAction) => void;
  onSwitcherOpenChange: (open: boolean) => void;
  onToggleLivePause: () => void;
  onToggleMaximize: () => void;
  onTogglePin: () => void;
  onToggleSettings: () => void;
  outputFocusRef: RefObject<HTMLDivElement | null>;
  outputKind: "markdown" | "code";
  parsed: ParsedCommand;
  pinned: boolean;
  query: string;
  requestSubmit: (action: PaletteAction, argumentOverride?: string) => void;
  run: RunState;
  selected: number;
  setDraftConfig: Dispatch<SetStateAction<PaletteConfig | null>>;
  setHistory: Dispatch<SetStateAction<HistoryItem[]>>;
  setQuery: Dispatch<SetStateAction<string>>;
  setRun: Dispatch<SetStateAction<RunState>>;
  setSelected: Dispatch<SetStateAction<number>>;
  settingsFocusRef: RefObject<HTMLDivElement | null>;
  settingsOpen: boolean;
  shortcutOptions: readonly string[];
  showActionPanel: boolean;
  showBackButton: boolean;
  showContent: boolean;
  showResultsLayout: boolean;
  sourcesFilter: string;
  sourcesGrouped: boolean;
  sourcesSort: SourceSortMode;
  submitDisabled: boolean;
  switchActionMode: (action: PaletteAction) => void;
  validation: string;
  showHelpFor: (action?: PaletteAction, unknownTarget?: string) => void;
  minimizeAsyncJob: () => void;
  closeAsyncJob: () => void;
  setSourcesFilter: Dispatch<SetStateAction<string>>;
  setSourcesSort: Dispatch<SetStateAction<SourceSortMode>>;
  setSourcesGrouped: Dispatch<SetStateAction<boolean>>;
}

export function PaletteShell(props: PaletteShellProps) {
  function runHighlightedAction() {
    const action = props.filtered[props.selected] ?? props.active;
    if (!action) return;

    if (
      props.parsed.invoked ||
      action.argMode === "none" ||
      (action.subcommand === "ask" &&
        props.parsed.search.trim().length > 0 &&
        !actionMatches(action, props.parsed.search)) ||
      (acceptsDirectUrl(action) && looksLikeUrl(props.parsed.search))
    ) {
      props.requestSubmit(
        action,
        action.subcommand === "ask" &&
          props.parsed.search.trim().length > 0 &&
          !actionMatches(action, props.parsed.search)
          ? props.parsed.search
          : undefined,
      );
      return;
    }

    props.enterActionMode(action);
  }

  function selectHighlightedAction() {
    const action = props.filtered[props.selected] ?? props.active;
    if (!action) return;
    if (props.parsed.invoked && action.argMode === "none") {
      props.requestSubmit(action);
      return;
    }
    props.enterActionMode(action);
  }

  function onShellKeyDownCapture(event: React.KeyboardEvent<HTMLDivElement>) {
    if (!props.listboxOpen || props.submitDisabled || event.defaultPrevented) return;
    if (event.key !== "Tab" && event.key !== "Enter") return;

    const target = event.target instanceof HTMLElement ? event.target : null;
    if (target?.closest("input, textarea, select, [contenteditable='true']")) return;

    event.preventDefault();
    if (event.key === "Enter") runHighlightedAction();
    else selectHighlightedAction();
  }

  return (
    <div
      className={`aurora-page-shell palette-shell${props.compact ? " palette-shell-compact" : ""}${props.showResultsLayout ? " palette-shell-results" : " palette-shell-browse"}${props.jobExpanded ? " palette-shell-job" : ""}`}
      onKeyDownCapture={onShellKeyDownCapture}
    >
      <AuthNotice />
      {!props.hideCommandBar && (
        <PaletteCommandBar
          active={props.active}
          activeDescendantId={props.activeDescendantId}
          config={props.config}
          endpointLabel={props.endpointLabel}
          endpointTone={props.endpointTone}
          hasQuery={props.hasQuery}
          listboxOpen={props.listboxOpen}
          modeAction={props.modeAction}
          query={props.query}
          running={props.commandRunning}
          settingsOpen={props.settingsOpen}
          showBackButton={props.showBackButton}
          submitDisabled={props.submitDisabled}
          validation={props.validation || props.guardMessage}
          askSessions={props.askSessions}
          askSessionsOpen={props.askSessionsOpen}
          onBack={props.onBack}
          onAskSessionsOpenChange={props.onAskSessionsOpenChange}
          onHelp={props.showHelpFor}
          onInputKeyDown={props.onInputKeyDown}
          onQueryChange={props.setQuery}
          onReset={props.onReset}
          onResumeAskSession={props.onResumeAskSession}
          onSubmit={props.onSubmitAction}
          onSwitchAction={props.switchActionMode}
          onSwitcherOpenChange={props.onSwitcherOpenChange}
          onToggleMaximize={props.onToggleMaximize}
          onToggleSettings={props.onToggleSettings}
        />
      )}

      <JobTray {...props} />
      <SettingsRegion {...props} />
      <MainContent {...props} />
      <FooterRegion {...props} />
    </div>
  );
}

function JobTray(props: PaletteShellProps) {
  if (props.jobMinimized && props.run.kind === "asyncJob") {
    return (
      <Button
        variant="plain"
        size="unstyled"
        className="idle-tray"
        type="button"
        onClick={props.expandAsyncJob}
        title="Expand job"
      >
        <Workflow size={14} strokeWidth={1.9} />
        <span>
          {jobFamilyVerb(props.run.family)} {props.run.snapshot.label}
        </span>
        {props.run.snapshot.percent != null && (
          <>
            <span className="idle-tray-bar">
              <span
                style={{
                  width: `${Math.max(MIN_PROGRESS_PCT, Math.round(props.run.snapshot.percent))}%`,
                }}
              />
            </span>
            <strong>{Math.round(props.run.snapshot.percent)}%</strong>
          </>
        )}
        <ChevronRight size={15} />
      </Button>
    );
  }

  return null;
}

function SettingsRegion(props: PaletteShellProps) {
  if (!props.settingsOpen || !props.draftConfig) return null;
  return (
    <div ref={props.settingsFocusRef} style={{ display: "contents" }}>
      <SettingsPanel
        configError={props.configError}
        draftConfig={props.draftConfig}
        shortcutOptions={props.shortcutOptions}
        onChange={props.setDraftConfig}
        onClose={() => props.dispatchView({ type: "closeSettings" })}
        onSave={props.onSaveSettings}
      />
    </div>
  );
}

function MainContent(props: PaletteShellProps) {
  if (!props.showContent || props.settingsOpen) return null;
  if (props.browserOpen) {
    return (
      <main className="palette-grid palette-grid-output-only">
        <div ref={props.browserFocusRef} style={{ display: "contents" }}>
          <BrowserView initialTarget={props.browserInitialTarget} onClose={props.onCloseBrowser} />
        </div>
      </main>
    );
  }
  return (
    <main
      className={
        props.showResultsLayout
          ? props.showActionPanel
            ? "palette-grid"
            : "palette-grid palette-grid-output-only"
          : "palette-suggestions"
      }
    >
      {props.showActionPanel && (
        <ActionList
          filtered={props.filtered}
          selected={props.selected}
          setSelected={props.setSelected}
          parsed={props.parsed}
          onSubmit={props.requestSubmit}
          onEnterMode={props.enterActionMode}
          onHelp={props.showHelpFor}
        />
      )}

      {props.historyOpen && (
        <div ref={props.historyFocusRef} style={{ display: "contents" }}>
          <HistoryPanel
            items={props.history}
            onClear={() => props.setHistory([])}
            onOpen={(item) => {
              props.dispatchView({ type: "openHistoryItem", action: item.action });
              props.setQuery(item.target);
              const historyRun = runStateFromHistory(item);
              if (historyRun) {
                props.setRun(historyRun);
              } else if (item.running) {
                props.setRun({
                  kind: "running",
                  title: `Running ${item.action.label}`,
                  subtitle: item.target,
                });
              }
            }}
          />
        </div>
      )}

      <JobRegion {...props} />
      <OutputRegion {...props} />
    </main>
  );
}

function JobRegion(props: PaletteShellProps) {
  if (!props.showResultsLayout || props.historyOpen) return null;
  if (props.run.kind === "asyncJob") {
    return (
      <div ref={props.outputFocusRef} style={{ display: "contents" }}>
        <JobProgressView
          snapshot={props.run.snapshot}
          nowMs={props.jobNowMs}
          canceling={props.jobCanceling}
          onCancel={() => void props.cancelAsyncJob()}
          onMinimize={props.minimizeAsyncJob}
          onClose={props.closeAsyncJob}
        />
      </div>
    );
  }
  return null;
}

function OutputRegion(props: PaletteShellProps) {
  if (!props.showResultsLayout || props.historyOpen || props.run.kind === "asyncJob") {
    return null;
  }
  return (
    <div ref={props.outputFocusRef} style={{ display: "contents" }}>
      <OutputPanel
        active={props.active}
        copied={props.copied}
        outputKind={props.outputKind}
        run={props.run}
        onCopy={props.onCopy}
        onRetry={props.onRetry}
        onFollowUp={props.onFollowUp}
        onHistory={props.onHistory}
        onCollapse={props.onCollapse}
        onTogglePin={props.onTogglePin}
        pinned={props.pinned}
        agentBubbles={props.config?.agentBubbles ?? false}
        liveRefresh={props.liveRefresh}
        onToggleLivePause={props.onToggleLivePause}
        onOpenJob={props.onOpenJob}
        onRunAction={props.onRunAction}
        onSuggestMessage={props.onSuggestMessage}
        onDrillDomain={props.onDrillDomain}
        sourcesFilter={props.sourcesFilter}
        sourcesSort={props.sourcesSort}
        sourcesGrouped={props.sourcesGrouped}
        onSourcesFilterChange={props.setSourcesFilter}
        onSourcesSortChange={props.setSourcesSort}
        onSourcesGroupedChange={props.setSourcesGrouped}
        client={props.client}
        config={props.config}
      />
    </div>
  );
}

function FooterRegion(props: PaletteShellProps) {
  if (!props.showContent || props.settingsOpen) return null;
  return (
    <PaletteFooter
      config={props.config}
      configError={props.configError}
      onRecent={() => {
        props.setRun({ kind: "idle" });
        props.dispatchView({ type: "toggleHistory" });
      }}
      onSettings={() => props.dispatchView({ type: "toggleSettings" })}
      onHide={() => void invoke("hide_palette")}
    />
  );
}
