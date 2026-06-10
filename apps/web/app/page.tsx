'use client';

import {
  Activity,
  Ban,
  Braces,
  ClipboardCopy,
  Command,
  FileCog,
  Globe2,
  ListChecks,
  LockKeyhole,
  Play,
  RefreshCw,
  RotateCcw,
  Save,
  Settings2,
  Terminal,
  X
} from 'lucide-react';

import { commandExamples } from './command-format';
import {
  CheckCard,
  CommandResultCard,
  DoctorCard,
  EmptyState,
  JobRow,
  StatusGlyph,
  SubsectionTitle,
  SummaryPill,
  UrlCard,
  overallStatusLabel,
  summarizeChecks
} from './panel-components';
import { jobSummary } from './job-helpers';
import { usePanelData } from './use-panel-data';

export default function Page() {
  const {
    token, setToken,
    password, setPassword,
    message,
    login,
    panelState,
    activePanelTab, setActivePanelTab,
    stack,
    stackLoading,
    stackStatus,
    stackUpdatedAt,
    doctor,
    doctorMessage,
    doctorUpdatedAt,
    urlSummary,
    runtimeChecks,
    skippedHostChecks,
    overallStatus,
    doctorServices,
    doctorSummary,
    axonStatus,
    liveJobs,
    activeJobs,
    statusMessage,
    statusUpdatedAt,
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
    paletteOpen, setPaletteOpen,
    commandInput, setCommandInput,
    commandBusy,
    commandResult,
    commandHistory,
    runCommand,
    refreshDashboard,
    refreshAxonStatus,
    savedMessage,
  } = usePanelData();

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
              window.localStorage.removeItem('axon-panel-token');
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
