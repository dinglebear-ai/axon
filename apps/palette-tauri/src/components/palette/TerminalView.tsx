import { Check, Copy, Trash2 } from "lucide-react";
import { memo, useEffect, useRef, useState } from "react";

import { WorkspaceSurface } from "@/components/palette/WorkspaceSurface";
import { Button } from "@/components/ui/aurora/button";
import { invoke, isTauriRuntime } from "@/lib/invoke";

interface TerminalRunResult {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  cwd: string;
}

type TerminalLineKind = "sys" | "in" | "out" | "err";

interface TerminalLine {
  id: number;
  kind: TerminalLineKind;
  text: string;
}

let lineIdSeq = 0;
function nextLineId(): number {
  lineIdSeq += 1;
  return lineIdSeq;
}

function shortCwd(cwd: string): string {
  const home = cwd.match(/^\/(?:home|Users)\/[^/]+/)?.[0];
  if (home && cwd.startsWith(home)) return `~${cwd.slice(home.length)}` || "~";
  return cwd || "~";
}

/**
 * Real shell terminal: every submitted line runs a genuine command via the
 * `terminal_run` Tauri command (see `src-tauri/src/terminal.rs`), which spawns
 * the user's real login shell and captures real stdout/stderr. The working
 * directory is tracked server-side across commands in this session (`cd`
 * updates it) so this behaves like a real interactive shell, not a one-shot
 * subprocess per keystroke.
 *
 * In the browser-dev fallback (no Tauri runtime — `pnpm vite:dev`), there is
 * no real shell to spawn, so this shows a clear unavailable message instead
 * of faking output.
 */
export const TerminalView = memo(function TerminalView() {
  const [lines, setLines] = useState<TerminalLine[]>(() => [
    {
      id: nextLineId(),
      kind: "sys",
      text: "Shell ready.",
    },
  ]);
  const [cwd, setCwd] = useState<string>("");
  const [input, setInput] = useState("");
  const [running, setRunning] = useState(false);
  const [copied, setCopied] = useState(false);
  const [history, setHistory] = useState<string[]>([]);
  const historyIndexRef = useRef<number | null>(null);
  const draftRef = useRef("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!isTauriRuntime) return;
    let cancelled = false;
    void invoke<string>("terminal_cwd").then((value) => {
      if (!cancelled) setCwd(value);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `lines` is the scroll trigger.
  useEffect(() => {
    const element = scrollRef.current;
    if (element) element.scrollTop = element.scrollHeight;
  }, [lines]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  function appendLine(kind: TerminalLineKind, text: string) {
    if (!text) return;
    setLines((prev) => [...prev, { id: nextLineId(), kind, text }]);
  }

  async function runCommand(command: string) {
    const trimmed = command.trim();
    appendLine("in", command);
    if (trimmed) {
      setHistory((prev) => (prev[prev.length - 1] === trimmed ? prev : [...prev, trimmed]));
    }
    historyIndexRef.current = null;
    draftRef.current = "";
    setInput("");

    if (trimmed === "clear") {
      setLines([]);
      return;
    }
    if (!trimmed) return;

    setRunning(true);
    try {
      const result = await invoke<TerminalRunResult>("terminal_run", { command: trimmed });
      if (result.stdout) appendLine("out", result.stdout.replace(/\n$/, ""));
      if (result.stderr) appendLine("err", result.stderr.replace(/\n$/, ""));
      if (result.exitCode !== null && result.exitCode !== 0) {
        appendLine("sys", `exit ${result.exitCode}`);
      }
      setCwd(result.cwd);
    } catch (err) {
      appendLine("err", err instanceof Error ? err.message : String(err));
    } finally {
      setRunning(false);
    }
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "Enter") {
      event.preventDefault();
      if (running) return;
      void runCommand(input);
      return;
    }
    if (event.key === "ArrowUp") {
      if (history.length === 0) return;
      event.preventDefault();
      const current = historyIndexRef.current;
      if (current === null) draftRef.current = input;
      const nextIndex = current === null ? history.length - 1 : Math.max(current - 1, 0);
      historyIndexRef.current = nextIndex;
      setInput(history[nextIndex] ?? "");
      return;
    }
    if (event.key === "ArrowDown") {
      if (historyIndexRef.current === null) return;
      event.preventDefault();
      const nextIndex = historyIndexRef.current + 1;
      if (nextIndex >= history.length) {
        historyIndexRef.current = null;
        setInput(draftRef.current);
      } else {
        historyIndexRef.current = nextIndex;
        setInput(history[nextIndex] ?? "");
      }
    }
  }

  if (!isTauriRuntime) {
    return (
      <div className="output-body terminal-view terminal-view-unavailable" role="status">
        <p>Terminal requires the desktop app.</p>
        <p className="terminal-view-unavailable-detail">
          Real shell commands can only run through the Tauri desktop shell — the browser dev preview
          has no local shell to spawn.
        </p>
      </div>
    );
  }

  return (
    <WorkspaceSurface className="output-body terminal-view">
      <div className="terminal-toolbar" role="toolbar" aria-label="Terminal controls">
        <span className="terminal-toolbar-path">{shortCwd(cwd)}</span>
        <span className={`terminal-toolbar-state${running ? " is-running" : ""}`}>
          {running ? "Running" : "Ready"}
        </span>
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          onClick={() => {
            void navigator.clipboard
              .writeText(lines.map((line) => line.text).join("\n"))
              .then(() => {
                setCopied(true);
                window.setTimeout(() => setCopied(false), 1400);
              });
          }}
          aria-label="Copy terminal output"
          title={copied ? "Copied" : "Copy output"}
          className={copied ? "is-confirmed" : undefined}
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
        </Button>
        <Button
          variant="plain"
          size="unstyled"
          type="button"
          onClick={() => setLines([])}
          aria-label="Clear terminal"
          title="Clear terminal"
          disabled={lines.length === 0}
        >
          <Trash2 size={14} />
        </Button>
      </div>
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: keyboard users focus the input directly; this wrapper only expands the pointer target */}
      <div
        ref={scrollRef}
        className="terminal-scroll aurora-scrollbar"
        role="log"
        aria-live="polite"
        onClick={() => inputRef.current?.focus()}
      >
        {lines.map((line) => (
          <div key={line.id} className={`terminal-line terminal-line-${line.kind}`}>
            {line.kind === "in" ? (
              <>
                <span className="terminal-prompt">{shortCwd(cwd)}$</span>
                <span className="terminal-line-text">{line.text}</span>
              </>
            ) : (
              <span className="terminal-line-text">{line.text}</span>
            )}
          </div>
        ))}
        <div className="terminal-line terminal-input-row">
          <span className="terminal-prompt">{shortCwd(cwd)}$</span>
          <input
            ref={inputRef}
            className="terminal-input"
            value={input}
            spellCheck={false}
            autoComplete="off"
            autoCapitalize="off"
            disabled={running}
            aria-label="Terminal command input"
            onChange={(event) => setInput(event.target.value)}
            onKeyDown={onKeyDown}
          />
        </div>
      </div>
    </WorkspaceSurface>
  );
});
