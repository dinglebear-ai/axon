import { Activity, AlertTriangle, Plus, RefreshCw, Server, Trash2 } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "@/components/ui/aurora/button";
import { Input } from "@/components/ui/aurora/input";
import type { BackendProfile } from "@/lib/backendProfiles/model";
import { GatewayClient, GatewayClientError } from "@/lib/labby/gateway/client";
import {
  draftFromGateway,
  emptyGatewayDraft,
  type GatewayDraft,
  type GatewayView,
  gatewayChallenge,
  gatewayFingerprint,
} from "@/lib/labby/gateway/model";

type Pending = { label: string; run: () => Promise<void> } | null;

export function GatewayWorkspace({ profile }: { profile: BackendProfile }) {
  const client = useMemo(() => new GatewayClient(profile), [profile]);
  const [rows, setRows] = useState<GatewayView[]>([]);
  const [selected, setSelected] = useState<GatewayView | null>(null);
  const [draft, setDraft] = useState<GatewayDraft>(emptyGatewayDraft());
  const [mode, setMode] = useState<"list" | "add" | "edit">("list");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending>(null);
  const request = useRef<AbortController | null>(null);

  const refresh = useCallback(async () => {
    request.current?.abort();
    const abort = new AbortController();
    request.current = abort;
    setBusy(true);
    setError(null);
    try {
      setRows(await client.list(abort.signal));
    } catch (cause) {
      if ((cause as Error).name !== "AbortError") setError(messageFor(cause));
    } finally {
      if (request.current === abort) {
        request.current = null;
        setBusy(false);
      }
    }
  }, [client]);

  useEffect(() => {
    setRows([]);
    setSelected(null);
    setMode("list");
    setPending(null);
    setNotice(null);
    void refresh();
    return () => request.current?.abort();
  }, [refresh]);

  const run = useCallback(
    async (operation: (signal: AbortSignal) => Promise<void>) => {
      request.current?.abort();
      const abort = new AbortController();
      request.current = abort;
      setBusy(true);
      setError(null);
      setNotice(null);
      try {
        await operation(abort.signal);
        setPending(null);
        setMode("list");
        setSelected(null);
        setRows(await client.list(abort.signal));
      } catch (cause) {
        if ((cause as Error).name !== "AbortError") setError(messageFor(cause));
      } finally {
        if (request.current === abort) {
          request.current = null;
          setBusy(false);
        }
      }
    },
    [client],
  );

  function beginEdit(row: GatewayView) {
    setSelected(row);
    setDraft(draftFromGateway(row));
    setMode("edit");
    setError(null);
    setNotice(null);
  }

  function test() {
    const challenge = gatewayChallenge(draft);
    setPending({
      label: challenge.messages.length
        ? `Confirm test: ${challenge.messages.join(" ")}`
        : "Confirm connection test against the Labby host.",
      run: async () => {
        setBusy(true);
        setError(null);
        try {
          const runtime = await client.testDraft(draft);
          setNotice(
            runtime.connected
              ? `Connected — ${runtime.tool_count} tools, ${runtime.resource_count} resources, ${runtime.prompt_count} prompts.`
              : (runtime.last_error ?? "The upstream did not connect."),
          );
          setPending(null);
        } catch (cause) {
          setError(messageFor(cause));
        } finally {
          setBusy(false);
        }
      },
    });
  }

  function save() {
    const challenge = gatewayChallenge(draft);
    setPending({
      label: `Confirm ${mode === "add" ? "add" : "update"}. ${challenge.messages.join(" ")}`.trim(),
      run: () =>
        run(async (signal) => {
          if (mode === "add") await client.create(draft, signal);
          else if (selected)
            await client.update(selected.config.name, draft, gatewayFingerprint(selected), signal);
        }),
    });
  }

  return (
    <section className="gateway-workspace" aria-label="Labby gateway administration">
      <header className="gateway-workspace-header">
        <span>
          <Server size={16} />
          <strong>Labby gateway</strong>
          <small>{profile.label} · server-owned upstream administration</small>
        </span>
        <Button size="sm" variant="neutral" disabled={busy} onClick={() => void refresh()}>
          <RefreshCw size={13} />
          Refresh
        </Button>
      </header>

      {error ? (
        <div className="gateway-banner gateway-banner-error" role="alert">
          {error}
        </div>
      ) : null}
      {notice ? (
        <div className="gateway-banner" aria-live="polite">
          {notice}
        </div>
      ) : null}
      {pending ? (
        <div className="gateway-confirm" role="alertdialog" aria-label="Confirm gateway operation">
          <AlertTriangle size={16} />
          <span>{pending.label}</span>
          <Button size="sm" variant="aurora" disabled={busy} onClick={() => void pending.run()}>
            Confirm
          </Button>
          <Button size="sm" variant="neutral" onClick={() => setPending(null)}>
            Cancel
          </Button>
        </div>
      ) : null}

      {mode === "list" ? (
        <div className="gateway-list">
          <div className="gateway-list-toolbar">
            <span>
              {busy ? "Loading…" : `${rows.length} upstream${rows.length === 1 ? "" : "s"}`}
            </span>
            <Button
              size="sm"
              variant="aurora"
              onClick={() => {
                setDraft(emptyGatewayDraft());
                setMode("add");
              }}
            >
              <Plus size={13} />
              Add upstream
            </Button>
          </div>
          {rows.map((row) => (
            <button
              type="button"
              className="gateway-row"
              key={row.config.name}
              onClick={() => beginEdit(row)}
            >
              <span
                className={row.runtime.connected ? "gateway-dot gateway-dot-ok" : "gateway-dot"}
              />
              <span>
                <strong>{row.config.name}</strong>
                <small>
                  {row.config.command ? `stdio · ${row.config.command}` : row.config.url}
                </small>
              </span>
              <span>
                {row.config.enabled ? "Enabled" : "Disabled"}
                <small>{row.runtime.tool_count} tools</small>
              </span>
            </button>
          ))}
          {!busy && rows.length === 0 ? (
            <div className="gateway-empty">No upstreams configured.</div>
          ) : null}
        </div>
      ) : (
        <GatewayForm
          draft={draft}
          busy={busy}
          onChange={setDraft}
          onCancel={() => {
            setMode("list");
            setSelected(null);
            setPending(null);
          }}
          onTest={test}
          onSave={save}
          onRemove={
            mode === "edit" && selected
              ? () =>
                  setPending({
                    label: `Remove ${selected.config.name} from Labby's persisted gateway configuration?`,
                    run: () =>
                      run((signal) =>
                        client.remove(selected.config.name, gatewayFingerprint(selected), signal),
                      ),
                  })
              : undefined
          }
          onReload={
            mode === "edit" && selected
              ? () =>
                  setPending({
                    label: `Reload Labby's gateway runtime and re-read ${selected.config.name}? In-flight requests keep their existing pool.`,
                    run: () =>
                      run(async (signal) => {
                        await client.reload(
                          selected.config.name,
                          gatewayFingerprint(selected),
                          signal,
                        );
                      }),
                  })
              : undefined
          }
        />
      )}
    </section>
  );
}

