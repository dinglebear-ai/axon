import {
  Activity,
  ArrowLeft,
  Check,
  Copy,
  ExternalLink,
  History,
  MoreHorizontal,
  Pin,
  RotateCw,
  X,
  type LucideIcon,
} from "lucide-react";
import { memo, useMemo } from "react";

import { AskConversation } from "@/components/palette/AskConversation";
import { ErrorResultView } from "@/components/palette/ErrorResultView";
import { EvaluateView } from "@/components/palette/EvaluateView";
import { MarkdownBody } from "@/components/palette/MarkdownBody";
import { OutputLiveBadge } from "@/components/palette/OutputLiveBadge";
import {
  hasStructuredOperationView,
  OperationResultView,
} from "@/components/palette/OperationResultView";
import { arrayByKeys } from "@/components/palette/OperationResultViewShared";
import { DoctorView } from "@/components/palette/DoctorView";
import { DomainsView } from "@/components/palette/DomainsView";
import { SourcesView } from "@/components/palette/SourcesView";
import { StatsView } from "@/components/palette/StatsView";
import { StatusView, type OpenJobHandler } from "@/components/palette/StatusView";
import { Button } from "@/components/ui/aurora/button";
import { Spinner } from "@/components/ui/aurora/spinner";
import { actionBehavior } from "@/lib/actionRegistry";
import type { PaletteAction } from "@/lib/actions";
import { numField, strField, unwrapPayload } from "@/lib/payload";
import type { ChatSuggestion, RunState } from "@/lib/runState";
import { buildSourcesModel, type SourceSortMode } from "@/lib/sourcesModel";
import type { LiveRefreshState } from "@/lib/useLiveRefresh";
import { firstUrl, hostLabel } from "@/lib/url";

interface OutputPanelProps {
  active?: PaletteAction;
  copied: boolean;
  outputKind: "markdown" | "code";
  run: RunState;
  onCopy: (text: string) => void;
  onRetry: () => void;
  onFollowUp: (text: string) => void;
  onHistory: () => void;
  onCollapse: () => void;
  onTogglePin: () => void;
  pinned: boolean;
  agentBubbles?: boolean;
  liveRefresh?: LiveRefreshState;
  onToggleLivePause?: () => void;
  onOpenJob?: OpenJobHandler;
  onRunAction?: (subcommand: string, argument: string) => void;
  onSuggestMessage?: (message: string) => Promise<ChatSuggestion[]>;
  onDrillDomain?: (domain: string) => void;
  sourcesFilter?: string;
  sourcesSort?: SourceSortMode;
  sourcesGrouped?: boolean;
  onSourcesFilterChange?: (filter: string) => void;
  onSourcesSortChange?: (sort: SourceSortMode) => void;
  onSourcesGroupedChange?: (grouped: boolean) => void;
}

