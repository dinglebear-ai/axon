'use client';

import {
  Activity,
  Ban,
  Bot,
  Braces,
  CheckCircle2,
  ClipboardCopy,
  Command,
  Cpu,
  Database,
  ExternalLink,
  FileCog,
  Globe2,
  HelpCircle,
  LockKeyhole,
  ListChecks,
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  Server,
  Settings2,
  Terminal,
  ShieldCheck,
  TriangleAlert,
  X,
  XCircle
} from 'lucide-react';
import type { LucideIcon } from 'lucide-react';
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

type EnvConfigResponse = {
  path: string;
  raw_env: string;
  restart_required: boolean;
};

type SaveConfigResponse = {
  ok: boolean;
  restart_required: boolean;
  message: string;
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

type PanelStatusResponse = {
  payload: {
    local_crawl_jobs?: ServiceJob[];
    local_extract_jobs?: ServiceJob[];
    local_embed_jobs?: ServiceJob[];
    local_ingest_jobs?: ServiceJob[];
    totals?: Record<string, number>;
  };
  text: string;
  totals: Record<string, number>;
};

type ServiceJob = {
  id: string;
  status: string;
  updated_at: string;
  created_at: string;
  kind?: 'crawl' | 'extract' | 'embed' | 'ingest';
  error_text?: string | null;
  url?: string | null;
  target?: string | null;
  source_type?: string | null;
  urls_json?: unknown;
};

type ArtifactHandle = {
  relative_path: string;
  bytes?: number;
};

type PanelCommandResponse = {
  command: string;
  action: unknown;
  result: unknown;
};

type CommandResultView = {
  ok: boolean;
  title: string;
  subtitle: string;
  rows: Array<{ label: string; value: string }>;
  body?: string;
  raw?: string;
  imageUrl?: string;
};

type PanelDoctorResponse = {
  payload: {
    observed_at_utc?: string;
    all_ok?: boolean;
    services?: Record<string, DoctorService>;
    pipelines?: Record<string, boolean>;
    browser_runtime?: {
      selection?: string;
    };
  };
};

type DoctorService = {
  ok?: boolean;
  url?: string | null;
  detail?: string | null;
  model?: string | null;
  collection?: string | null;
  vector_mode?: string | null;
  path?: string | null;
  exists?: boolean;
  command?: string | null;
};

type CheckSummary = {
  ok: number;
  warn: number;
  error: number;
  skipped: number;
  total: number;
};

type ConfigFile = 'toml' | 'env';
type PanelTab = 'dashboard' | 'jobs' | 'configurator';

const TOKEN_KEY = 'axon-panel-token';
const commandExamples = [
  'scrape code.claude.com',
  'crawl code.claude.com',
  'ask How do I create claude code hooks?',
  'extract all the prices from https://example.com/products'
];

const checkIcons: Record<string, LucideIcon> = {
  Chrome: Globe2,
  'Compose assets': FileCog,
  Docker: Server,
  'Docker Compose': Server,
  'Gemini CLI': Bot,
  'MCP/API token': ShieldCheck,
  'NVIDIA runtime': Cpu,
  'OAuth / lab-auth': ShieldCheck,
  Qdrant: Database,
  'TEI / Qwen3': Cpu
};

const urlIcons: Record<string, LucideIcon> = {
  'Chrome control': Globe2,
  'MCP endpoint': ShieldCheck,
  'Panel / readyz': Server,
  'Public URL': Globe2,
  'Qdrant readyz': Database,
  'TEI health': Cpu
};

export default function Page() {
  const [token, setToken] = useState('');
  const [password, setPassword] = useState('');
  const [panelState, setPanelState] = useState<PanelState | null>(null);
  const [config, setConfig] = useState('');
  const [loadedConfig, setLoadedConfig] = useState('');
  const [envConfig, setEnvConfig] = useState('');
  const [loadedEnvConfig, setLoadedEnvConfig] = useState('');
  const [envPath, setEnvPath] = useState('');
  const [activeConfigFile, setActiveConfigFile] = useState<ConfigFile>('toml');
  const [stack, setStack] = useState<StackResponse | null>(null);
  const [stackLoading, setStackLoading] = useState(false);
  const [stackStatus, setStackStatus] = useState('');
  const [stackUpdatedAt, setStackUpdatedAt] = useState('');
  const [axonStatus, setAxonStatus] = useState<PanelStatusResponse | null>(null);
  const [doctor, setDoctor] = useState<PanelDoctorResponse | null>(null);
  const [statusMessage, setStatusMessage] = useState('');
  const [statusUpdatedAt, setStatusUpdatedAt] = useState('');
  const [doctorMessage, setDoctorMessage] = useState('');
  const [doctorUpdatedAt, setDoctorUpdatedAt] = useState('');
  const [activePanelTab, setActivePanelTab] = useState<PanelTab>('dashboard');
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [commandInput, setCommandInput] = useState('');
  const [commandBusy, setCommandBusy] = useState(false);
  const [commandResult, setCommandResult] = useState<CommandResultView | null>(null);
  const [commandHistory, setCommandHistory] = useState<string[]>([]);
  const [message, setMessage] = useState('');

  const authedHeaders = useMemo(
    () => ({
      'content-type': 'application/json',
      'x-axon-panel-token': token
    }),
    [token]
  );

  const urlSummary = useMemo(() => summarizeChecks(stack?.urls ?? []), [stack]);
  const runtimeChecks = useMemo(() => (stack?.checks ?? []).filter((check) => check.status !== 'skipped'), [stack]);
  const skippedHostChecks = useMemo(() => (stack?.checks ?? []).filter((check) => check.status === 'skipped'), [stack]);
  const overallStatus = useMemo(
    () => mergeStatus([urlSummary, summarizeChecks(runtimeChecks)]),
    [urlSummary, runtimeChecks]
  );
  const configMeta = useMemo(() => summarizeConfig(config), [config]);
  const envMeta = useMemo(() => summarizeConfig(envConfig), [envConfig]);
  const configDirty = config !== loadedConfig;
  const envDirty = envConfig !== loadedEnvConfig;
  const activeDirty = activeConfigFile === 'toml' ? configDirty : envDirty;
  const activeConfigPath = activeConfigFile === 'toml' ? panelState?.config_path : envPath;
  const activeConfigMeta = activeConfigFile === 'toml' ? configMeta : envMeta;
  const activeConfigValue = activeConfigFile === 'toml' ? config : envConfig;
  const liveJobs = useMemo(() => collectJobs(axonStatus), [axonStatus]);
  const activeJobs = useMemo(
    () => liveJobs.filter((job) => ['pending', 'running'].includes(job.status)),
    [liveJobs]
  );
  const doctorServices = useMemo(() => collectDoctorServices(doctor), [doctor]);
  const doctorSummary = useMemo(() => doctorCheckSummary(doctorServices), [doctorServices]);

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
        setLoadedConfig(cfg.raw_toml);
      })
      .catch((error) => setMessage(String(error)));

    void fetch('/api/panel/env', { headers: authedHeaders })
      .then(async (res) => {
        if (!res.ok) throw new Error(await res.text());
        return res.json();
      })
      .then((envData: EnvConfigResponse) => {
        setEnvConfig(envData.raw_env);
        setLoadedEnvConfig(envData.raw_env);
        setEnvPath(envData.path);
      })
      .catch((error) => setMessage(String(error)));

    void refreshAll();
  }, [token, authedHeaders]);

  useEffect(() => {
    if (!token) return;
    const timer = window.setInterval(() => void refreshAll({ quiet: true }), 5000);
    return () => window.clearInterval(timer);
  }, [token, authedHeaders]);

  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault();
        setPaletteOpen(true);
      }
      if (event.key === 'Escape') setPaletteOpen(false);
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);

  async function refreshDashboard(options: { quiet?: boolean } = {}) {
    await Promise.all([refreshStack(options), refreshDoctor(options)]);
  }

  async function refreshAll(options: { quiet?: boolean } = {}) {
    await Promise.all([refreshStack(options), refreshDoctor(options), refreshAxonStatus(options)]);
  }

  async function refreshStack(options: { quiet?: boolean } = {}) {
    if (!token) return;
    if (!options.quiet) setStackLoading(true);
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
        setStackUpdatedAt(new Date().toLocaleTimeString());
      } catch (error) {
        setStack(null);
        setStackStatus(`Stack check returned invalid JSON: ${String(error)}`);
      }
    } catch (error) {
      setStack(null);
      setStackStatus(`Stack check failed: ${String(error)}`);
    } finally {
      if (!options.quiet) setStackLoading(false);
    }
  }

  async function refreshAxonStatus(options: { quiet?: boolean } = {}) {
    if (!token) return;
    if (!options.quiet) setStatusMessage('');
    try {
      const res = await fetch('/api/panel/status', { headers: authedHeaders });
      const body = await res.text();
      if (!res.ok) {
        setStatusMessage(body || `Status failed with HTTP ${res.status}`);
        return;
      }
      setAxonStatus(JSON.parse(body) as PanelStatusResponse);
      setStatusUpdatedAt(new Date().toLocaleTimeString());
    } catch (error) {
      setStatusMessage(`Status failed: ${String(error)}`);
    }
  }

  async function refreshDoctor(options: { quiet?: boolean } = {}) {
    if (!token) return;
    if (!options.quiet) setDoctorMessage('');
    try {
      const res = await fetch('/api/panel/doctor', { headers: authedHeaders });
      const body = await res.text();
      if (!res.ok) {
        setDoctorMessage(body || `Doctor failed with HTTP ${res.status}`);
        return;
      }
      setDoctor(JSON.parse(body) as PanelDoctorResponse);
      setDoctorUpdatedAt(new Date().toLocaleTimeString());
    } catch (error) {
      setDoctorMessage(`Doctor failed: ${String(error)}`);
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
    const res = await fetch(activeConfigFile === 'toml' ? '/api/panel/config' : '/api/panel/env', {
      method: 'PUT',
      headers: authedHeaders,
      body:
        activeConfigFile === 'toml'
          ? JSON.stringify({ raw_toml: config })
          : JSON.stringify({ raw_env: envConfig })
    });
    if (!res.ok) {
      setMessage(await res.text());
      return;
    }
    const body = (await res.json()) as SaveConfigResponse;
    setMessage(body.message);
    if (activeConfigFile === 'toml') setLoadedConfig(config);
    else setLoadedEnvConfig(envConfig);
  }

  function revertConfig() {
    if (activeConfigFile === 'toml') setConfig(loadedConfig);
    else setEnvConfig(loadedEnvConfig);
  }

  function updateActiveConfig(value: string) {
    if (activeConfigFile === 'toml') setConfig(value);
    else setEnvConfig(value);
  }

  async function runCommand(command = commandInput) {
    const trimmed = command.trim();
    if (!trimmed) return;
    setCommandBusy(true);
    setCommandResult(null);
    try {
      const res = await fetch('/api/panel/command', {
        method: 'POST',
        headers: authedHeaders,
        body: JSON.stringify({ command: trimmed })
      });
      const body = await res.text();
      if (!res.ok) {
        setCommandResult({
          ok: false,
          title: 'Command failed',
          subtitle: trimmed,
          rows: [{ label: 'HTTP status', value: String(res.status) }],
          body: body || 'No error details returned.'
        });
      } else {
        try {
          setCommandResult(formatCommandResponse(JSON.parse(body) as PanelCommandResponse));
        } catch (error) {
          setCommandResult({
            ok: true,
            title: 'Command completed',
            subtitle: trimmed,
            rows: [],
            body,
            raw: `Response was not JSON: ${String(error)}`
          });
        }
      }
      setCommandHistory((history) => [trimmed, ...history.filter((item) => item !== trimmed)].slice(0, 6));
      await refreshAll({ quiet: true });
    } catch (error) {
      setCommandResult({
        ok: false,
        title: 'Command failed',
        subtitle: trimmed,
        rows: [],
        body: String(error)
      });
    } finally {
      setCommandBusy(false);
    }
  }

  if (!token) {
    return (
      <main className="shell narrow">
        <section className="login-panel">
          <div className="brand-heading">
            <img className="brand-mark" src="/assets/axon-glyph.svg" alt="" aria-hidden="true" />
            <div>
              <p className="eyebrow">Axon Admin</p>
              <h1>{panelState?.setup_required ? 'Setup Wizard' : 'Management Dashboard'}</h1>
              <p className="muted">{panelState?.config_path ?? '~/.axon/config.toml'}</p>
            </div>
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
          <button onClick={() => void login()}>
            <LockKeyhole aria-hidden="true" className="button-icon" />
            Unlock
          </button>
          {message && <p className="error">{message}</p>}
        </section>
      </main>
    );
  }

  return (
    <main className="shell">
      <header className="topbar">
        <div className="brand-heading">
          <img className="brand-mark" src="/assets/axon-glyph.svg" alt="" aria-hidden="true" />
          <div>
            <p className="eyebrow">Axon Admin</p>
            <h1>{panelState?.setup_required ? 'Setup Wizard' : 'Management Dashboard'}</h1>
          </div>
        </div>
        <div className="topbar-actions">
          <button className="command-launch" onClick={() => setPaletteOpen(true)}>
            <Command aria-hidden="true" className="button-icon" />
            Command
            <kbd>⌘K</kbd>
          </button>
          <button
            className="ghost"
            onClick={() => {
              window.localStorage.removeItem(TOKEN_KEY);
              setToken('');
            }}
          >
            <LockKeyhole aria-hidden="true" className="button-icon" />
            Lock
          </button>
        </div>
      </header>

      <nav className="panel-tabs" aria-label="Admin panel sections">
        <button
          className={activePanelTab === 'dashboard' ? 'selected' : ''}
          onClick={() => setActivePanelTab('dashboard')}
        >
          <Activity aria-hidden="true" className="button-icon" />
          Dashboard
        </button>
        <button
          className={activePanelTab === 'configurator' ? 'selected' : ''}
          onClick={() => setActivePanelTab('configurator')}
        >
          <FileCog aria-hidden="true" className="button-icon" />
          Configurator
          {(configDirty || envDirty) && <span className="dirty-dot" aria-label="Modified" />}
        </button>
        <button
          className={activePanelTab === 'jobs' ? 'selected' : ''}
          onClick={() => setActivePanelTab('jobs')}
        >
          <ListChecks aria-hidden="true" className="button-icon" />
          Jobs
          {activeJobs.length > 0 && <span className="dirty-dot" aria-label="Active jobs" />}
        </button>
      </nav>

      {activePanelTab === 'dashboard' && (
        <section className="stack-panel">
          <div className="section-heading">
            <div className="health-title">
              <div className={`status-orb ${overallStatus}`}>
                <StatusGlyph status={overallStatus} />
              </div>
              <div>
                <p className="eyebrow">Dashboard</p>
                <h2>Runtime Health</h2>
                <p>
                  {stack
                    ? `${overallStatusLabel(overallStatus)} · ${stack.server_url} · doctor ${doctor?.payload.all_ok ? 'clear' : 'checking'}`
                    : stackLoading
                      ? 'Checking runtime'
                      : stackStatus || 'Runtime status unavailable'}
                </p>
              </div>
            </div>
            <button className="ghost" onClick={() => void refreshDashboard()} disabled={stackLoading}>
              <RefreshCw aria-hidden="true" className={`button-icon ${stackLoading ? 'spin' : ''}`} />
              {stackLoading ? 'Refreshing' : 'Refresh'}
            </button>
          </div>
          <div className="summary-strip" aria-label="Runtime health summary">
            <SummaryPill label="Service URLs" summary={urlSummary} />
            <SummaryPill label="Dependencies" summary={summarizeChecks(runtimeChecks)} />
            <SummaryPill label="Doctor" summary={doctorSummary} />
            <span className="timestamp">{doctorUpdatedAt || stackUpdatedAt ? `Live ${doctorUpdatedAt || stackUpdatedAt}` : 'Starting live view'}</span>
          </div>
          {(stackStatus || doctorMessage) && <p className="error">{stackStatus || doctorMessage}</p>}
          <div className="runtime-grid">
            <div className="runtime-primary">
              <SubsectionTitle icon={Globe2} title="Service URLs" note="Reachability from this Axon server." />
              {stack?.urls?.length ? (
                <div className="url-list" aria-label="Service URL reachability">
                  {stack.urls.map((urlCheck) => (
                    <UrlCard check={urlCheck} key={urlCheck.label} />
                  ))}
                </div>
              ) : (
                <EmptyState loading={stackLoading} text="No URL checks returned." />
              )}
            </div>
            <div className="runtime-secondary">
              <SubsectionTitle icon={Settings2} title="Runtime Dependencies" note="Server-context checks." />
              {runtimeChecks.length ? (
                <div className="dependency-list">
                  {runtimeChecks.map((check) => (
                    <CheckCard check={check} key={check.label} />
                  ))}
                </div>
              ) : (
                <EmptyState loading={stackLoading} text="No dependency checks returned." />
              )}
              {skippedHostChecks.length > 0 && (
                <div className="skip-strip">
                  <div>
                    <Ban aria-hidden="true" className="heading-icon" />
                    <strong>Host-only checks unavailable</strong>
                  </div>
                  <p>{skippedHostChecks.map((check) => check.label).join(' · ')}</p>
                </div>
              )}
            </div>
          </div>
          <div className="doctor-panel">
            <SubsectionTitle icon={Activity} title="Doctor" note="Live `axon doctor` service report." />
            {doctorServices.length ? (
              <div className="doctor-grid">
                {doctorServices.map((service) => (
                  <DoctorCard service={service} key={service.name} />
                ))}
              </div>
            ) : (
              <EmptyState loading={stackLoading} text="No doctor report returned." />
            )}
          </div>
        </section>
      )}

      {activePanelTab === 'jobs' && (
        <section className="stack-panel">
          <div className="section-heading">
            <div className="health-title">
              <div className={`status-orb ${activeJobs.length ? 'warn' : 'ok'}`}>
                <ListChecks aria-hidden="true" className="status-glyph" />
              </div>
              <div>
                <p className="eyebrow">Jobs</p>
                <h2>Axon Status</h2>
                <p>{activeJobs.length} active jobs · {liveJobs.length} recent rows</p>
              </div>
            </div>
            <button className="ghost" onClick={() => void refreshAxonStatus()} disabled={stackLoading}>
              <RefreshCw aria-hidden="true" className={`button-icon ${stackLoading ? 'spin' : ''}`} />
              Refresh
            </button>
          </div>
          <div className="summary-strip" aria-label="Job summary">
            <SummaryPill label="Active jobs" summary={jobSummary(activeJobs)} />
            <span className="timestamp">{statusUpdatedAt ? `Live ${statusUpdatedAt}` : 'Starting live view'}</span>
          </div>
          {statusMessage && <p className="error">{statusMessage}</p>}
          <div className="status-grid">
            <div className="status-panel">
              <SubsectionTitle icon={Activity} title="Axon Status" note="Queue totals and recent jobs." />
              <div className="job-total-grid">
                {Object.entries(axonStatus?.totals ?? {}).map(([label, value]) => (
                  <div className="job-total" key={label}>
                    <span>{label}</span>
                    <strong>{value}</strong>
                  </div>
                ))}
              </div>
              {liveJobs.length ? (
                <div className="job-list">
                  {liveJobs.slice(0, 10).map((job) => (
                    <JobRow job={job} key={job.id} />
                  ))}
                </div>
              ) : (
                <EmptyState loading={stackLoading} text="No recent jobs returned." />
              )}
            </div>
            <div className="status-panel command-card">
              <SubsectionTitle icon={Command} title="Command Palette" note="Run Axon commands from the browser." />
              <button className="command-open" onClick={() => setPaletteOpen(true)}>
                <Terminal aria-hidden="true" className="button-icon" />
                Open command palette
                <kbd>⌘K</kbd>
              </button>
              <div className="command-examples">
                {commandExamples.map((example) => (
                  <button
                    className="ghost"
                    key={example}
                    onClick={() => {
                      setCommandInput(example);
                      setPaletteOpen(true);
                    }}
                  >
                    {example}
                  </button>
                ))}
              </div>
            </div>
          </div>
        </section>
      )}

      {activePanelTab === 'configurator' && (
        <section className="workbench-shell">
          <div className="workbench-header">
            <div className="section-heading">
              <div>
                <h2>
                  <FileCog aria-hidden="true" className="heading-icon" />
                  Configurator
                </h2>
                <p>Manage `config.toml` and `.env` without leaving the dashboard.</p>
              </div>
            </div>
            <span className="workbench-path">config.toml and .env</span>
          </div>

          <div className="editor-panel">
            <div className="editor-toolbar">
              <div>
                <h2>
                  <FileCog aria-hidden="true" className="heading-icon" />
                  Configurator
                </h2>
                <p>{activeConfigPath}</p>
              </div>
              <div className="editor-actions">
                <button
                  className="ghost"
                  onClick={() => void navigator.clipboard?.writeText(activeConfigPath ?? '')}
                  disabled={!activeConfigPath}
                  title="Copy active config path"
                >
                  <ClipboardCopy aria-hidden="true" className="button-icon" />
                  Copy path
                </button>
                <button className="ghost" onClick={revertConfig} disabled={!activeDirty}>
                  <RotateCcw aria-hidden="true" className="button-icon" />
                  Revert
                </button>
                <button onClick={() => void saveConfig()} disabled={!activeDirty}>
                  <Save aria-hidden="true" className="button-icon" />
                  Save
                </button>
              </div>
            </div>
            <div className="config-tabs" role="tablist" aria-label="Config file">
              <button
                className={activeConfigFile === 'toml' ? 'selected' : ''}
                onClick={() => setActiveConfigFile('toml')}
                role="tab"
                aria-selected={activeConfigFile === 'toml'}
              >
                <FileCog aria-hidden="true" className="button-icon" />
                config.toml
                {configDirty && <span className="dirty-dot" aria-label="Modified" />}
              </button>
              <button
                className={activeConfigFile === 'env' ? 'selected' : ''}
                onClick={() => setActiveConfigFile('env')}
                role="tab"
                aria-selected={activeConfigFile === 'env'}
              >
                <Braces aria-hidden="true" className="button-icon" />
                .env
                {envDirty && <span className="dirty-dot" aria-label="Modified" />}
              </button>
            </div>
            <div className="editor-meta" aria-label="Config metadata">
              <span>
                <Braces aria-hidden="true" className="inline-icon" />
                {activeConfigMeta.lines} lines
              </span>
              <span>{activeConfigMeta.characters} chars</span>
              <span>{activeConfigFile === 'toml' ? 'TOML validated on save' : 'dotenv parsed on save'}</span>
              <span className={activeDirty ? 'meta-dirty' : ''}>{activeDirty ? 'Modified' : 'Saved'}</span>
            </div>
            <textarea
              value={activeConfigValue}
              onChange={(event) => updateActiveConfig(event.target.value)}
              spellCheck={false}
            />
            {message && <p className={savedMessage(message) ? 'ok' : 'error'}>{message}</p>}
          </div>
        </section>
      )}

      {paletteOpen && (
        <div className="palette-backdrop" role="presentation" onMouseDown={() => setPaletteOpen(false)}>
          <section className="command-palette" role="dialog" aria-modal="true" onMouseDown={(event) => event.stopPropagation()}>
            <div className="palette-input-row">
              <Command aria-hidden="true" className="heading-icon" />
              <input
                value={commandInput}
                onChange={(event) => setCommandInput(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === 'Enter') void runCommand();
                }}
                placeholder="scrape code.claude.com"
                autoFocus
              />
              <button className="ghost icon-button" onClick={() => setPaletteOpen(false)} title="Close palette">
                <X aria-hidden="true" className="button-icon" />
              </button>
            </div>
            <div className="palette-body">
              <div className="palette-suggestions">
                {[...commandHistory, ...commandExamples]
                  .filter((item, index, all) => all.indexOf(item) === index)
                  .slice(0, 8)
                  .map((example) => (
                    <button
                      className="palette-suggestion"
                      key={example}
                      onClick={() => {
                        setCommandInput(example);
                        void runCommand(example);
                      }}
                    >
                      <Terminal aria-hidden="true" className="button-icon" />
                      <span>{example}</span>
                      <Play aria-hidden="true" className="inline-icon" />
                    </button>
                  ))}
              </div>
              <button className="command-run" onClick={() => void runCommand()} disabled={commandBusy || !commandInput.trim()}>
                <Play aria-hidden="true" className="button-icon" />
                {commandBusy ? 'Running' : 'Run command'}
              </button>
              {commandResult && <CommandResultCard result={commandResult} />}
            </div>
          </section>
        </div>
      )}
    </main>
  );
}