function GatewayForm({
  draft,
  busy,
  onChange,
  onCancel,
  onTest,
  onSave,
  onRemove,
  onReload,
}: {
  draft: GatewayDraft;
  busy: boolean;
  onChange: (draft: GatewayDraft) => void;
  onCancel: () => void;
  onTest: () => void;
  onSave: () => void;
  onRemove?: () => void;
  onReload?: () => void;
}) {
  const challenge = useMemo(() => gatewayChallenge(draft), [draft]);
  const field = (key: keyof GatewayDraft, value: string | boolean) =>
    onChange({ ...draft, [key]: value });
  return (
    <div className="gateway-form">
      <div className="gateway-form-grid">
        <label htmlFor="gateway-name">
          Name
          <Input
            id="gateway-name"
            value={draft.name}
            disabled={Boolean(onRemove)}
            onChange={(event) => field("name", event.target.value)}
          />
        </label>
        <label htmlFor="gateway-transport">
          Transport
          <select
            id="gateway-transport"
            value={draft.transport}
            onChange={(event) => field("transport", event.target.value)}
          >
            <option value="http">HTTP</option>
            <option value="stdio">stdio</option>
          </select>
        </label>
        {draft.transport === "http" ? (
          <label className="gateway-form-wide" htmlFor="gateway-url">
            Endpoint URL
            <Input
              id="gateway-url"
              value={draft.url}
              onChange={(event) => field("url", event.target.value)}
              placeholder="https://server.example/mcp"
            />
          </label>
        ) : (
          <>
            <label htmlFor="gateway-command">
              Command
              <Input
                id="gateway-command"
                value={draft.command}
                onChange={(event) => field("command", event.target.value)}
                placeholder="npx"
              />
            </label>
            <label htmlFor="gateway-args">
              Arguments (one per line)
              <textarea
                id="gateway-args"
                value={draft.args}
                onChange={(event) => field("args", event.target.value)}
              />
            </label>
          </>
        )}
        <label htmlFor="gateway-token-env">
          Bearer token environment variable
          <Input
            id="gateway-token-env"
            value={draft.bearerTokenEnv}
            onChange={(event) => field("bearerTokenEnv", event.target.value)}
            placeholder="MCP_TOKEN"
          />
        </label>
        <label htmlFor="gateway-exposure">
          Tool exposure (one pattern per line)
          <textarea
            id="gateway-exposure"
            value={draft.exposeTools}
            onChange={(event) => field("exposeTools", event.target.value)}
            placeholder="Leave blank to expose all"
          />
        </label>
      </div>
      <div className="gateway-toggles">
        {(
          [
            ["enabled", "Enabled"],
            ["oauthEnabled", "OAuth"],
            ["proxyResources", "Proxy resources"],
            ["proxyPrompts", "Proxy prompts"],
          ] as const
        ).map(([key, label]) => (
          <label key={key}>
            <input
              type="checkbox"
              checked={draft[key]}
              onChange={(event) => field(key, event.target.checked)}
            />
            {label}
          </label>
        ))}
      </div>
      {challenge.messages.map((message) => (
        <div className="gateway-disclosure" key={message}>
          <AlertTriangle size={14} />
          {message}
        </div>
      ))}
      <div className="gateway-form-actions">
        <Button size="sm" variant="neutral" onClick={onCancel}>
          Cancel
        </Button>
        <Button size="sm" variant="neutral" disabled={busy || !draft.name.trim()} onClick={onTest}>
          <Activity size={13} />
          Test
        </Button>
        {onReload ? (
          <Button size="sm" variant="neutral" disabled={busy} onClick={onReload}>
            <RefreshCw size={13} />
            Reload
          </Button>
        ) : null}
        {onRemove ? (
          <Button size="sm" variant="neutral" disabled={busy} onClick={onRemove}>
            <Trash2 size={13} />
            Remove
          </Button>
        ) : null}
        <Button
          size="sm"
          variant="aurora"
          disabled={
            busy ||
            !draft.name.trim() ||
            (draft.transport === "http" ? !draft.url.trim() : !draft.command.trim())
          }
          onClick={onSave}
        >
          Save
        </Button>
      </div>
    </div>
  );
}

function messageFor(cause: unknown): string {
  if (cause instanceof GatewayClientError) {
    if (cause.status === 401 || cause.status === 403)
      return `Authorization challenge: ${cause.message}`;
    if (cause.status >= 500) return `Labby gateway runtime unavailable: ${cause.message}`;
  }
  return cause instanceof Error ? cause.message : String(cause);
}
