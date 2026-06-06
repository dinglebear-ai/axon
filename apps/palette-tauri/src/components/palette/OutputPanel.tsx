import {
  Activity,
  BarChart3,
  BookOpen,
  Bot,
  Boxes,
  Braces,
  Camera,
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
  Pin,
  PackageOpen,
  RotateCw,
  SearchCheck,
  Sparkles,
  Stethoscope,
  X,
  type LucideIcon,
} from "lucide-react";
import { useState } from "react";
import { Streamdown } from "streamdown";

import { Spinner } from "@/components/ui/aurora/spinner";
import type { PaletteAction } from "@/lib/actions";
import type { RunState } from "@/lib/runState";

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

  return (
    <section className="output-panel">
      <div className={`output-state output-${run.kind} output-tone-${active?.tone ?? "neutral"}`}>
        <header className="output-header">
          <span className="output-op-tile" aria-hidden="true">
            <Icon size={19} strokeWidth={1.65} />
          </span>
          <div className="output-meta-info">
            <span className="output-title">{copied ? "Copied" : outputTitle(run)}</span>
            <span className="output-subtitle">{outputSubtitle(run, active)}</span>
          </div>
          <span className={`output-status output-status-${status.tone}`}>{status.label}</span>
          <span className="output-tools">
            {run.kind === "running" || run.kind === "streaming" ? (
              <>
                <button type="button" onClick={onHistory} title="History" aria-label="Open history">
                  <History size={13} />
                </button>
                <button type="button" onClick={onCollapse} title="Collapse" aria-label="Collapse output">
                  <X size={13} />
                </button>
              </>
            ) : (
              <>
                {"text" in run && (
                  <button type="button" onClick={() => onCopy(run.text)} title="Copy" aria-label="Copy output">
                  <Copy size={13} />
                  </button>
                )}
                <button type="button" onClick={onRetry} title="Re-run" aria-label="Re-run">
                  <RotateCw size={13} />
                </button>
                {outputUrl && (
                  <button type="button" onClick={() => window.open(outputUrl, "_blank", "noopener,noreferrer")} title="Open source" aria-label="Open source">
                    <ExternalLink size={13} />
                  </button>
                )}
                <button
                  type="button"
                  className={pinned ? "output-tool-active" : undefined}
                  onClick={onTogglePin}
                  title={pinned ? "Unpin" : "Pin"}
                  aria-label={pinned ? "Unpin output" : "Pin output"}
                >
                  <Pin size={13} />
                </button>
                <button type="button" onClick={onHistory} title="History" aria-label="Open history">
                  <History size={13} />
                </button>
                <button type="button" onClick={onCollapse} title="Collapse" aria-label="Collapse output">
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
        ) : null}
        {"text" in run && run.kind !== "streaming" && conversationMode ? (
          <AskConversation prompt={run.prompt ?? ""} answer={run.text} onFollowUp={onFollowUp} />
        ) : "text" in run && run.kind !== "streaming" &&
          (outputKind === "markdown" ? (
            <div className="output-body output-markdown">
              <Streamdown>{run.text}</Streamdown>
            </div>
          ) : (
            <pre className="output-body output-code">
              <code>{run.text}</code>
            </pre>
          ))}
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

function AskConversation({
  prompt,
  answer,
  pending,
  onFollowUp,
}: {
  prompt: string;
  answer: string;
  pending?: boolean;
  onFollowUp: (text: string) => void;
}) {
  const [draft, setDraft] = useState("");
  const canSend = draft.trim().length > 0 && !pending;
  return (
    <div className="ask-body">
      <div className="ask-thread aurora-scrollbar">
        {prompt ? (
          <div className="ask-message ask-message-user">
            <span>You</span>
            <p>{prompt}</p>
          </div>
        ) : null}
        <div className="ask-message ask-message-assistant">
          <span>Axon</span>
          <div className="ask-answer">
            {answer ? <Streamdown>{answer}</Streamdown> : <span className="ask-waiting">Waiting for response...</span>}
          </div>
        </div>
      </div>
      <form
        className="ask-compose"
        onSubmit={(event) => {
          event.preventDefault();
          const value = draft.trim();
          if (!value || pending) return;
          setDraft("");
          onFollowUp(value);
        }}
      >
        <input
          value={draft}
          disabled={pending}
          onChange={(event) => setDraft(event.target.value)}
          placeholder={pending ? "Waiting for response..." : "Ask a follow-up..."}
          aria-label="Ask a follow-up"
        />
        <button type="submit" disabled={!canSend}>Send</button>
      </form>
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
