'use client';

import { useEffect, useMemo, useState } from 'react';

type PanelState = {
  setup_required: boolean;
  config_path: string;
};

type ConfigResponse = {
  path: string;
  raw_toml: string;
  restart_required: boolean;
};

type SaveConfigResponse = {
  ok: boolean;
  restart_required: boolean;
  message: string;
};

type OpsResponse = {
  qdrant_url: string;
  tei_url: string;
  collection: string;
  mcp_http_url: string;
};

type StackCheck = {
  label: string;
  status: 'ok' | 'warn' | 'error' | 'skipped' | string;
  detail: string;
};

type StackUrlCheck = StackCheck & {
  url: string;
};

type StackResponse = {
  runtime_mode: 'host' | 'container' | string;
  server_url: string;
  mcp_url: string;
  log_dir: string;
  compose_file: string;
  urls: StackUrlCheck[];
  checks: StackCheck[];
};


const TOKEN_KEY = 'axon-panel-token';

export default function Page() {
  const [token, setToken] = useState('');
  const [password, setPassword] = useState('');
  const [panelState, setPanelState] = useState<PanelState | null>(null);
  const [config, setConfig] = useState('');
  const [ops, setOps] = useState<OpsResponse | null>(null);
  const [stack, setStack] = useState<StackResponse | null>(null);
  const [stackLoading, setStackLoading] = useState(false);
  const [stackStatus, setStackStatus] = useState('');
  const [firstUrl, setFirstUrl] = useState('https://example.com');
  const [firstQuestion, setFirstQuestion] = useState('What did we crawl?');
  const [firstRunResult, setFirstRunResult] = useState('');
  const [firstRunBusy, setFirstRunBusy] = useState(false);
  const [message, setMessage] = useState('');

  const authedHeaders = useMemo(
    () => ({
      'content-type': 'application/json',
      'x-axon-panel-token': token
    }),
    [token]
  );

  useEffect(() => {
    setToken(window.localStorage.getItem(TOKEN_KEY) ?? '');
    fetch('/api/panel/state')
      .then((res) => res.json())
      .then(setPanelState)
      .catch((error) => setMessage(String(error)));
  }, []);

  useEffect(() => {
    if (!token) return;
    void fetch('/api/panel/config', { headers: authedHeaders })
      .then(async (res) => {
        if (!res.ok) throw new Error(await res.text());
        return res.json();
      })
      .then((cfg: ConfigResponse) => {
        setConfig(cfg.raw_toml);
      })
      .catch((error) => setMessage(String(error)));

    void fetch('/api/panel/ops', { headers: authedHeaders })
      .then(async (res) => {
        if (!res.ok) throw new Error(await res.text());
        return res.json();
      })
      .then((opsData: OpsResponse) => {
        setOps(opsData);
      })
      .catch((error) => setMessage(String(error)));

    void refreshStack();
  }, [token, authedHeaders]);

  async function refreshStack() {
    if (!token) return;
    setStackLoading(true);
    setStackStatus('');
    try {
      const res = await fetch('/api/panel/stack', { headers: authedHeaders });
      const body = await res.text();
      if (!res.ok) {
        setStack(null);
        setStackStatus(body || `Stack check failed with HTTP ${res.status}`);
        return;
      }
      try {
        setStack(JSON.parse(body) as StackResponse);
      } catch (error) {
        setStack(null);
        setStackStatus(`Stack check returned invalid JSON: ${String(error)}`);
      }
    } catch (error) {
      setStack(null);
      setStackStatus(`Stack check failed: ${String(error)}`);
    } finally {
      setStackLoading(false);
    }
  }

  async function login() {
    const res = await fetch('/api/panel/login', {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ password })
    });
    const body = await res.json();
    if (!body.ok || !body.token) {
      setMessage('Password rejected');
      return;
    }
    window.localStorage.setItem(TOKEN_KEY, body.token);
    setToken(body.token);
    setPassword('');
    setMessage('');
  }

  async function saveConfig() {
    const res = await fetch('/api/panel/config', {
      method: 'PUT',
      headers: authedHeaders,
      body: JSON.stringify({ raw_toml: config })
    });
    if (!res.ok) {
      setMessage(await res.text());
      return;
    }
    const body = (await res.json()) as SaveConfigResponse;
    setMessage(body.message);
  }

  async function runFirstCrawl() {
    setFirstRunBusy(true);
    setFirstRunResult('');
    try {
      const res = await fetch('/api/panel/first-run/crawl', {
        method: 'POST',
        headers: authedHeaders,
        body: JSON.stringify({ url: firstUrl })
      });
      const body = await res.text();
      setFirstRunResult(res.ok ? body : `Error: ${body}`);
    } catch (error) {
      setFirstRunResult(`Error: ${String(error)}`);
    } finally {
      setFirstRunBusy(false);
    }
  }

  async function runFirstAsk() {
    setFirstRunBusy(true);
    setFirstRunResult('');
    try {
      const res = await fetch('/api/panel/first-run/ask', {
        method: 'POST',
        headers: authedHeaders,
        body: JSON.stringify({ query: firstQuestion })
      });
      const body = await res.text();
      setFirstRunResult(res.ok ? body : `Error: ${body}`);
    } catch (error) {
      setFirstRunResult(`Error: ${String(error)}`);
    } finally {
      setFirstRunBusy(false);
    }
  }

  if (!token) {
    return (
      <main className="shell narrow">
        <section className="login-panel">
          <div>
            <p className="eyebrow">Axon Admin</p>
            <h1>{panelState?.setup_required ? 'Setup Wizard' : 'Management Dashboard'}</h1>
            <p className="muted">{panelState?.config_path ?? '~/.axon/config.toml'}</p>
          </div>
          <label>
            Panel password
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void login();
              }}
              autoFocus
            />
          </label>
          <button onClick={() => void login()}>Unlock</button>
          {message && <p className="error">{message}</p>}
        </section>
      </main>
    );
  }

  return (
    <main className="shell">
      <header className="topbar">
        <div>
          <p className="eyebrow">Axon Admin</p>
          <h1>{panelState?.setup_required ? 'Setup Wizard' : 'Management Dashboard'}</h1>
        </div>
        <button
          className="ghost"
          onClick={() => {
            window.localStorage.removeItem(TOKEN_KEY);
            setToken('');
          }}
        >
          Lock
        </button>
      </header>

      <section className="ops-grid">
        <Metric label="Collection" value={ops?.collection ?? '...'} />
        <Metric label="Qdrant" value={ops?.qdrant_url ?? '...'} />
        <Metric label="TEI" value={ops?.tei_url ?? '...'} />
        <Metric label="MCP HTTP" value={ops?.mcp_http_url ?? '...'} />
      </section>

      <section className="stack-panel">
        <div className="section-heading">
          <div>
            <h2>Docker Stack</h2>
            <p>
              {stack
                ? `${stack.runtime_mode} · ${stack.server_url} · ${stack.log_dir}`
                : stackLoading
                  ? 'Checking runtime'
                  : stackStatus || 'Runtime status unavailable'}
            </p>
          </div>
          <button className="ghost" onClick={() => void refreshStack()} disabled={stackLoading}>
            {stackLoading ? 'Refreshing' : 'Refresh'}
          </button>
        </div>
        {stackStatus && <p className="error">{stackStatus}</p>}
        <div className="url-grid" aria-label="Service URL reachability">
          {(stack?.urls ?? []).map((urlCheck) => (
            <UrlCard check={urlCheck} key={urlCheck.label} />
          ))}
        </div>
        <div className="check-grid">
          {(stack?.checks ?? []).map((check) => (
            <div className={`check-card ${check.status}`} key={check.label}>
              <span>{check.label}</span>
              <StatusBadge status={check.status} />
              <p>{check.detail}</p>
            </div>
          ))}
        </div>
      </section>

      <section className="first-run-panel">
        <div className="section-heading">
          <div>
            <h2>First Run</h2>
            <p>{stack?.mcp_url ?? ops?.mcp_http_url ?? 'Server-backed crawl and ask'}</p>
          </div>
        </div>
        <div className="first-run-controls">
          <label>
            URL
            <input value={firstUrl} onChange={(event) => setFirstUrl(event.target.value)} />
          </label>
          <button onClick={() => void runFirstCrawl()} disabled={firstRunBusy}>
            Crawl
          </button>
          <label>
            Question
            <input value={firstQuestion} onChange={(event) => setFirstQuestion(event.target.value)} />
          </label>
          <button onClick={() => void runFirstAsk()} disabled={firstRunBusy}>
            Ask
          </button>
        </div>
        {firstRunResult && <pre className="result-box">{firstRunResult}</pre>}
      </section>

      <section className="editor-panel">
        <div className="section-heading">
          <div>
            <h2>Config</h2>
            <p>{panelState?.config_path}</p>
          </div>
          <button onClick={() => void saveConfig()}>Save</button>
        </div>
        <textarea value={config} onChange={(event) => setConfig(event.target.value)} spellCheck={false} />
        {message && <p className={message.startsWith('Config saved') ? 'ok' : 'error'}>{message}</p>}
      </section>
    </main>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function UrlCard({ check }: { check: StackUrlCheck }) {
  return (
    <div className={`url-card ${check.status}`}>
      <div>
        <span>{check.label}</span>
        <StatusBadge status={check.status} />
      </div>
      {check.url ? <strong>{check.url}</strong> : <strong>Not configured</strong>}
      <p>{check.detail}</p>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  return <strong className={`status-badge ${status}`}>{status}</strong>;
}
