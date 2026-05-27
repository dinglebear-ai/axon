import { Activity, CheckCircle2, Copy, ExternalLink, RotateCw, XCircle } from "lucide-react";
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
}

export function OutputPanel({
  active,
  copied,
  outputKind,
  run,
  onCopy,
  onRetry,
}: OutputPanelProps) {
  const outputUrl = "text" in run ? firstUrl(run.text) : null;

  return (
    <section className="output-panel">
      <div className={`output-state output-${run.kind}`}>
        <div className="output-meta">
          <div className="output-meta-info">
            <span className="output-title">{copied ? "Copied" : outputTitle(run)}</span>
            <span className="output-subtitle">{outputSubtitle(run, active)}</span>
          </div>
          <span className="output-tools">
            {"text" in run && (
              <>
                <button type="button" onClick={() => onCopy(run.text)} title="Copy output" aria-label="Copy output">
                  <Copy size={13} />
                </button>
                <button type="button" onClick={onRetry} title="Retry" aria-label="Retry">
                  <RotateCw size={13} />
                </button>
              </>
            )}
            {outputUrl && (
              <button type="button" onClick={() => window.open(outputUrl, "_blank", "noopener,noreferrer")} title="Open first URL" aria-label="Open first URL">
                <ExternalLink size={13} />
              </button>
            )}
            {run.kind === "running" || run.kind === "streaming" ? (
              <Spinner size="sm" />
            ) : run.kind === "success" ? (
              <CheckCircle2 size={14} />
            ) : run.kind === "error" ? (
              <XCircle size={14} />
            ) : (
              <Activity size={14} />
            )}
          </span>
        </div>
        {"text" in run &&
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