export const OutputPanel = memo(function OutputPanel({
  active,
  copied,
  outputKind,
  run,
  onCopy,
  onRetry,
  onFollowUp,
  onHistory,
  onCollapse,
  onTogglePin,
  pinned,
  agentBubbles = false,
  liveRefresh,
  onToggleLivePause,
  onOpenJob,
  onRunAction,
  onSuggestMessage,
  onDrillDomain,
  sourcesFilter = "",
  sourcesSort = "chunks",
  sourcesGrouped = false,
  onSourcesFilterChange = () => {},
  onSourcesSortChange = () => {},
  onSourcesGroupedChange = () => {},
}: OutputPanelProps) {
  const runText = "text" in run ? run.text : "";
  // P-M1: the URL regex scans the whole growing buffer; without memoization it ran
  // O(n) per stream token → O(n²) over a stream. Keyed on the text so it only
  // recomputes when the buffer actually changes.
  const outputUrl = useMemo(() => (runText ? firstUrl(runText) : null), [runText]);
  const Icon = active ? outputIcon(active.subcommand) : Activity;
  const status = statusFor(run);
  const conversationMode = active?.subcommand === "ask" || active?.subcommand === "chat";
  const transcript = "transcript" in run ? run.transcript : undefined;
  const quietConversationChrome = conversationMode && run.kind !== "running";
  // P-M1: recomputes the scrape/retrieve reading-header metrics only when the run or
  // action changes, not on every unrelated parent re-render / stream token.
  const headerSummary = useMemo(() => readingHeaderSummary(run, active), [run, active]);
  const sourcesModel = useMemo(
    () =>
      run.kind === "success" && active?.subcommand === "sources"
        ? buildSourcesModel(run.result.payload, sourcesFilter, sourcesSort, sourcesGrouped)
        : null,
    [run, active, sourcesFilter, sourcesSort, sourcesGrouped],
  );

  return (
    <section
      className={
        quietConversationChrome ? "output-panel output-panel-conversation" : "output-panel"
      }
    >
      {/* A11Y-C2: terse, polite announcement of run-state transitions for screen
          readers — NOT the per-token streaming text (which would be a firehose). */}
      <span className="sr-only" aria-live="polite">
        {liveStatusMessage(run, active)}
      </span>
      <div
        className={`output-state output-${run.kind} output-tone-${active?.tone ?? "neutral"}${quietConversationChrome ? " output-conversation" : ""}`}
      >
        <header className={headerSummary ? "output-header output-header-summary" : "output-header"}>
          {quietConversationChrome ? (
            <Button
              variant="plain"
              size="unstyled"
              className="output-conversation-back"
              type="button"
              onClick={onCollapse}
              aria-label="Back"
              title="Back"
            >
              <ArrowLeft size={15} strokeWidth={1.85} />
            </Button>
          ) : null}
          {quietConversationChrome ? null : (
            <span className="output-op-tile" aria-hidden="true">
              <Icon size={19} strokeWidth={1.65} />
            </span>
          )}
          <div className="output-meta-info">
            <span className="output-title-line">
              <span className="output-title">
                {headerSummary?.title ??
                  (quietConversationChrome && "prompt" in run && run.prompt
                    ? run.prompt
                    : outputTitle(run))}
              </span>
              {headerSummary ? (
                <span className="output-summary-chips">
                  {headerSummary.metrics.map(([label, value]) => (
                    <span key={label}>
                      <strong>{value}</strong>
                      {label}
                    </span>
                  ))}
                </span>
              ) : null}
            </span>
            {quietConversationChrome ? null : (
              <span className="output-subtitle">{outputSubtitle(run, active)}</span>
            )}
          </div>
          {liveRefresh?.active ? (
            <OutputLiveBadge
              state={liveRefresh}
              onTogglePause={onToggleLivePause}
              onRefreshNow={liveRefresh.refreshNow}
            />
          ) : null}
          {quietConversationChrome ? null : (
            <span className={`output-status output-status-${status.tone}`}>{status.label}</span>
          )}
          <span className="output-tools">
            {run.kind === "running" || run.kind === "streaming" ? (
              <>
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  onClick={onHistory}
                  title="History"
                  aria-label="Open history"
                  data-tooltip="History"
                >
                  <History size={13} />
                </Button>
                {quietConversationChrome ? null : (
                  <Button
                    variant="plain"
                    size="unstyled"
                    type="button"
                    onClick={onCollapse}
                    title="Collapse"
                    aria-label="Collapse output"
                    data-tooltip="Collapse"
                  >
                    <X size={13} />
                  </Button>
                )}
              </>
            ) : quietConversationChrome ? (
              <details className="output-tool-menu">
                {/* biome-ignore lint/a11y/useSemanticElements: summary is the native disclosure control; role keeps test/AT semantics stable. */}
                <summary
                  role="button"
                  title="More actions"
                  aria-label="More actions"
                  data-tooltip="More"
                >
                  <MoreHorizontal size={13} />
                </summary>
                <div>
                  {"text" in run && (
                    <Button
                      variant="plain"
                      size="unstyled"
                      type="button"
                      onClick={() => onCopy(run.text)}
                    >
                      <Copy size={13} />
                      <span>Copy</span>
                    </Button>
                  )}
                  <Button variant="plain" size="unstyled" type="button" onClick={onHistory}>
                    <History size={13} />
                    <span>History</span>
                  </Button>
                </div>
              </details>
            ) : (
              <>
                {"text" in run && (
                  <Button
                    variant="plain"
                    size="unstyled"
                    type="button"
                    className={copied ? "output-tool-copied" : undefined}
                    onClick={() => onCopy(run.text)}
                    title={copied ? "Copied" : "Copy"}
                    aria-label={copied ? "Copied output" : "Copy output"}
                    data-tooltip={copied ? "Copied" : "Copy"}
                  >
                    {copied ? <Check size={13} /> : <Copy size={13} />}
                  </Button>
                )}
                <details className="output-tool-menu">
                  {/* biome-ignore lint/a11y/useSemanticElements: summary is the native disclosure control; role keeps test/AT semantics stable. */}
                  <summary
                    role="button"
                    title="More actions"
                    aria-label="More actions"
                    data-tooltip="More"
                  >
                    <MoreHorizontal size={13} />
                  </summary>
                  <div>
                    <Button variant="plain" size="unstyled" type="button" onClick={onRetry}>
                      <RotateCw size={13} />
                      <span>Re-run</span>
                    </Button>
                    {outputUrl && (
                      <Button
                        variant="plain"
                        size="unstyled"
                        type="button"
                        onClick={() => window.open(outputUrl, "_blank", "noopener,noreferrer")}
                      >
                        <ExternalLink size={13} />
                        <span>Open source</span>
                      </Button>
                    )}
                    <Button
                      variant="plain"
                      size="unstyled"
                      type="button"
                      className={pinned ? "output-tool-active" : undefined}
                      onClick={onTogglePin}
                    >
                      <Pin size={13} />
                      <span>{pinned ? "Unpin" : "Pin"}</span>
                    </Button>
                    <Button variant="plain" size="unstyled" type="button" onClick={onHistory}>
                      <History size={13} />
                      <span>History</span>
                    </Button>
                  </div>
                </details>
                <Button
                  variant="plain"
                  size="unstyled"
                  type="button"
                  onClick={onCollapse}
                  title="Collapse"
                  aria-label="Collapse output"
                  data-tooltip="Collapse"
                >
                  <X size={13} />
                </Button>
              </>
            )}
            {run.kind === "running" || run.kind === "streaming" ? <Spinner size="sm" /> : null}
          </span>
        </header>
        {(run.kind === "streaming" || run.kind === "running") &&
        conversationMode &&
        transcript?.length ? (
          <AskConversation
            prompt={run.prompt ?? ""}
            answer={"text" in run ? run.text : ""}
            transcript={transcript}
            pending
            onFollowUp={onFollowUp}
            onRunAction={onRunAction}
            suggestionsEnabled={active?.subcommand === "chat"}
            onSuggestMessage={onSuggestMessage}
            agentBubbles={agentBubbles}
          />
        ) : run.kind === "streaming" && conversationMode ? (
          <AskConversation
            prompt={run.prompt ?? ""}
            answer={run.text}
            pending
            onFollowUp={onFollowUp}
            onRunAction={onRunAction}
            suggestionsEnabled={active?.subcommand === "chat"}
            onSuggestMessage={onSuggestMessage}
            agentBubbles={agentBubbles}
          />
        ) : run.kind === "running" || run.kind === "streaming" ? (
          <PendingBody run={run} />
        ) : run.kind === "success" && active?.subcommand === "evaluate" ? (
          <EvaluateView payload={run.result.payload} />
        ) : run.kind === "success" && active?.subcommand === "stats" ? (
          <StatsView payload={run.result.payload} />
        ) : run.kind === "success" && active?.subcommand === "status" ? (
          <StatusView payload={run.result.payload} onOpenJob={onOpenJob} />
        ) : run.kind === "success" && active?.subcommand === "doctor" ? (
          <DoctorView payload={run.result.payload} />
        ) : run.kind === "success" && active?.subcommand === "sources" ? (
          sourcesModel ? (
            <SourcesView
              model={sourcesModel}
              onRunAction={onRunAction}
              filter={sourcesFilter}
              sort={sourcesSort}
              grouped={sourcesGrouped}
              onFilterChange={onSourcesFilterChange}
              onSortChange={onSourcesSortChange}
              onGroupedChange={onSourcesGroupedChange}
            />
          ) : null
        ) : run.kind === "success" && active?.subcommand === "domains" ? (
          <DomainsView payload={run.result.payload} onDrillDomain={onDrillDomain} />
        ) : run.kind === "success" && active && hasStructuredOperationView(active.subcommand) ? (
          <OperationResultView
            payload={run.result.payload}
            subcommand={active.subcommand}
            fallbackText={"text" in run ? run.text : ""}
          />
        ) : run.kind === "error" ? (
          <ErrorResultView result={run.result} text={run.text} />
        ) : "text" in run && conversationMode ? (
          <AskConversation
            prompt={run.prompt ?? ""}
            answer={run.text}
            transcript={transcript}
            onFollowUp={onFollowUp}
            onRunAction={onRunAction}
            suggestionsEnabled={active?.subcommand === "chat"}
            onSuggestMessage={onSuggestMessage}
            agentBubbles={agentBubbles}
          />
        ) : "text" in run ? (
          outputKind === "markdown" ? (
            <div className="output-body output-markdown">
              <MarkdownBody>{run.text}</MarkdownBody>
            </div>
          ) : (
            <pre className="output-body output-code">
              <code>{run.text}</code>
            </pre>
          )
        ) : null}
      </div>
    </section>
  );
});

