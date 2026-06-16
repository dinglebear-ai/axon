'use client';

import { useEffect, useMemo, useState } from 'react';
import {
  TOKEN_KEY,
  type ConfigFile,
  type ConfigResponse,
  type CommandResultView,
  type EnvConfigResponse,
  type PanelCommandResponse,
  type PanelDoctorResponse,
  type PanelState,
  type PanelStatusResponse,
  type PanelTab,
  type SaveConfigResponse,
  type StackResponse
} from './panel-types';
import { formatCommandResponse } from './command-format';
import { collectDoctorServices, collectJobs, doctorCheckSummary, savedMessage } from './job-helpers';
import { mergeStatus, summarizeChecks, summarizeConfig } from './panel-components';

export function usePanelData() {
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [token, authedHeaders]);

  useEffect(() => {
    if (!token) return;
    const timer = window.setInterval(() => void refreshAll({ quiet: true }), 5000);
    return () => window.clearInterval(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
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

  async function refreshDashboard(options: { quiet?: boolean } = {}) {
    await Promise.all([refreshStack(options), refreshDoctor(options)]);
  }

  async function refreshAll(options: { quiet?: boolean } = {}) {
    await Promise.all([refreshStack(options), refreshDoctor(options), refreshAxonStatus(options)]);
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
          const contentType = res.headers.get('content-type') ?? 'unknown';
          setCommandResult({
            ok: false,
            title: 'Command response was invalid',
            subtitle: trimmed,
            rows: [
              { label: 'HTTP status', value: String(res.status) },
              { label: 'Content type', value: contentType }
            ],
            body: `Expected JSON but got: ${String(error)}`,
            raw: body.slice(0, 2000)
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

  return {
    // auth
    token, setToken,
    password, setPassword,
    message,
    login,
    // panel state
    panelState,
    activePanelTab, setActivePanelTab,
    // stack / doctor
    stack,
    stackLoading,
    stackStatus,
    stackUpdatedAt,
    doctor,
    doctorMessage,
    doctorUpdatedAt,
    // derived stack
    urlSummary,
    runtimeChecks,
    skippedHostChecks,
    overallStatus,
    doctorServices,
    doctorSummary,
    // jobs / status
    axonStatus,
    liveJobs,
    activeJobs,
    statusMessage,
    statusUpdatedAt,
    // config
    config, envConfig,
    activeConfigFile, setActiveConfigFile,
    activeConfigPath,
    activeConfigMeta,
    activeConfigValue,
    activeDirty,
    configDirty,
    envDirty,
    updateActiveConfig,
    revertConfig,
    saveConfig,
    // command palette
    paletteOpen, setPaletteOpen,
    commandInput, setCommandInput,
    commandBusy,
    commandResult,
    commandHistory,
    runCommand,
    // refresh
    refreshDashboard,
    refreshAxonStatus,
    // helpers needed in JSX
    savedMessage,
  };
}
