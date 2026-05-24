import { invoke } from "@tauri-apps/api/core";
import { Activity, CheckCircle2, Search, Send, XCircle } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import { Badge } from "@/components/ui/aurora/badge";
import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import { ScrollArea } from "@/components/ui/aurora/scroll-area";
import { Separator } from "@/components/ui/aurora/separator";
import { Spinner } from "@/components/ui/aurora/spinner";
import { StatusIndicator } from "@/components/ui/aurora/status-indicator";
import {
  type PaletteConfig,
  type PaletteResult,
  createAxonClient,
  executeAction,
} from "@/lib/axonClient";
import {
  ACTIONS,
  type PaletteAction,
  acceptsDirectUrl,
  actionInvokedBy,
  actionMatches,
} from "@/lib/actions";
import { formatPayload } from "@/lib/format";

type RunState =
  | { kind: "idle" }
  | { kind: "running"; title: string; subtitle: string }
  | { kind: "success" | "error"; title: string; subtitle: string; text: string; result: PaletteResult };

export default function App() {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const [config, setConfig] = useState<PaletteConfig | null>(null);
  const [configError, setConfigError] = useState<string | null>(null);
  const [run, setRun] = useState<RunState>({ kind: "idle" });
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    invoke<PaletteConfig>("load_palette_config")
      .then(setConfig)
      .catch((err) => setConfigError(String(err)));
  }, []);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const parsed = useMemo(() => parseCommand(query), [query]);
  const filtered = useMemo(() => {
    if (parsed.invoked) return [parsed.invoked];
    return ACTIONS.filter((action) => actionMatches(action, parsed.search)).slice(0, 12);
  }, [parsed.invoked, parsed.search]);
  const active = filtered[Math.min(selected, Math.max(filtered.length - 1, 0))];

  useEffect(() => {
    setSelected(0);
  }, [parsed.search]);

  const client = useMemo(() => (config ? createAxonClient(config) : null), [config]);

  async function submit(action: PaletteAction = active) {
    if (!client || !action || run.kind === "running") return;
    const argument = argumentFor(action, parsed);
    const commandLine = `${action.subcommand}${argument ? ` ${argument}` : ""}`;
    setRun({
      kind: "running",
      title: `Running ${action.label}`,
      subtitle: commandLine,
    });
    try {
      const result = await executeAction(client, action, argument);
      setRun({
        kind: result.ok ? "success" : "error",
        title: `${action.label} ${result.ok ? "completed" : "failed"}`,
        subtitle: `${result.method} ${result.path} · HTTP ${result.status}`,
        text: formatPayload(action.subcommand, result.payload),
        result,
      });
    } catch (err) {
      setRun({
        kind: "error",
        title: `${action.label} failed`,
        subtitle: commandLine,
        text: err instanceof Error ? err.message : String(err),
        result: { ok: false, status: 0, path: "", method: "POST", payload: null },
      });
    }
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelected((idx) => Math.min(idx + 1, Math.max(filtered.length - 1, 0)));
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelected((idx) => Math.max(idx - 1, 0));
    } else if (event.key === "Enter") {
      event.preventDefault();
      if (active) void submit(active);
    } else if (event.key === "Tab") {
      event.preventDefault();
      if (active) setQuery(`${active.subcommand} `);
    }
  }

  return (
    <div className="aurora-page-shell palette-shell">
      <header className="palette-titlebar">
        <div className="palette-brand">
          <span className="brand-dot" />
          <span>Axon Palette</span>
          <Badge tone={configError ? "error" : config ? "success" : "neutral"} shape="tag">
            Tauri v2
          </Badge>
        </div>
        <div className="palette-status">
          {config ? (
            <StatusIndicator tone="online" label={hostLabel(config.serverUrl)} />
          ) : configError ? (
            <StatusIndicator tone="error" label="Config error" />
          ) : (
            <StatusIndicator tone="syncing" label="Loading config" />
          )}
        </div>
      </header>

      <section className="command-bar">
        <Search size={16} />
        <Input
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={onKeyDown}
          placeholder={active?.example ?? "Search commands"}
          className="command-input"
          aria-label="Axon command"
        />
        <Button size="sm" variant="rose" onClick={() => active && void submit(active)} disabled={!client || !active || run.kind === "running"}>
          {run.kind === "running" ? <Spinner size="sm" tone="rose" /> : <Send size={14} />}
          Send
        </Button>
      </section>

      <main className="palette-grid">
        <section className="action-panel">
          <div className="panel-heading">
            <span>Actions</span>
            <span>{filtered.length}</span>
          </div>
          <ScrollArea className="action-scroll" viewportClassName="action-scroll-viewport">
            <div className="action-list">
              {filtered.map((action, index) => (
                <button
                  key={action.subcommand}
                  className={index === selected ? "action-row action-row-selected" : "action-row"}
                  onClick={() => {
                    setSelected(index);
                    if ((parsed.invoked || acceptsDirectUrl(action)) && run.kind !== "running") {
                      void submit(action);
                    }
                  }}
                >
                  <span className="action-main">
                    <span className="action-label">{action.label}</span>
                    <span className="action-description">{action.description}</span>
                  </span>
                  <Badge tone={action.tone} shape="tag">
                    {action.subcommand}
                  </Badge>
                </button>
              ))}
            </div>
          </ScrollArea>
        </section>

        <section className="output-panel">
          <div className="panel-heading">
            <span>Output</span>
            {run.kind === "running" ? <Spinner size="sm" /> : run.kind === "success" ? <CheckCircle2 size={15} /> : run.kind === "error" ? <XCircle size={15} /> : <Activity size={15} />}
          </div>
          <Separator />
          <div className={`output-state output-${run.kind}`}>
            <div className="output-title">{outputTitle(run)}</div>
            <div className="output-subtitle">{outputSubtitle(run, active)}</div>
            {"text" in run && (
              <pre className="output-body">
                <code>{run.text}</code>
              </pre>
            )}
          </div>
        </section>
      </main>
    </div>
  );
}

function parseCommand(raw: string): { invoked?: PaletteAction; search: string; arg: string } {
  const trimmed = raw.trimStart();
  const [token = ""] = trimmed.split(/\s+/);
  const invoked = ACTIONS.find((action) => actionInvokedBy(action, token));
  if (invoked) {
    return { invoked, search: token, arg: trimmed.slice(token.length).trimStart() };
  }
  return { search: trimmed, arg: "" };
}

function argumentFor(action: PaletteAction, parsed: { invoked?: PaletteAction; search: string; arg: string }): string {
  if (parsed.invoked?.subcommand === action.subcommand) return parsed.arg;
  if (looksLikeUrl(parsed.search) && acceptsDirectUrl(action)) return parsed.search;
  return parsed.search;
}

function looksLikeUrl(value: string): boolean {
  return /^https?:\/\//i.test(value.trim());
}

function outputTitle(run: RunState): string {
  if (run.kind === "idle") return "Ready";
  return run.title;
}

function outputSubtitle(run: RunState, action: PaletteAction | undefined): string {
  if (run.kind === "idle") return action?.description ?? "No matching action";
  return run.subtitle;
}

function hostLabel(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
