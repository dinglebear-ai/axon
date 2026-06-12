import {
  Activity,
  BarChart3,
  BookOpen,
  Bot,
  Boxes,
  Braces,
  Camera,
  Check,
  Copy,
  Database,
  ExternalLink,
  FileDown,
  GitBranch,
  GitCompare,
  Globe,
  History,
  HelpCircle,
  Layers,
  Map,
  MoreHorizontal,
  Pin,
  PackageOpen,
  RotateCw,
  SearchCheck,
  Sparkles,
  Stethoscope,
  X,
  type LucideIcon,
} from "lucide-react";
import { Streamdown } from "streamdown";

import { AskConversation } from "@/components/palette/AskConversation";
import { ErrorResultView } from "@/components/palette/ErrorResultView";
import { EvaluateView } from "@/components/palette/EvaluateView";
import { hasStructuredOperationView, OperationResultView } from "@/components/palette/OperationResultView";
import { StatsView } from "@/components/palette/StatsView";
import { StatusView } from "@/components/palette/StatusView";
import { Spinner } from "@/components/ui/aurora/spinner";
import type { PaletteAction } from "@/lib/actions";
import { arrField, numField, strField, unwrapPayload } from "@/lib/payload";
import type { RunState } from "@/lib/runState";
import { STREAMDOWN_CODE_THEMES, STREAMDOWN_PLUGINS } from "@/lib/streamdownConfig";

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
}

