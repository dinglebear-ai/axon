'use client';

import { useEffect, useMemo, useState } from 'react';

type PanelState = {
  setup_required: boolean;
  config_path: string;
};

type ConfigResponse = {
  path: string;
  raw_toml: string;
};

type OpsResponse = {
  qdrant_url: string;
  tei_url: string;
  collection: string;
  mcp_http_url: string;
};

type SshTarget = {
  alias: string;
  host_name?: string;
  user?: string;
  port?: number;
};

type DeployResult = {
  target: string;
  remote_host: string;
  remote_dir: string;
  public_exposure: boolean;
  qdrant_url: string;
  tei_url: string;
  chrome_remote_url: string;
  config_path: string;
  tunnel_command?: string;
  steps: { name: string; ok: boolean; detail: string }[];
};

const TOKEN_KEY = 'axon-panel-token';

export default function Page() {
  const [token, setToken] = useState('');
  const [password, setPassword] = useState('');
  const [panelState, setPanelState] = useState<PanelState | null>(null);
  const [config, setConfig] = useState('');
  const [ops, setOps] = useState<OpsResponse | null>(null);
  const [targets, setTargets] = useState<SshTarget[]>([]);
  const [selectedTarget, setSelectedTarget] = useState('');
  const [publicExposure, setPublicExposure] = useState(false);
  const [acceptNewHostKey, setAcceptNewHostKey] = useState(false);
  const [deploying, setDeploying] = useState(false);
  const [deployResult, setDeployResult] = useState<DeployResult | null>(null);
  const [targetError, setTargetError] = useState('');
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

    void fetch('/api/panel/setup/targets', { headers: authedHeaders })
      .then(async (res) => {
        if (!res.ok) throw new Error(await res.text());
        return res.json();
      })
      .then((targetData: SshTarget[]) => {
        setTargetError('');
        setTargets(targetData);
        setSelectedTarget((current) => current || targetData[0]?.alias || '');
      })
      .catch((error) => setTargetError(String(error)));
  }, [token, authedHeaders]);

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
    setMessage(res.ok ? 'Config saved' : await res.text());
  }

  async function deployRemote() {
    if (!selectedTarget) {
      setMessage('Select an SSH target');
      return;
    }
    setDeploying(true);
    setDeployResult(null);
    setMessage('');
    const res = await fetch('/api/panel/setup/deploy', {
      method: 'POST',
      headers: authedHeaders,
      body: JSON.stringify({
        target: selectedTarget,
        public_exposure: publicExposure,
        accept_new_host_key: acceptNewHostKey
      })
    });
    setDeploying(false);
    if (!res.ok) {
      setMessage(await res.text());
      return;
    }
    const body = (await res.json()) as DeployResult;
    setDeployResult(body);
    setConfig(await fetch('/api/panel/config', { headers: authedHeaders }).then((cfg) => cfg.json()).then((cfg) => cfg.raw_toml));
    setMessage('Deployment complete');
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

      <section className="deploy-panel">
        <div className="section-heading">
          <div>
            <h2>Remote Deploy</h2>
            <p>
              {targetError ||
                (targets.length ? `${targets.length} SSH target${targets.length === 1 ? '' : 's'}` : '~/.ssh/config')}
            </p>
          </div>
          <button onClick={() => void deployRemote()} disabled={deploying || !selectedTarget}>
            {deploying ? 'Deploying' : 'Deploy'}
          </button>
        </div>
        <div className="deploy-controls">
          <label>
            Target
            <select value={selectedTarget} onChange={(event) => setSelectedTarget(event.target.value)}>
              {targets.map((target) => (
                <option key={target.alias} value={target.alias}>
                  {target.alias}
                  {target.host_name ? ` (${target.host_name})` : ''}
                </option>
              ))}
            </select>
          </label>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={acceptNewHostKey}
              onChange={(event) => setAcceptNewHostKey(event.target.checked)}
            />
            Accept new SSH host key
          </label>
          <label className="checkbox-row">
            <input
              type="checkbox"
              checked={publicExposure}
              onChange={(event) => setPublicExposure(event.target.checked)}
            />
            Public service ports
          </label>
        </div>
        {deployResult && (
          <div className="deploy-result">
            <Metric label="Remote" value={deployResult.remote_host} />
            <Metric label="Qdrant" value={deployResult.qdrant_url} />
            <Metric label="TEI" value={deployResult.tei_url} />
            <Metric label="Chrome" value={deployResult.chrome_remote_url} />
            {deployResult.tunnel_command && <Metric label="Tunnel" value={deployResult.tunnel_command} />}
          </div>
        )}
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
        {message && <p className={message === 'Config saved' ? 'ok' : 'error'}>{message}</p>}
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