function savedMessage(message: string) {
  return message.toLowerCase().includes('saved');
}

function SummaryPill({ label, summary }: { label: string; summary: CheckSummary }) {
  const status = summary.error > 0 ? 'error' : summary.warn > 0 ? 'warn' : summary.ok > 0 ? 'ok' : 'skipped';
  const Icon = statusIcon(status);
  const parts = [
    `${summary.ok} ok`,
    summary.warn ? `${summary.warn} warn` : '',
    summary.error ? `${summary.error} error` : '',
    summary.skipped ? `${summary.skipped} skipped` : ''
  ].filter(Boolean);

  return (
    <div className={`summary-pill ${status}`}>
      <Icon aria-hidden="true" className="status-icon" />
      <span>{label}</span>
      <strong>{summary.total ? parts.join(' · ') : 'pending'}</strong>
    </div>
  );
}

function SubsectionTitle({ icon: Icon, title, note }: { icon: LucideIcon; title: string; note: string }) {
  return (
    <div className="subsection-heading">
      <h3>
        <Icon aria-hidden="true" className="heading-icon" />
        {title}
      </h3>
      <p>{note}</p>
    </div>
  );
}

function UrlCard({ check }: { check: StackUrlCheck }) {
  const endpoint = describeEndpoint(check.url);
  const Icon = urlIcons[check.label] ?? Globe2;

  return (
    <div className={`url-card ${check.status}`}>
      <div className="url-service">
        <span>
          <Icon aria-hidden="true" className="card-icon" />
          {check.label}
        </span>
        <small>{endpoint.protocol}</small>
      </div>
      <div className="url-target">
        <strong>{check.url ? endpoint.host : 'Not configured'}</strong>
        {check.url && <code>{endpoint.path}</code>}
      </div>
      <div className="url-state">
        <StatusBadge status={check.status} />
        <p>
          {compactReachabilityDetail(check.detail)}
          {check.url && <ExternalLink aria-hidden="true" className="inline-icon" />}
        </p>
      </div>
    </div>
  );
}