export function OutputPanel({
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
}: OutputPanelProps) {
  const outputUrl = "text" in run ? firstUrl(run.text) : null;
  const Icon = active ? outputIcon(active.subcommand) : Activity;
  const status = statusFor(run);
  const conversationMode = active?.subcommand === "ask" || active?.subcommand === "chat";
  const headerSummary = readingHeaderSummary(run, active);

  return (
    <section className="output-panel">
      <div className={`output-state output-${run.kind} output-tone-${active?.tone ?? "neutral"}`}>
        <header className={headerSummary ? "output-header output-header-summary" : "output-header"}>
          <span className="output-op-tile" aria-hidden="true">
            <Icon size={19} strokeWidth={1.65} />
          </span>
          <div className="output-meta-info">
            <span className="output-title-line">
              <span className="output-title">{headerSummary?.title ?? outputTitle(run)}</span>
              {headerSummary ? (
                <span className="output-summary-chips" aria-label="Result summary">
                  {headerSummary.metrics.map(([label, value]) => (
                    <span key={label}>
                      <strong>{value}</strong>
                      {label}
                    </span>
                  ))}
                </span>
              ) : null}
            </span>
            <span className="output-subtitle">{outputSubtitle(run, active)}</span>
          </div>
          <span className={`output-status output-status-${status.tone}`}>{status.label}</span>
          <span className="output-tools">
            {run.kind === "running" || run.kind === "streaming" ? (
              <>
                <button type="button" onClick={onHistory} title="History" aria-label="Open history" data-tooltip="History">
                  <History size={13} />
                </button>
                <button type="button" onClick={onCollapse} title="Collapse" aria-label="Collapse output" data-tooltip="Collapse">
                  <X size={13} />
                </button>
              </>
            ) : (
              <>
                {"text" in run && (
                  <button
                    type="button"
                    className={copied ? "output-tool-copied" : undefined}
                    onClick={() => onCopy(run.text)}
                    title={copied ? "Copied" : "Copy"}
                    aria-label={copied ? "Copied output" : "Copy output"}
                    data-tooltip={copied ? "Copied" : "Copy"}
                  >
                    {copied ? <Check size={13} /> : <Copy size={13} />}
                  </button>
                )}
                <details className="output-tool-menu">
                  <summary title="More actions" aria-label="More actions" data-tooltip="More">
                    <MoreHorizontal size={13} />
                  </summary>
                  <div>
                    <button type="button" onClick={onRetry}>
                      <RotateCw size={13} />
                      <span>Re-run</span>
                    </button>
                    {outputUrl && (
                      <button type="button" onClick={() => window.open(outputUrl, "_blank", "noopener,noreferrer")}>
                        <ExternalLink size={13} />
                        <span>Open source</span>
                      </button>
                    )}
                    <button type="button" className={pinned ? "output-tool-active" : undefined} onClick={onTogglePin}>
                      <Pin size={13} />
                      <span>{pinned ? "Unpin" : "Pin"}</span>
                    </button>
                    <button type="button" onClick={onHistory}>
                      <History size={13} />
                      <span>History</span>
                    </button>
                  </div>
                </details>
                <button type="button" onClick={onCollapse} title="Collapse" aria-label="Collapse output" data-tooltip="Collapse">
                  <X size={13} />
                </button>
              </>
            )}
            {run.kind === "running" || run.kind === "streaming" ? <Spinner size="sm" /> : null}
          </span>
        </header>
        {run.kind === "streaming" && conversationMode ? (
          <AskConversation prompt={run.prompt ?? ""} answer={run.text} pending onFollowUp={onFollowUp} />
        ) : (run.kind === "running" || run.kind === "streaming") ? (
          <PendingBody run={run} />
        ) : run.kind === "success" && active?.subcommand === "evaluate" ? (
          <EvaluateView payload={run.result.payload} />
        ) : run.kind === "success" && active?.subcommand === "stats" ? (
          <StatsView payload={run.result.payload} />
        ) : run.kind === "success" && active?.subcommand === "status" ? (
          <StatusView payload={run.result.payload} />
        ) : run.kind === "success" && active && hasStructuredOperationView(active.subcommand) ? (
          <OperationResultView payload={run.result.payload} subcommand={active.subcommand} />
        ) : run.kind === "error" ? (
          <ErrorResultView result={run.result} text={run.text} />
        ) : "text" in run && conversationMode ? (
          <AskConversation prompt={run.prompt ?? ""} answer={run.text} onFollowUp={onFollowUp} />
        ) : "text" in run ? (
          outputKind === "markdown" ? (
            <div className="output-body output-markdown">
              <Streamdown plugins={STREAMDOWN_PLUGINS} shikiTheme={STREAMDOWN_CODE_THEMES}>
                {run.text}
              </Streamdown>
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
  if (run.kind !== "success" || !(action?.subcommand === "scrape" || action?.subcommand === "retrieve")) return undefined;

  const payload = unwrapPayload(run.result.payload);
  const markdown =
    strField(payload, "markdown") ??
    strField(payload, "content") ??
    strField(payload, "output") ??
    strField(payload, "text") ??
    strField(payload, "body");
  const chunks = firstArray(payload, ["chunks", "documents", "results"]);
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

function firstArray(payload: Record<string, unknown>, keys: string[]): unknown[] {
  for (const key of keys) {
    const value = arrField(payload, key);
    if (value.length > 0) return value;
  }
  return [];
}

function estimateWords(value: string | undefined): number | undefined {
  const words = value?.trim().split(/\s+/).filter(Boolean).length;
  return words ? words : undefined;
}

function hostLabel(url: string): string {
  try {
    return new URL(url).hostname;
  } catch {
    return url.split("/")[0] || url;
  }
}

function firstUrl(value: string): string | null {
  return value.match(/https?:\/\/[^\s"')\]}]+/i)?.[0] ?? null;
}

function PendingBody({
  run,
}: {
  run: Extract<RunState, { kind: "running" | "streaming" }>;
}) {
  return (
    <div className="output-body output-code output-pending">
      <code>
        {run.kind === "streaming" ? run.text || "Waiting for streamed response..." : "Waiting for response..."}
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
  if (run.kind === "running" || run.kind === "streaming") return { label: "202 Accepted", tone: "warn" };
  return { label: "ready", tone: "neutral" };
}

function outputIcon(subcommand: string): LucideIcon {
  const lifecycle = /^(crawl|embed|extract|ingest)-(list|status|cancel|cleanup|clear|recover)$/.exec(subcommand);
  if (lifecycle) {
    const [, family, action] = lifecycle;
    if (action === "cancel") return X;
    if (action === "cleanup" || action === "clear" || action === "recover") return RotateCw;
    if (action === "list" || action === "status") return Activity;
    if (family === "crawl") return GitBranch;
    if (family === "embed") return Layers;
    if (family === "extract") return Braces;
    if (family === "ingest") return PackageOpen;
  }

  switch (subcommand) {
    case "scrape":
      return FileDown;
    case "crawl":
      return GitBranch;
    case "map":
      return Map;
    case "summarize":
      return BookOpen;
    case "ask":
      return Bot;
    case "query":
      return SearchCheck;
    case "retrieve":
      return Database;
    case "suggest":
      return Sparkles;
    case "evaluate":
      return BarChart3;
    case "search":
    case "research":
      return Globe;
    case "embed":
      return Layers;
    case "extract":
      return Braces;
    case "ingest":
      return PackageOpen;
    case "status":
      return Activity;
    case "sources":
      return Boxes;
    case "domains":
      return Database;
    case "stats":
      return BarChart3;
    case "doctor":
      return Stethoscope;
    case "brand":
      return Sparkles;
    case "diff":
      return GitCompare;
    case "screenshot":
      return Camera;
    default:
      return HelpCircle;
  }
}