// Terse run-state line for the polite aria-live region (A11Y-C2). Deliberately
// excludes streamed body text — only the transition (running → complete/failed).
function liveStatusMessage(run: RunState, action: PaletteAction | undefined): string {
  const label = action?.label ?? action?.subcommand ?? "Action";
  switch (run.kind) {
    case "running":
      return `${label} running.`;
    case "streaming":
      return `${label} streaming response.`;
    case "success":
      return `${label} complete.`;
    case "error":
      return `${label} failed.`;
    default:
      return "";
  }
}

function outputTitle(run: RunState): string {
  if (run.kind === "idle") return "Ready";
  return run.title;
}

function outputSubtitle(run: RunState, action: PaletteAction | undefined): string {
  if (run.kind === "idle") return action?.description ?? "No matching action";
  return run.subtitle;
}

function readingHeaderSummary(
  run: RunState,
  action: PaletteAction | undefined,
): { title: string; metrics: Array<[string, string]> } | undefined {
  if (
    run.kind !== "success" ||
    !(action?.subcommand === "scrape" || action?.subcommand === "retrieve")
  )
    return undefined;

  const payload = unwrapPayload(run.result.payload);
  const markdown =
    strField(payload, "markdown") ??
    strField(payload, "content") ??
    strField(payload, "output") ??
    strField(payload, "text") ??
    strField(payload, "body");
  const chunks = arrayByKeys(payload, ["chunks", "documents", "results"]);
  const url = strField(payload, "url") ?? strField(payload, "source_url");
  const title = strField(payload, "title") ?? strField(payload, "name") ?? outputTitle(run);
  const words = numField(payload, "word_count") ?? estimateWords(markdown);

  return {
    title,
    metrics: [
      ["Words", words ? words.toLocaleString() : "-"],
      ["Chunks", chunks.length ? chunks.length.toLocaleString() : "-"],
      ["Source", url ? hostLabel(url) : "inline"],
    ],
  };
}

function estimateWords(value: string | undefined): number | undefined {
  const words = value?.trim().split(/\s+/).filter(Boolean).length;
  return words ? words : undefined;
}

function PendingBody({ run }: { run: Extract<RunState, { kind: "running" | "streaming" }> }) {
  return (
    <div className="output-body output-code output-pending">
      <code>
        {run.kind === "streaming"
          ? run.text || "Waiting for streamed response..."
          : "Waiting for response..."}
        {"\n"}
        {run.subtitle}
      </code>
      <div className="output-pending-spinner">
        <Spinner size="sm" />
      </div>
    </div>
  );
}

function statusFor(run: RunState): { label: string; tone: "ok" | "warn" | "error" | "neutral" } {
  if (run.kind === "success") return { label: "complete", tone: "ok" };
  if (run.kind === "error") return { label: "failed", tone: "error" };
  if (run.kind === "running" || run.kind === "streaming")
    return { label: "202 Accepted", tone: "warn" };
  return { label: "ready", tone: "neutral" };
}

/** Output-panel header icon for a subcommand. Derived from the registry. */
function outputIcon(subcommand: string): LucideIcon {
  return actionBehavior(subcommand).outputIcon;
}