function CheckCard({ check }: { check: StackCheck }) {
  const Icon = checkIcons[check.label] ?? statusIcon(check.status);

  return (
    <div className={`check-card ${check.status}`}>
      <span>
        <Icon aria-hidden="true" className="card-icon" />
        {check.label}
      </span>
      <StatusBadge status={check.status} />
      <p>{check.detail}</p>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const Icon = statusIcon(status);

  return (
    <strong className={`status-badge ${status}`}>
      <Icon aria-hidden="true" className="status-icon" />
      {statusLabel(status)}
    </strong>
  );
}

function StatusGlyph({ status }: { status: string }) {
  const Icon = statusIcon(status);
  return <Icon aria-hidden="true" className="status-glyph" />;
}

function EmptyState({ loading, text }: { loading: boolean; text: string }) {
  return <p className="empty-state">{loading ? 'Checking...' : text}</p>;
}

function CommandResultCard({ result }: { result: CommandResultView }) {
  return (
    <section className={`palette-result ${result.ok ? 'ok' : 'error'}`} aria-live="polite">
      <div className="palette-result-heading">
        <div>
          <p className="eyebrow">{result.ok ? 'Command complete' : 'Command error'}</p>
          <h3>{result.title}</h3>
          <span>{result.subtitle}</span>
        </div>
        <StatusBadge status={result.ok ? 'ok' : 'error'} />
      </div>
      {result.rows.length > 0 && (
        <dl className="palette-result-grid">
          {result.rows.map((row) => (
            <div key={`${row.label}-${row.value}`}>
              <dt>{row.label}</dt>
              <dd>{row.value}</dd>
            </div>
          ))}
        </dl>
      )}
      {result.imageUrl && (
        <img className="palette-result-image" src={result.imageUrl} alt="Screenshot" />
      )}
      {result.body && <p className="palette-result-body">{result.body}</p>}
      {result.raw && <pre className="palette-result-raw">{result.raw}</pre>}
    </section>
  );
}

function DoctorCard({ service }: { service: DoctorService & { name: string } }) {
  const status = service.ok === false ? 'error' : 'ok';
  const detail = service.detail ?? service.model ?? service.vector_mode ?? service.command ?? service.path ?? 'ready';
  const target = service.url ?? service.collection ?? service.path ?? service.command ?? '';

  return (
    <div className={`doctor-card ${status}`}>
      <span>
        <StatusGlyph status={status} />
        {titleLabel(service.name)}
      </span>
      <StatusBadge status={status} />
      {target && <strong>{target}</strong>}
      <p>{detail}</p>
    </div>
  );
}

function JobRow({ job }: { job: ServiceJob }) {
  const rawTarget = job.url ?? job.target ?? jobTargetFromUrls(job.urls_json) ?? job.id;
  const target = compactJobTarget(rawTarget);
  const updatedAt = new Date(job.updated_at).toLocaleTimeString();

  return (
    <div className={`job-row ${job.status}`}>
      <div className="job-row-main">
        <strong title={rawTarget}>{target}</strong>
        <small className="job-row-meta">
          <span>{jobKindLabel(job.kind)}</span>
          <span>{updatedAt}</span>
        </small>
      </div>
      <StatusBadge status={normalizeJobStatus(job.status)} />
    </div>
  );
}

function collectDoctorServices(doctor: PanelDoctorResponse | null): Array<DoctorService & { name: string }> {
  const services = doctor?.payload.services ?? {};
  return Object.entries(services).map(([name, service]) => ({
    name: name.replaceAll('_', ' '),
    ...service
  }));
}

function doctorCheckSummary(services: Array<DoctorService & { name: string }>): CheckSummary {
  return services.reduce(
    (summary, service) => {
      if (service.ok === false) summary.error += 1;
      else summary.ok += 1;
      summary.total += 1;
      return summary;
    },
    { ok: 0, warn: 0, error: 0, skipped: 0, total: 0 }
  );
}

function collectJobs(status: PanelStatusResponse | null): ServiceJob[] {
  if (!status) return [];
  return [
    ...withJobKind('crawl', status.payload.local_crawl_jobs),
    ...withJobKind('extract', status.payload.local_extract_jobs),
    ...withJobKind('embed', status.payload.local_embed_jobs),
    ...withJobKind('ingest', status.payload.local_ingest_jobs)
  ].sort((left, right) => Date.parse(right.updated_at) - Date.parse(left.updated_at));
}

function withJobKind(kind: ServiceJob['kind'], jobs: ServiceJob[] | undefined): ServiceJob[] {
  return (jobs ?? []).map((job) => ({ ...job, kind }));
}

function jobSummary(jobs: ServiceJob[]): CheckSummary {
  return jobs.reduce(
    (summary, job) => {
      if (job.status === 'failed' || job.status === 'canceled') summary.error += 1;
      else if (job.status === 'running') summary.ok += 1;
      else if (job.status === 'pending') summary.warn += 1;
      else summary.skipped += 1;
      summary.total += 1;
      return summary;
    },
    { ok: 0, warn: 0, error: 0, skipped: 0, total: 0 }
  );
}

function normalizeJobStatus(status: string): string {
  if (status === 'completed') return 'ok';
  if (status === 'running') return 'ok';
  if (status === 'pending') return 'warn';
  if (status === 'failed' || status === 'canceled') return 'error';
  return 'skipped';
}

function jobTargetFromUrls(value: unknown): string | null {
  if (Array.isArray(value) && value.length > 0 && typeof value[0] === 'string') return value[0];
  return null;
}

function compactJobTarget(value: string): string {
  if (value.startsWith('/home/axon/.axon/output/domains/')) {
    return compactOutputArtifact(value.replace('/home/axon/.axon/output/domains/', ''));
  }
  if (value.startsWith('/home/axon/.axon/')) return value.replace('/home/axon/.axon/', '~/.axon/');
  if (value.startsWith('/home/jmagar/.axon/')) return value.replace('/home/jmagar/.axon/', '~/.axon/');

  try {
    const url = new URL(value);
    const path = url.pathname === '/' ? '' : url.pathname.replace(/\/$/, '');
    return `${url.hostname}${path}`;
  } catch {
    return value;
  }
}

function compactOutputArtifact(path: string): string {
  const parts = path.split('/').filter(Boolean);
  if (parts.length >= 3) {
    const domain = parts[0];
    const leaf = parts.at(-1);
    return `artifact: ${domain}${leaf ? `/${leaf}` : ''}`;
  }
  return `artifact: domains/${path}`;
}

function jobKindLabel(kind: ServiceJob['kind']): string {
  if (!kind) return 'Job';
  return kind.charAt(0).toUpperCase() + kind.slice(1);
}

function titleLabel(value: string): string {
  return value
    .split(/[\s_-]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function formatCommandResponse(response: PanelCommandResponse): CommandResultView {
  const action = commandVerb(response.command);
  const result = asRecord(response.result);
  const rows: Array<{ label: string; value: string }> = [
    { label: 'Command', value: response.command },
    { label: 'Action', value: titleLabel(action) }
  ];

  const title = commandResultTitle(action, result);
  const subtitle = commandResultSubtitle(action, result);
  rows.push(...commandResultRows(action, result));

  const handle = extractArtifactHandle(result);
  const imageUrl = handle ? `/v1/artifacts/${handle.relative_path}` : undefined;

  return {
    ok: true,
    title,
    subtitle,
    rows: compactRows(rows),
    body: commandResultBody(action, result),
    raw: shouldShowRawResult(action, result) ? JSON.stringify(response.result, null, 2) : undefined,
    imageUrl
  };
}

function commandVerb(command: string): string {
  return command.trim().split(/\s+/, 1)[0]?.toLowerCase() || 'command';
}

function commandResultTitle(action: string, result: Record<string, unknown> | null): string {
  const job = extractTerminalJob(result);
  if (job) {
    const status = String(job.status);
    if (status === 'completed') return `${titleLabel(action)} complete`;
    if (status === 'failed') return `${titleLabel(action)} failed`;
    return `${titleLabel(action)} ${status}`;
  }
  if (action === 'ask') return 'Answer ready';
  if (action === 'status') return 'Status loaded';
  if (action === 'crawl') return hasJobIds(result) ? 'Crawl queued' : 'Crawl complete';
  if (action === 'scrape') return 'Scrape complete';
  if (action === 'screenshot') return 'Screenshot captured';
  if (action === 'extract') return hasJobIds(result) ? 'Extract queued' : 'Extract complete';
  return `${titleLabel(action)} complete`;
}

function commandResultSubtitle(action: string, result: Record<string, unknown> | null): string {
  const target = firstStringField(result, ['url', 'target', 'query', 'question', 'output_dir']);
  if (target) return compactJobTarget(target);
  if (action === 'status') return 'Current SQLite queue state';
  return 'Axon returned a successful response';
}

function commandResultRows(action: string, result: Record<string, unknown> | null): Array<{ label: string; value: string }> {
  if (!result) return [];

  if (action === 'status') {
    const totals = asRecord(result.totals);
    return ['crawl', 'extract', 'embed', 'ingest']
      .map((key) => ({ label: titleLabel(key), value: stringifyScalar(totals?.[key]) }))
      .filter((row) => row.value);
  }

  // Terminal job result (from polling path) — show metrics, not raw job IDs
  const job = extractTerminalJob(result);
  if (job) return terminalJobRows(action, job);

  const rows: Array<{ label: string; value: string }> = [];
  const jobIds = stringArrayField(result, 'job_ids');
  const jobs = arrayField(result, 'jobs');
  const urls = stringArrayField(result, 'urls');
  const outputFiles = stringArrayField(result, 'output_files');
  const predictedPaths = stringArrayField(result, 'predicted_paths');
  const hasArtifact = Boolean(extractArtifactHandle(result));

  if (jobIds.length > 0) rows.push({ label: jobIds.length === 1 ? 'Job ID' : 'Jobs', value: jobIds.join(', ') });
  if (jobs.length > 0) rows.push({ label: 'Jobs', value: String(jobs.length) });
  if (urls.length > 0) rows.push({ label: urls.length === 1 ? 'URL' : 'URLs', value: urls.map(compactJobTarget).join(', ') });
  addStringRow(rows, 'Status', result.status);
  addStringRow(rows, 'Collection', result.collection);
  addStringRow(rows, 'Output', result.output_dir, compactJobTarget);
  addStringRow(rows, 'File', result.output_file, compactJobTarget);
  addNumberRow(rows, 'Pages', result.pages);
  addNumberRow(rows, 'Chunks', result.chunks);
  addNumberRow(rows, 'Count', result.count);

  if (outputFiles.length > 0) rows.push({ label: 'Files', value: outputFiles.map(compactJobTarget).slice(0, 3).join(', ') });
  // Only show predicted paths when there are no real output files and no rendered artifact
  if (predictedPaths.length > 0 && outputFiles.length === 0 && !hasArtifact) {
    rows.push({ label: 'Predicted files', value: predictedPaths.map(compactJobTarget).slice(0, 3).join(', ') });
  }

  return rows;
}

function terminalJobRows(action: string, job: Record<string, unknown>): Array<{ label: string; value: string }> {
  const rows: Array<{ label: string; value: string }> = [];
  const resultJson = asRecord(job.result_json);

  if (job.status === 'failed' && typeof job.error_text === 'string' && job.error_text) {
    rows.push({ label: 'Error', value: job.error_text });
  }

  for (const key of ['pages_crawled', 'docs_embedded', 'chunks_embedded', 'md_created']) {
    const val = resultJson?.[key];
    if (typeof val === 'number' && val > 0) {
      rows.push({ label: titleLabel(key.replaceAll('_', ' ')), value: val.toLocaleString() });
    }
  }

  const elapsedMs = resultJson?.elapsed_ms;
  if (typeof elapsedMs === 'number' && elapsedMs >= 1000) {
    rows.push({ label: 'Elapsed', value: `${(elapsedMs / 1000).toFixed(1)}s` });
  }

  const target = firstStringField(job, ['url', 'target']);
  if (target) {
    rows.push({ label: action === 'ingest' ? 'Source' : 'URL', value: compactJobTarget(target) });
  }

  return rows;
}

function commandResultBody(action: string, result: Record<string, unknown> | null): string | undefined {
  if (!result) return undefined;
  if (action === 'ask') return firstStringField(result, ['answer', 'response', 'text', 'summary']);
  if (action === 'status') return firstStringField(result, ['text']);
  if (action === 'screenshot') {
    const handle = extractArtifactHandle(result);
    if (handle?.bytes) return formatBytes(handle.bytes);
  }
  return firstStringField(result, ['message', 'summary', 'detail']);
}

function shouldShowRawResult(action: string, result: Record<string, unknown> | null): boolean {
  if (!result) return true;
  if (action === 'status' || action === 'ask') return false;
  if (extractTerminalJob(result)) return false;
  if (extractArtifactHandle(result)) return false;
  return commandResultRows(action, result).length <= 2 && !commandResultBody(action, result);
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (value && typeof value === 'object' && !Array.isArray(value)) return value as Record<string, unknown>;
  return null;
}

function extractTerminalJob(result: Record<string, unknown> | null): Record<string, unknown> | null {
  if (!result) return null;
  const job = asRecord(result.job);
  if (!job || typeof job.status !== 'string') return null;
  if (!['completed', 'failed', 'canceled', 'cancelled'].includes(job.status)) return null;
  return job;
}

function extractArtifactHandle(result: Record<string, unknown> | null): ArtifactHandle | null {
  if (!result) return null;
  const handle = asRecord(result.artifact_handle);
  if (!handle || typeof handle.relative_path !== 'string') return null;
  return {
    relative_path: handle.relative_path,
    bytes: typeof handle.bytes === 'number' ? handle.bytes : undefined
  };
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function arrayField(record: Record<string, unknown>, key: string): unknown[] {
  return Array.isArray(record[key]) ? record[key] : [];
}

function stringArrayField(record: Record<string, unknown>, key: string): string[] {
  return arrayField(record, key).filter((item): item is string => typeof item === 'string');
}

function firstStringField(record: Record<string, unknown> | null, keys: string[]): string | undefined {
  if (!record) return undefined;
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.trim()) return value;
  }
  return undefined;
}

function addStringRow(
  rows: Array<{ label: string; value: string }>,
  label: string,
  value: unknown,
  transform: (value: string) => string = (item) => item
) {
  if (typeof value === 'string' && value.trim()) rows.push({ label, value: transform(value) });
}

function addNumberRow(rows: Array<{ label: string; value: string }>, label: string, value: unknown) {
  if (typeof value === 'number') rows.push({ label, value: value.toLocaleString() });
}

function stringifyScalar(value: unknown): string {
  if (typeof value === 'number') return value.toLocaleString();
  if (typeof value === 'string') return value;
  if (typeof value === 'boolean') return value ? 'Yes' : 'No';
  return '';
}

function compactRows(rows: Array<{ label: string; value: string }>): Array<{ label: string; value: string }> {
  const seen = new Set<string>();
  return rows.filter((row) => {
    if (!row.value) return false;
    const key = `${row.label}:${row.value}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function hasJobIds(result: Record<string, unknown> | null): boolean {
  return Boolean(result && stringArrayField(result, 'job_ids').length > 0);
}

function summarizeChecks(checks: StackCheck[]): CheckSummary {
  return checks.reduce(
    (summary, check) => {
      if (check.status === 'ok') summary.ok += 1;
      else if (check.status === 'warn') summary.warn += 1;
      else if (check.status === 'error') summary.error += 1;
      else if (check.status === 'skipped') summary.skipped += 1;

      summary.total += 1;
      return summary;
    },
    { ok: 0, warn: 0, error: 0, skipped: 0, total: 0 }
  );
}

function mergeStatus(summaries: CheckSummary[]): string {
  if (summaries.some((summary) => summary.error > 0)) return 'error';
  if (summaries.some((summary) => summary.warn > 0)) return 'warn';
  if (summaries.some((summary) => summary.ok > 0)) return 'ok';
  return 'skipped';
}

function overallStatusLabel(status: string): string {
  if (status === 'ok') return 'Operational';
  if (status === 'warn') return 'Needs attention';
  if (status === 'error') return 'Degraded';
  return 'Pending checks';
}

function statusIcon(status: string): LucideIcon {
  if (status === 'ok') return CheckCircle2;
  if (status === 'warn') return TriangleAlert;
  if (status === 'error') return XCircle;
  if (status === 'skipped') return Ban;
  if (status === 'oauth') return ShieldCheck;
  if (status === 'agent') return Bot;
  return HelpCircle;
}

function statusLabel(status: string): string {
  if (status === 'ok') return 'Online';
  if (status === 'warn') return 'Degraded';
  if (status === 'error') return 'Offline';
  if (status === 'skipped') return 'Skipped';
  return status;
}

function describeEndpoint(url: string): { protocol: string; host: string; path: string } {
  if (!url) return { protocol: 'unset', host: 'Not configured', path: '' };

  try {
    const parsed = new URL(url);
    const path = `${parsed.pathname}${parsed.search}` || '/';
    return {
      protocol: parsed.protocol.replace(':', '').toUpperCase(),
      host: parsed.host,
      path
    };
  } catch {
    return { protocol: 'custom', host: url, path: '' };
  }
}

function compactReachabilityDetail(detail: string): string {
  return detail.replace(/^reachable;\s*/i, '');
}

function summarizeConfig(raw: string): { lines: number; characters: number } {
  return {
    lines: raw ? raw.split(/\r\n|\r|\n/).length : 0,
    characters: raw.length
  };
}
